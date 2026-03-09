# ADR-006: Git Lifecycle Hooks

## Status

Accepted

## Context

Chronicle's release workflow is currently a three-step manual process:

1. **`chronicle release`** — Updates the filesystem (version bumps, changelog generation, changeset deletion)
2. **Manual git operations** — User commits changes, creates tags, and pushes to remote
3. **`chronicle publish`** — Publishes packages to registries and creates GitHub Releases (ADR-004, ADR-005)

ADR-003 explicitly states that Chronicle "intentionally does not handle the commit step" because "users run different CI systems, may want different commit strategies, and may require GPG signing or other policies that Chronicle should not assume."

Without git automation, there is a manual gap between steps 1 and 3. After running `chronicle release`, users must manually:

- Stage the modified files
- Create a commit
- Create git tags for each released package
- Push the commit and tags to the remote

This manual workflow is error-prone and tedious, especially in monorepos where multiple packages are released simultaneously, each requiring its own tag.

While ADR-003's reasoning is sound for general-purpose commit workflows, a significant portion of Chronicle users likely follow a standard pattern: commit all release changes, tag each package, and push to origin. For these users, automating the git lifecycle would reduce friction without imposing unwanted policies.

## Decision

Introduce opt-in git lifecycle hooks that automate git operations after `chronicle release` completes its filesystem modifications.

### Configuration

Add a new `[git]` section to `.chronicle/config.toml`:

```toml
[git]
enabled = false          # bool — master toggle, defaults to false
run_until = "tag"        # "commit" | "tag" | "push"
tag_format = "auto"      # "auto" | "prefixed" | "simple"
extra_files = []         # list of paths — additional files to stage before committing
```

**Field semantics:**

- `enabled`: Master toggle for git integration
  - Default value is derived: `true` if `[github].enabled` is `true`, `false` otherwise
  - Explicit configuration always overrides the default
  - When `false`, all git operations are skipped regardless of other settings
- `run_until`: Controls how far through the git lifecycle Chronicle proceeds. Git operations are sequential — each step implies all previous steps:
  - `"commit"`: Create a release commit only (no tags, no push)
  - `"tag"` (default): Create a release commit and annotated tags
  - `"push"`: Create a release commit, annotated tags, and push to origin
  - This enum design eliminates nonsensical configurations that independent booleans would allow (e.g., tagging without committing). The git lifecycle is inherently sequential, so `run_until` models it as a high-water mark rather than independent toggles.
- `tag_format`: Tag naming strategy, also used by ADR-005's GitHub Releases (see Relationship to ADR-005 below)
  - `"auto"` (default): Use `pkg@version` for multi-package repos, `v{version}` for single-package repos. "Multi-package" is determined by the total project count in the workspace, not the number of packages released in a given run.
  - `"prefixed"`: Always use `pkg@version` format
  - `"simple"`: Always use `v{version}` format (suitable for single-package repos only)
- `extra_files`: Additional file paths (relative to the git root) to stage before committing. Paths are resolved against the git working directory and validated to not escape the repository root. Staging an unmodified file is a no-op in git, so it is safe to list files that may not have changed. This is useful when a custom `lock_command` is configured and Chronicle cannot determine which lock file the command writes. Defaults to an empty list.

### CLI override

`chronicle release` accepts a `--no-git` flag that disables all git operations for that invocation, regardless of the `[git]` configuration:

```text
chronicle release --no-git
```

This is useful for CI pipelines that handle git operations separately, or for debugging a release without side effects beyond the filesystem. It is equivalent to temporarily setting `[git].enabled = false`.

`--no-git` composes with `--dry-run`: `chronicle release --dry-run --no-git` skips both filesystem and git operations, and the dry-run output omits the git summary.

### Behaviour

Git hooks execute **after** `chronicle release` completes all filesystem modifications (version bumps, changelog generation, changeset deletion) but **before** printing the final summary.

The steps run sequentially, up to and including the step specified by `run_until`:

#### 1. Commit (always runs when git is enabled)

- **What**: Create a git commit with all Chronicle-modified files
- **Files staged**: All files modified or deleted by Chronicle during the release:
  - Modified manifest files (`Cargo.toml`, `package.json`)
  - Modified or created `CHANGELOG.md` files
  - Deleted changeset files (`.chronicle/*.md`)
  - Modified lock files (`Cargo.lock`, `package-lock.json`, etc.)
- **Commit message format**:

  ```text
  chore(release): <pkg1>@<version1>, <pkg2>@<version2>
  ```

  Examples:
  - Single package: `chore(release): chronicle-cli@0.2.0`
  - Multiple packages: `chore(release): chronicle-cli@0.2.0, @mscharley/chronicle@1.0.0`
- **Unstaged files**: Chronicle only stages the files it modified. Any other uncommitted changes in the working tree are left untouched.

#### 2. Tag (runs when `run_until` is `"tag"` or `"push"`)

- **What**: Create annotated git tags for each released package
- **Tag name**: Determined by `tag_format` configuration
- **Tag message**: `Release {package} version {version}`
  - Example: `Release chronicle-cli version 0.2.0`
- **Tag target**: The commit created in step 1

#### 3. Push (runs only when `run_until` is `"push"`)

- **What**: Push the commit and tags to the remote
- **Remote**: Uses the default remote (`origin`)
- **Push command**: `git push origin HEAD --follow-tags`
  - Pushes the current branch and all tags reachable from `HEAD`
- Push is opt-in because it is the only step with external side effects. The push function is marked `#[mutants::skip]` since it cannot be meaningfully tested without a real remote.

### Dry-run support

When `chronicle release --dry-run` is invoked:

- All filesystem modifications are skipped (existing ADR-003 behaviour)
- All git operations are skipped
- Summary output includes what **would** have been done, up to the configured `run_until` step

### Error handling

Git operations may fail for various reasons (uncommitted conflicts, no remote configured, authentication required, etc.).

**Chronicle's error handling policy:**

- Filesystem modifications are NOT rolled back on git failure
- The release has already happened from Chronicle's perspective (versions bumped, changelogs written, changesets deleted)
- Git failures are reported clearly with the underlying git error message
- Chronicle exits with a non-zero status code
- Users can inspect the state, fix the issue (e.g., configure remote, resolve conflicts), and manually complete the git operations

**Rationale:** Rolling back filesystem changes after a git failure would be complex and risky. The release operation (bumping versions, writing changelogs) is the primary concern. Git operations are convenience automation. If they fail, users can complete them manually without losing the release work.

### Relationship to ADR-003

ADR-003 (Accepted) states: "Chronicle intentionally does not handle the commit step."

This ADR introduces an **opt-in alternative** to that position while preserving ADR-003's behaviour as the default:

- The default behaviour remains unchanged: git operations are disabled by default, preserving ADR-003's filesystem-only approach
- Users who prefer manual git control (the ADR-003 stance) simply leave `[git].enabled = false` or omit the `[git]` section entirely
- Users who want automation opt in explicitly
- The original reasoning (different CI systems, commit strategies, GPG signing) still holds for users who need those workflows — they continue to manage git manually
- An Errata note has been added to ADR-003 documenting this optional extension

### Relationship to ADR-005

ADR-005 (GitHub Releases) creates GitHub Releases during `chronicle publish`, identified by git tags. The two ADRs share responsibility for tag naming and creation.

**Tag format configuration:**

- `tag_format` lives in `[git]` because git tags are a git concept, not a GitHub-specific concept
- ADR-005's GitHub Releases reference `[git].tag_format` when determining which tag corresponds to each release
- For backward compatibility, if `[github].tag_format` is present but `[git].tag_format` is not, Chronicle uses the value from `[github]` and prints a deprecation warning

**Tag creation:**

- When `[git].enabled = true` and `run_until` is `"tag"` or `"push"`, Chronicle creates git tags during `chronicle release`
- When `[git].enabled = false`, tags must already exist before running `chronicle publish`, and the user or CI is responsible for creating them

## Consequences

### Positive

- Reduces manual steps: users no longer need to remember the correct tag format, stage the right files, or manually push after every release.
- Opt-in by default: the feature is disabled unless explicitly enabled (or `[github].enabled = true`), preserving ADR-003's conservative stance.
- Simple mental model: `run_until` eliminates nonsensical configurations (e.g., tagging without committing) by modelling the git lifecycle as a sequential pipeline with a single stopping point.
- Consistent with existing patterns: uses the same config-driven approach as `[npm]`, `[cargo]`, and `[github]` sections.
- Improves GitHub Releases workflow: when combined with ADR-005, users can go from changesets to published packages with GitHub Releases in a single command sequence: `chronicle release && chronicle publish`.
- Backward compatible: existing configurations with no `[git]` section behave identically to before (no git operations).
- Forward compatible: adding `[git]` configuration to an existing repository opts into the new behaviour without breaking existing workflows.

### Negative

- Couples Chronicle to git: Chronicle now invokes git commands and must handle git failures. Previously, Chronicle was filesystem-only.
- Not suitable for all workflows: users with complex commit requirements (GPG signing, multi-commit strategies, custom commit messages) must continue managing git manually.
- Error handling complexity: git operations can fail in many ways (network errors, authentication, conflicts). Chronicle must detect and report these clearly without losing the release work.
- Less granularity than independent toggles: users cannot, for example, tag without committing. This is by design (such states are nonsensical), but some edge-case workflows may want finer control.

### Neutral

- Unit tests should cover commit message formatting, tag name generation, and dry-run output.
- Integration tests must set up temporary git repositories and verify git operations are performed correctly. Test repos should set `commit.gpgsign = false` and `tag.gpgsign = false` to avoid GPG prompts.
- Error path tests should simulate git failures (no remote, authentication required, merge conflicts) and verify Chronicle reports errors without rolling back the release.

## Alternatives Considered

### Independent boolean toggles (`commit`, `tag`, `push`)

The original design proposed three independent boolean fields: `commit = true`, `tag = true`, `push = false`. This was rejected because git lifecycle steps are inherently sequential -- you cannot tag without a commit to tag, and pushing without commits or tags is meaningless. Independent booleans allow nonsensical states (e.g., `commit = false, tag = true`) that would require runtime validation to reject. The `run_until` enum makes invalid states unrepresentable at the configuration level, is easier to reason about, and requires no cross-field validation logic.

### No git integration (status quo)

Continuing to rely on manual git operations, as established by ADR-003. This was rejected because the manual workflow is error-prone and tedious, especially in monorepos. The opt-in nature of `[git].enabled` preserves the status quo as the default for users who prefer manual control.

## Errata

**2026-03-09**: ADR-015 replaces the `run_until` field (with variants `commit | tag | push`) with a `strategy` field (with variants `push | branch`). The `tag` step described in section "2. Tag" is removed from the release workflow entirely -- tags are now created during `chronicle publish`, not during `chronicle release`. The `commit` variant is also removed; both strategies include committing as an inherent step. The `--no-git` flag, originally defined in this ADR only for `chronicle release`, now also applies to `chronicle publish`. See ADR-015 for the revised git integration model.

**2026-03-09**: ADR-016 renames the `chronicle release` subcommand to `chronicle prepare`. References to `chronicle release` in this ADR now refer to `chronicle prepare`. The behavior is unchanged. See ADR-016 for details.
