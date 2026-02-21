# ADR-006: Git Lifecycle Hooks

## Status

Proposed

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
enabled = true       # bool — defaults to true if [github].enabled, false otherwise
commit = true        # create a commit after release (default: true)
tag = true           # create git tag(s) after commit (default: true)
push = true          # push commit and tags to remote (default: true)
tag_format = "auto"  # "auto" | "prefixed" | "simple"
```

**Field semantics:**

- `enabled`: Master toggle for git integration
  - Default value is derived: `true` if `[github].enabled` is `true`, `false` otherwise
  - Explicit configuration always overrides the default
  - When `false`, all git operations are skipped regardless of other settings
- `commit`, `tag`, `push`: Fine-grained control over which git operations to perform
  - Each defaults to `true` when `enabled` is `true`
  - Independently toggleable to support workflows like "commit and tag locally but don't push"
- `tag_format`: Tag naming strategy, also used by ADR-005's GitHub Releases (see Relationship to ADR-005 below)
  - `"auto"` (default): Use `pkg-name@version` for multi-package repos, `v{version}` for single-package repos
  - `"prefixed"`: Always use `pkg-name@version` format
  - `"simple"`: Always use `v{version}` format (suitable for single-package repos only)

### CLI override

`chronicle release` accepts a `--no-git` flag that disables all git operations for that invocation, regardless of the `[git]` configuration:

```text
chronicle release --no-git
```

This is useful for CI pipelines that handle git operations separately, or for debugging a release without side effects beyond the filesystem. It is equivalent to temporarily setting `[git].enabled = false`.

`--no-git` composes with `--dry-run`: `chronicle release --dry-run --no-git` skips both filesystem and git operations, and the dry-run output omits the git summary.

### Behaviour

Git hooks execute **after** `chronicle release` completes all filesystem modifications (version bumps, changelog generation, changeset deletion) but **before** printing the final summary.

The hooks run in this order:

#### 1. Commit (if `git.commit` is `true`)

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

#### 2. Tag (if `git.tag` is `true`)

- **What**: Create annotated git tags for each released package
- **Tag name**: Determined by `tag_format` configuration
- **Tag message**: `Release <package-name> <version>`
  - Example: `Release chronicle-cli 0.2.0`
- **Tag target**: The commit created in step 1, or `HEAD` if no commit was created
- **Dependency**: Requires a commit to exist. If `git.commit` is `false` and no prior commit exists, tagging fails with a clear error message.

#### 3. Push (if `git.push` is `true`)

- **What**: Push the commit and tags to the remote
- **Remote**: Uses the default remote (`origin`)
- **Push command**: `git push origin HEAD --follow-tags`
  - Pushes the current branch and all tags reachable from `HEAD`
- **Dependency**: Requires something to push (either a new commit or new tags). If both `git.commit` and `git.tag` are `false`, push is a no-op.

### Dry-run support

When `chronicle release --dry-run` is invoked:

- All filesystem modifications are skipped (existing ADR-003 behaviour)
- All git operations are skipped
- Summary output includes what **would** have been committed, tagged, and pushed

Example dry-run output:

```text
Would release:
  chronicle-cli: 0.1.0 -> 0.2.0 (minor)
  @mscharley/chronicle: 0.1.0 -> 0.2.0 (minor)

Would create commit: chore(release): chronicle-cli@0.2.0, @mscharley/chronicle@0.2.0
Would create tags:
  chronicle-cli@0.2.0
  @mscharley/chronicle@0.2.0
Would push to origin
```

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

- When `[git].enabled = true` and `[git].tag = true`, Chronicle creates git tags during `chronicle release`
- When `[git].enabled = false`, tags must already exist before running `chronicle publish`, and the user or CI is responsible for creating them

## Consequences

### Benefits

- **Reduces manual steps**: Users no longer need to remember the correct tag format, stage the right files, or manually push after every release
- **Opt-in by default**: The feature is disabled unless explicitly enabled (or `[github].enabled = true`), preserving ADR-003's conservative stance
- **Granular control**: Users can enable commit but disable push (for local-only workflows), or enable tag but disable commit (for pre-existing commits)
- **Consistent with existing patterns**: Uses the same config-driven approach as `[npm]`, `[cargo]`, and `[github]` sections
- **Improves GitHub Releases workflow**: When combined with ADR-005, users can go from changesets to published packages with GitHub Releases in a single command sequence: `chronicle release && chronicle publish`

### Drawbacks

- **Couples Chronicle to git**: Chronicle now invokes git commands and must handle git failures. Previously, Chronicle was filesystem-only.
- **Not suitable for all workflows**: Users with complex commit requirements (GPG signing, multi-commit strategies, custom commit messages) must continue managing git manually
- **Error handling complexity**: Git operations can fail in many ways (network errors, authentication, conflicts). Chronicle must detect and report these clearly without losing the release work.
- **Implicit behaviour**: When `[github].enabled = true`, `[git].enabled` defaults to `true`, which may surprise users who expected Chronicle to remain filesystem-only

### Compatibility

- **Backward compatible**: Existing configurations with no `[git]` section behave identically to before (no git operations)
- **Forward compatible**: Adding `[git]` configuration to an existing repository opts into the new behaviour without breaking existing workflows

### Testing considerations

- **Unit tests**: Test commit message formatting, tag name generation, and dry-run output
- **Integration tests**: Must set up temporary git repositories and verify git operations are performed correctly
- **Error path tests**: Simulate git failures (no remote, authentication required, merge conflicts) and verify Chronicle reports errors without rolling back the release
