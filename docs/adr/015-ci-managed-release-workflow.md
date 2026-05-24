# ADR-015: CI-Managed Release Workflow

## Status

Accepted

## Context

Cursus's current three-step release workflow ([ADR-003](003-release-command.md), [ADR-004](004-publish-command.md), [ADR-006](006-git-lifecycle-hooks.md)) works well for manual releases: a developer runs `cursus release` locally, commits and pushes (or lets git hooks handle it), then runs `cursus publish`. However, this workflow does not map cleanly onto CI-managed approval flows where releases must go through a pull request review before landing on the main branch.

In a CI-managed workflow, the desired flow is:

1. CI detects pending changesets on the main branch and creates a release PR containing version bumps and changelog updates.
2. A human reviews and approves the PR.
3. Once merged, CI detects that the repository is in a post-release state (versions bumped, changesets consumed) and publishes to registries.

The current `cursus release` command with `[git].strategy = "push"` commits directly to the current branch. There is no mechanism to push release changes to a separate branch for review. Additionally, there is no single CI-friendly entrypoint that can infer what action to take based on the repository's current state -- CI pipelines must be manually wired to call `release` or `publish` at the right time.

GitHub Releases are currently modelled as a post-publish action in [ADR-005](005-github-releases.md), created during `cursus publish` after packages are uploaded to registries. Since GitHub Releases are metadata about git tags rather than registry artifacts, it is more natural to create them at tag-push time, which happens during the publish phase when git operations run.

## Decision

We will extend Cursus with a CI-managed release workflow by introducing a new `cursus ci` subcommand, a `branch` git strategy, and branch naming configuration in the `[git]` section.

### Tags belong to the publish phase

Git tags are semantically equivalent to pushing packages to a registry -- they are both public signals that a version has been officially released. The only distinction is that tags are metadata in the repository rather than registry artifacts. Therefore, tags will always be created during `cursus publish`, alongside registry pushes and GitHub Releases, regardless of which workflow the user follows.

This means the `release` command is strictly concerned with filesystem changes and branch management. It never creates tags. The `publish` command is the single place where all "make this release public" actions happen: publishing to registries, creating annotated tags, pushing tags to origin, and creating GitHub Releases.

### The `cursus ci` subcommand

We will add a `cursus ci` subcommand that serves as a smart CI entrypoint. It inspects the current repository state and dispatches to the appropriate action:

1. **If pending changesets exist** (`.cursus/*.md` files with frontmatter): run the release workflow, pushing changes to a release branch.
2. **If no pending changesets exist and current manifest versions are not yet tagged or published**: run the publish workflow.
3. **If neither condition is met**: exit successfully with a message indicating there is nothing to do.

The state detection logic is:

- **Changesets present**: Cursus already has `read_all_changesets()` which globs `.cursus/*.md`. If this returns a non-empty set, there is work for `release` to do.
- **Post-release state**: After a release PR is merged, changesets have been consumed and versions are bumped. Cursus detects this by checking whether the current manifest versions have been released. Specifically, for each configured package, if no git tag matching the current manifest version exists and the package has not yet been published at that version to its registry, `publish` should run. This combines two signals -- the absence of a tag and the absence of a published version -- to determine that the current version represents an unreleased release. The existing idempotency logic from [ADR-004](004-publish-command.md) still applies: packages already published are skipped during the publish operation itself.

`cursus ci` accepts the same flags as both `release` and `publish` (e.g., `--package`, `--dry-run`, `--branch`). Flags that are irrelevant to the inferred action are ignored.

`cursus ci` is always non-interactive. It ignores the `--interactive` / `--no-interactive` global flags and behaves as if `--no-interactive` is set.

### Git strategies for `release`

The `[git].strategy` field controls how Cursus handles git operations after the release command completes its filesystem modifications. Unlike the previous `run_until` field ([ADR-006](006-git-lifecycle-hooks.md)), which modelled a sequential pipeline of steps, the two strategies are distinct approaches to delivering release changes rather than a progression:

- `push` -- create a release commit and push it directly to the current branch on origin.
- `branch` -- create a release commit on a release branch, push it to origin, and return to the original branch.

The default value for `strategy` is derived at runtime rather than being a static value:

- If `[github].enabled = true`, the default is `"branch"`. When GitHub integration is active, Cursus can create a pull request from the release branch, making the approval flow useful. The value of the `branch` strategy comes from the ability to review release changes before they land.
- Otherwise, the default is `"push"`. Without GitHub integration (or a similar PR mechanism), pushing to a separate branch would leave release changes stranded on a remote branch with no automated way to create a review. Direct push to the current branch is the sensible default in this case.

Explicit configuration of `[git].strategy` always overrides the derived default. This follows the same derivation pattern as `[git].enabled`, which defaults to `true` when `[github].enabled = true` ([ADR-006](006-git-lifecycle-hooks.md)).

When `strategy = "push"`, Cursus:

1. Creates the release commit on the current branch (staging only Cursus-modified files).
2. Pushes the commit to the current branch on origin.

When `strategy = "branch"`, Cursus:

1. Creates and checks out the release branch (`git checkout -b <release-branch>`).
2. Performs all filesystem modifications (version bumps, changelog generation, changeset deletion) on the release branch.
3. Creates the release commit (staging only Cursus-modified files).
4. Pushes the release branch to origin.
5. If `[github].enabled = true`, creates a pull request from the release branch into the original branch (see "Automatic PR creation" below).
6. Checks out the original branch (`git checkout <original-branch>`), returning the working tree to its pre-release state.

The pre-flight check for uncommitted changes (see "Pre-flight check for uncommitted changes" above) has already run before strategy dispatch, so the working tree is guaranteed to be clean at step 1.

This approach avoids `git reset` entirely. No commits are ever made to the working branch -- all release changes happen exclusively on the release branch. When Cursus checks out the original branch in step 6, the working tree is clean and unchanged from the user's perspective.

Users who prefer direct pushes can set `strategy = "push"` explicitly, regardless of whether GitHub integration is enabled.

### Pre-flight check for uncommitted changes

When `[git].enabled = true`, Cursus must refuse to run the prepare command if there are any uncommitted changes in the working tree, whether staged or unstaged. Running with uncommitted changes risks confusion about what ends up in the release commit -- Cursus stages only the files it modifies, but uncommitted changes in the working tree make it ambiguous whether the release commit represents a clean state. For the `branch` strategy specifically, switching branches with uncommitted changes also risks losing work or creating conflicts.

Cursus will check for a clean working tree before proceeding with any filesystem modifications or git operations. If uncommitted changes are detected, Cursus exits with a non-zero status code and a clear error message:

```text
Error: Cannot prepare a release with uncommitted changes in the working tree.
Please commit or stash your changes before running cursus prepare.
```

This check runs regardless of which strategy is configured. When `[git].enabled = false` (or `--no-git` is passed), the check is skipped because Cursus is not performing any git operations and the working tree state is irrelevant.

### Automatic PR creation

When `strategy = "branch"` and `[github].enabled = true`, Cursus will automatically create a GitHub pull request from the release branch into the original branch after pushing. This is part of the `cursus release` workflow (or `cursus ci` in release mode), not `publish`.

The PR title is configurable via a new `[github].pull_request_title` field:

```toml
[github]
enabled = true
pull_request_title = "chore: release"
```

When `pull_request_title` is not configured, the default title is `"Release updates"`.

The PR body will contain the release summary (the same version bump summary that Cursus prints to the terminal). The PR is created as a regular (non-draft) pull request.

PR creation uses the same `GITHUB_TOKEN` authentication as GitHub Releases ([ADR-005](005-github-releases.md)). If PR creation fails (e.g., due to authentication or network errors), Cursus reports the failure but does not roll back the branch push. The release branch exists on the remote and the user can create the PR manually.

When `[github].enabled = false`, no PR is created regardless of the strategy. The release branch is pushed but no further action is taken. This is the scenario where `strategy` defaults to `"push"` -- pushing to a branch without PR creation is rarely useful, which is why `branch` is only the default when GitHub integration is available.

### Tag creation and pushing during `publish`

When `cursus publish` runs and `[git].enabled = true`, it performs git operations after registry publishing completes. The ordering is deliberate:

1. Publishes each package to its configured registry (crates.io, npm).
2. Creates an annotated git tag for each successfully published package using the configured `tag_format` from `[git]`.
3. Pushes all created tags to origin.
4. Creates GitHub Releases (if `[github].enabled`).

Tags are created and pushed *after* registry publishing, not before. This ordering prevents a state detection inconsistency: if tags were pushed first and then registry publishing failed, `cursus ci` would see "tag exists" and incorrectly conclude that publishing is not needed on retry. By creating tags only after successful registry publication, the absence of a tag remains a reliable signal that a version has not been fully released.

Tag creation uses the same `tag_format` configuration and naming conventions established in [ADR-006](006-git-lifecycle-hooks.md). The tag message format remains `Release {package} version {version}`.

If a tag already exists for a given package version, Cursus skips tag creation for that package -- consistent with the idempotency principle from [ADR-004](004-publish-command.md) where "version already exists" errors are treated as success.

Tag creation and pushing is governed solely by `[git].enabled`:

- When `[git].enabled = true`, `publish` creates and pushes tags. This applies regardless of whether `strategy` is `"push"` or `"branch"` -- the strategy field is a release-phase-only setting and has no effect on `publish`.
- When `[git].enabled = false`, the user manages all git operations manually, including tag creation.

### The `--no-git` flag on `release` and `publish`

[ADR-006](006-git-lifecycle-hooks.md) introduced `--no-git` for `cursus release`. We will extend `--no-git` to `cursus publish` as well, with the same semantics: skip all git operations for that invocation, regardless of the `[git]` configuration.

On `release`, `--no-git` skips committing, pushing, branch creation, and PR creation. Cursus performs only filesystem modifications.

On `publish`, `--no-git` skips tag creation and tag pushing. Since GitHub Releases depend on tags existing on the remote, `--no-git` also causes GitHub Release creation to be skipped. Registry publishing (crates.io, npm) proceeds normally because it does not depend on git.

In both cases, `--no-git` is equivalent to temporarily setting `[git].enabled = false` for that single invocation.

`--no-git` composes with `--dry-run`: `cursus publish --dry-run --no-git` skips both registry and git operations, and the dry-run output omits the git summary.

### Branch naming configuration

The release branch name is derived from a configurable prefix combined with the current branch name. The prefix is configurable at three levels, with later levels overriding earlier ones:

1. **Default prefix**: `cursus-release/` (e.g., if on `main`, the release branch is `cursus-release/main`).
2. **Config file**: A new `release_branch_prefix` field in the `[git]` section:

   ```toml
   [git]
   enabled = true
   strategy = "branch"
   release_branch_prefix = "release/"
   ```

   With this configuration and the current branch `main`, the release branch would be `release/main`.

3. **CLI flag**: `--branch <name>` on the `release` and `ci` subcommands provides a full branch name override (not a prefix), bypassing the prefix logic entirely. This is useful in CI where the branch name should be deterministic regardless of the current branch.

The `release_branch_prefix` config field is only meaningful when `strategy = "branch"`. If set when `strategy` is `"push"`, it is ignored. The `--branch` CLI flag similarly has no effect when `strategy` is `"push"`.

The current branch name is resolved at runtime by reading `HEAD`. If HEAD is detached (common in CI), Cursus falls back to appending `detached` to the prefix (e.g., `cursus-release/detached`) and logs a warning suggesting explicit configuration via `--branch`.

### GitHub Releases timing

GitHub Releases will be created during the publish phase after tag creation, which is consistent with [ADR-005](005-github-releases.md). The full publish ordering is: registry publish, then tag creation/push, then GitHub Releases -- all within the same `publish` invocation:

1. `cursus release` (or `ci` in release mode) bumps versions, generates changelogs, pushes to a release branch, and optionally creates a PR.
2. After the release PR is merged (or immediately in manual workflows), `cursus publish` (or `ci` in publish mode) publishes to registries, creates tags, pushes tags, and creates GitHub Releases.

### Interaction with existing workflow

The CI-managed workflow is an extension of, not a replacement for, the existing manual workflow:

- `cursus release` continues to work for filesystem modifications. The `branch` strategy is a new option alongside `push`.
- `cursus publish` continues to work as before, now also responsible for tag creation. It remains the single command that makes a release public.
- `cursus ci` is a convenience wrapper that calls `release` or `publish` based on repo state. Users who prefer explicit control can continue calling `release` and `publish` directly.
- The default `[git].strategy` is derived from whether GitHub integration is enabled: `"branch"` when `[github].enabled = true`, `"push"` otherwise. Users can override this by setting `strategy` explicitly.

### Example CI configuration

A typical GitHub Actions workflow using `cursus ci`:

```yaml
on:
  push:
    branches: [main]

jobs:
  cursus:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - run: cursus ci --branch cursus-release/main
```

This single workflow handles both scenarios: when changesets are present it creates a release PR branch (and opens a PR if GitHub is enabled), and when a release PR has been merged it publishes packages (creating tags and GitHub Releases).

### Dry-run support

`cursus ci --dry-run` reports which action would be taken (release or publish) and what that action would do, without performing any filesystem writes, git operations, or remote operations. This is consistent with [ADR-008](008-dry-run-local-only-guarantee.md).

## Consequences

### Positive

- CI pipelines can use a single `cursus ci` command instead of manually wiring conditional logic to determine whether to release or publish.
- Release changes go through the same PR review process as all other code changes, improving auditability and preventing accidental releases.
- The `branch` strategy keeps the developer's working branch clean by performing all release changes on a separate branch. No commits are made to the working branch, and no `git reset` is required.
- The pre-flight check for uncommitted changes ensures every release commit represents a clean state, catching the problem before any modifications are made. For the `branch` strategy it additionally prevents data loss when switching branches.
- Branch naming is configurable via prefix (config) and full name override (CLI), giving users flexibility without requiring configuration for common cases.
- The existing manual workflow is entirely unaffected. All current configurations and command invocations continue to work identically.
- Tags are created after registry publishing, ensuring that the absence of a tag is a reliable signal for `cursus ci` state detection. Tags are never orphaned for versions that failed to publish.
- Tag creation during `publish` means tags are never orphaned on a branch that gets rejected during PR review. Tags only exist for versions that are actually published.
- Extending `--no-git` to `publish` gives users a consistent escape hatch for skipping git operations across all commands.
- Automatic PR creation when `[github].enabled = true` completes the CI workflow end-to-end without requiring additional CI tooling for the PR step.

### Negative

- The `cursus ci` subcommand adds complexity to Cursus's command surface. Users must understand when to use `ci` versus explicit `release`/`publish` commands.
- State detection for the publish phase requires Cursus to query both git tags and registries, which adds a network dependency to what was previously a local-only detection step. If registry queries fail, `ci` cannot determine whether to publish.
- Detached HEAD in CI environments requires fallback logic for branch naming, adding an edge case that must be handled and tested.
- Moving tag creation from `release` to `publish` changes the existing contract from [ADR-006](006-git-lifecycle-hooks.md) where tags were created during the release step. Users who relied on tags existing after `release` but before `publish` will need to adjust their workflows.
- Both strategies require a clean working tree when git is enabled, which may be inconvenient for developers who want to prepare a release while they have work in progress. They must stash or commit changes first.
- Automatic PR creation couples Cursus more tightly to the GitHub API. Projects not using GitHub cannot benefit from the `branch` strategy's full workflow without external tooling to create PRs.

### Neutral

- The `strategy` field replaces `run_until` in the `[git]` section. Since the two values (`push` and `branch`) are distinct strategies rather than steps in a progression, the new name better reflects the semantics.
- `cursus ci` is syntactic sugar over `release` and `publish`. It does not introduce new release mechanics, only new orchestration logic.
- The release branch is a standard git branch. Apart from optional PR creation when GitHub is enabled, Cursus does not add labels, assign reviewers, or interact with code review platform features beyond creating the PR itself. Additional PR configuration is left to CI tooling.

## Alternatives Considered

### Tags created during the release step

Under this design, annotated tags would be created as part of `cursus release` within the git lifecycle (e.g., a `tag` step between `commit` and `push`, or folded into the `branch` strategy). Tags would then exist on the remote before `cursus publish` runs.

This was rejected because tags are semantically equivalent to publishing -- they are public signals that a version has been officially released. Creating tags during `release` means tags can exist for versions that have not been published to registries, or worse, for versions on a release branch that is ultimately rejected during PR review. By deferring tag creation to `publish`, Cursus ensures that tags only exist for versions that are actually released. This also simplifies state detection for `cursus ci`: the absence of tags for current manifest versions is one of the signals that publishing is needed.

### Tags pushed before registry publishing

Under this ordering, `cursus publish` would create and push tags first, then publish to registries. This was rejected because it creates a state detection inconsistency: if tags are pushed but registry publishing subsequently fails, `cursus ci` would see the existing tags and incorrectly determine that no publishing is needed on retry. By pushing tags only after successful registry publication, the tag remains a reliable signal that the version has been fully released.

### State file instead of version-based detection

Rather than comparing manifest versions against tags and published versions, Cursus could write a `.cursus/release-state.json` file during `release` that records which packages were bumped and to which versions. The `ci` command would read this file to determine whether to publish.

This was rejected because it introduces a new stateful artifact that must be committed, tracked, and cleaned up. It also creates a consistency risk: if the state file gets out of sync with the actual repository state (e.g., someone manually publishes a package), the `ci` command would make incorrect decisions. Version-based detection is derived from the repository's actual state and does not require additional artifacts.

### Commit message convention for state detection

Detect post-release state by looking for a commit message matching the `chore(release):` pattern from [ADR-006](006-git-lifecycle-hooks.md). If the latest commit matches, infer that publishing is needed.

This was rejected because commit message parsing is fragile. Users may amend, squash, or reword commits. Merge commits from PRs may not preserve the original message format. Version-based detection (checking tags and registry state against manifest versions) is more robust because it relies on actual published state rather than commit metadata that can be rewritten.

### Commit on working branch then reset HEAD

The original design for the `branch` strategy committed the release changes on the current working branch, pushed that commit to the release branch, and then ran `git reset` to undo the commit on the working branch. This was rejected because it is less safe than the adopted approach: it temporarily modifies the working branch's history, and if Cursus crashes or is interrupted between the commit and the reset, the working branch is left in a dirty state with a release commit that was never intended to land there. The adopted approach (checkout release branch, commit there, checkout back) never makes commits on the working branch at all, eliminating this risk.

### No automatic PR creation (leave to external tooling)

Cursus could push the release branch and leave PR creation entirely to external CI tooling (e.g., `gh pr create`, platform-specific actions). This was considered but rejected as the default because it leaves the CI workflow incomplete: the `branch` strategy's value proposition is the approval flow, and without PR creation the user must wire up additional CI steps. By creating the PR automatically when `[github].enabled = true`, Cursus provides a complete end-to-end workflow. Users who need custom PR configuration (specific reviewers, labels, draft status) can disable GitHub integration and use their own tooling, or use `--no-git` on the release step and create the PR externally.

## Errata

### 2026-05-24: Detached HEAD now hard-errors instead of falling back

The "Branch naming configuration" section above states that when HEAD is detached, Cursus "falls back to appending `detached` to the prefix (e.g., `cursus-release/detached`) and logs a warning suggesting explicit configuration via `--branch`." This is now functionally incorrect. Under the `branch` strategy, a detached HEAD is treated as a hard error: `prepare` bails during its preflight checks — before any commit, checkout, or push — with a message explaining that the branch strategy needs a current branch to use as the release base and to return to afterward, and suggesting the user check out a branch or switch to the `push` strategy. The synthetic `cursus-release/detached` branch name is no longer ever computed. (The deprecated [ADR-047](047-configurable-release-target-branch.md) anticipated this hard-error stance while reasoning about per-branch release targets, but it was never implemented; the change was made directly in the prepare git lifecycle and is not governed by a separate ADR.)
