# ADR-032: Verify Changeset Presence on Feature Branches

## Status

Proposed

## Context

Cursus's changeset-based release workflow ([ADR-002](002-changeset-recording.md)) relies on developers explicitly creating changeset files via `cursus change` before their work is merged. The `prepare` and `publish` steps ([ADR-003](003-release-command.md), [ADR-004](004-publish-command.md)) consume these changesets downstream, and the `ci` subcommand ([ADR-015](015-ci-managed-release-workflow.md)) uses their presence to decide whether to dispatch to `prepare` or `publish`.

However, there is no built-in mechanism to enforce that a changeset was actually filed before a feature branch is merged. A developer can open a pull request, get it reviewed and approved, and merge it without ever running `cursus change`. The missing changeset is only discovered later -- either when someone notices the changelog is incomplete, or when `ci` silently skips the package because there is nothing to prepare.

CI pipelines commonly enforce branch-level quality gates: linting, tests, type checks. Changeset presence is a natural addition to this set of checks, but Cursus currently offers no command that answers the question "does this branch include a changeset?" with a machine-readable signal.

The `ci` subcommand is not suitable for this purpose. It is designed for the release pipeline on the main branch, where it inspects the repository state to decide between `prepare` and `publish`. A feature-branch verification check has different semantics: it asks whether the branch *contributes* a changeset, not whether changesets are *pending* for release. These are distinct questions with different audiences (PR author vs. release pipeline) and different lifecycle stages (pre-merge vs. post-merge).

## Decision

We will add a `cursus verify` subcommand that checks whether the current git branch has added at least one changeset file to `.cursus/`.

### Subcommand definition

`verify` is a new top-level subcommand alongside `change`, `prepare`, `publish`, `ci`, and `init`. It accepts the following arguments:

- `--base <ref>`: The git ref to compare against. Defaults to `origin/HEAD` if not provided. This follows the same convention established by `cursus change --auto` ([ADR-025](025-auto-changeset-from-conventional-commit.md)), which uses `origin/HEAD` as the base for commit counting and file diffing.

`verify` is always non-interactive. It ignores the `--interactive` / `--no-interactive` global flags and behaves as if `--no-interactive` is set, mirroring the approach taken by `ci` ([ADR-015](015-ci-managed-release-workflow.md)).

### Detection method

The command will use `git diff --name-only --diff-filter=A <base>..HEAD` to list files that have been *added* (not modified, renamed, or deleted) between the base ref and the current HEAD. It then checks whether any of those added files match the pattern `.cursus/*.md`, excluding `README.md` (case-insensitive), consistent with how `Changeset::read_all` filters changeset files.

The `--diff-filter=A` restriction is deliberate. Only files *added* on the branch count as new changesets. Modifications to existing changeset files (e.g., fixing a typo in a changeset that was already on main) do not satisfy the check, because modifying an existing changeset does not represent a new change record for the branch's work. Deletions and renames are similarly excluded.

The command does not parse or validate the changeset file contents. It only checks for the presence of at least one qualifying file. Content validation (valid TOML frontmatter, known package names, valid change types) is the responsibility of `prepare`, which will reject malformed changesets when it consumes them. Keeping `verify` as a simple presence check makes it fast, predictable, and free of dependencies on project configuration -- it does not need to load `config.toml` or enumerate package managers.

### Exit codes

The command uses three distinct exit codes to provide a machine-readable signal:

- **Exit 0**: At least one changeset file was added on this branch. The command prints an informational message listing the detected changeset file(s).
- **Exit 1**: No changeset files were added on this branch. The command prints a message explaining that no changesets were found and suggesting `cursus change` to create one.
- **Exit 2**: An error occurred that prevented the check from completing (e.g., git is not available, the base ref does not exist, the diff command failed). The command prints the error to stderr.

The distinction between exit 1 and exit 2 is critical for CI integration. Exit 1 is a definitive "no changeset" signal that a pipeline can act on (e.g., fail the PR check). Exit 2 indicates that the check itself could not run, which a pipeline may want to handle differently (e.g., retry, alert, or allow to pass rather than blocking on infrastructure failures).

### Output

On success (exit 0), the command logs at `info!` level which changeset files were detected, e.g.:

```
Changeset(s) found on this branch:
  .cursus/scrupulously-affirming-thornbill.md
  .cursus/gently-waving-ibis.md
```

On failure (exit 1), the command logs at `warn!` level:

```
No changesets found on this branch (compared against origin/HEAD).
Run `cursus change` to record a changeset before merging.
```

On error (exit 2), the error is reported via the standard `anyhow` error chain to stderr.

Verbose mode (`-v`) logs the base ref being used and the full `git diff` command for debugging. Silent mode (`-s`) suppresses all output except errors, consistent with [ADR-014](014-verbose-mode.md).

### Dry-run semantics

`--dry-run` has no meaningful effect on `verify`. The command is read-only by nature -- it performs no filesystem writes, no git mutations, and no network requests. When `--dry-run` is passed, the command behaves identically to a normal invocation. This is consistent with [ADR-008](008-dry-run-local-only-guarantee.md): the dry-run guarantee is that no side effects occur, and `verify` already satisfies that guarantee unconditionally.

The command will not log a "dry-run: would have..." message because there is nothing it *would have* done differently. Silently ignoring `--dry-run` is preferable to producing misleading output suggesting the command has a destructive mode.

### No integration with `ci`

The `ci` subcommand will not call `verify` as a pre-flight check. The two commands serve different purposes at different lifecycle stages:

- `verify` runs on feature branches before merge, answering "did this branch contribute a changeset?"
- `ci` runs on the main branch after merge, answering "are there changesets to prepare, or versions to publish?"

Coupling them would conflate these concerns. A feature branch that legitimately has no changeset (CI config, documentation, refactoring) should be able to merge without `ci` refusing to run on main afterward. The changeset enforcement policy belongs to the CI pipeline configuration, not to Cursus's release logic.

### No built-in escape hatch

The command does not provide a mechanism to skip verification (e.g., a `[skip changeset]` marker in commit messages or a `--allow-missing` flag). How users integrate `verify` into their CI workflows -- including when to skip it -- is entirely their responsibility. The clear exit code contract (0 vs 1 vs 2) provides the necessary signal for pipelines to implement whatever branching logic they need.

This keeps the command simple and opinionated: it answers a single question with a clear signal. Policy decisions about when a missing changeset is acceptable belong in the CI configuration, not in Cursus.

### Example CI configurations

A GitHub Actions workflow using `verify` as a PR check:

```yaml
on:
  pull_request:
    branches: [main]

jobs:
  changeset-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - run: cursus verify --base origin/main
```

A configuration that allows certain PRs to skip the check using labels:

```yaml
on:
  pull_request:
    branches: [main]

jobs:
  changeset-check:
    if: "!contains(github.event.pull_request.labels.*.name, 'skip-changeset')"
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - run: cursus verify --base origin/main
```

## Consequences

### Positive

- Provides a first-class, machine-readable way to enforce changeset presence on feature branches, catching missing changesets before merge rather than after.
- The three-tier exit code scheme (0/1/2) gives CI pipelines precise control over how to handle each outcome, enabling sophisticated branching logic without Cursus needing to anticipate every workflow.
- The git-diff detection approach correctly distinguishes "this branch added a changeset" from "changesets exist on the base branch," avoiding false positives from inherited changesets.
- No configuration is required. The command works out of the box with `origin/HEAD` as the default base, and `--base` provides an override for non-standard setups.
- The command is entirely read-only and requires no project configuration, making it safe to run in any context without risk of side effects.

### Negative

- Adds a new subcommand to Cursus's command surface. Users must learn when to use `verify` versus relying on `ci` for changeset-related logic.
- The `origin/HEAD` default requires that the remote HEAD ref is configured correctly, which is not always the case in CI environments with shallow clones or non-standard remote configurations. Users must ensure `fetch-depth: 0` and correct remote setup, or use `--base` explicitly.
- The `--diff-filter=A` restriction means that a developer who moves a changeset file from another location into `.cursus/` (a rename rather than an add) will not satisfy the check. This is an unlikely edge case but could be confusing if encountered.
- No content validation means a branch could pass `verify` with a syntactically invalid changeset file (e.g., missing frontmatter). The error would surface later during `prepare`.

### Neutral

- `verify` does not load `config.toml` or interact with package managers. It is a pure git operation, making it usable even in repositories that have not yet run `cursus init`.
- The command reuses the existing `diff_names` infrastructure from `src/git/operations/mod.rs` with the addition of `--diff-filter=A`, consistent with how `change --auto` uses the same module for file detection.
- Not building in an escape hatch is a deliberate simplification. If demand for a built-in skip mechanism emerges, it can be added in a future ADR without breaking the current contract.

## Alternatives Considered

### Use `Changeset::read_all` to check for changeset presence

Instead of using git diff, simply check whether `.cursus/` currently contains any `.md` changeset files on the filesystem. This was rejected because it cannot distinguish between changesets added by this branch and changesets inherited from the base branch. On a main branch with pending changesets, every feature branch would pass verification even if it contributed nothing, defeating the purpose of the check.

### Integrate into `ci` as a pre-flight check

Have `ci` call `verify` before dispatching to `prepare` or `publish`, refusing to proceed if no changesets are found. This was rejected because `ci` runs on the main branch after merge, where changesets from multiple merged branches may be pending. The question `ci` answers ("are there changesets to act on?") is fundamentally different from what `verify` answers ("did this branch add a changeset?"). Coupling them would prevent legitimate no-changeset PRs from merging without blocking the release pipeline.

### Add a `--verify` flag to `cursus change`

Instead of a separate subcommand, add a `--verify` flag to the existing `change` command that checks for changeset presence without creating one. This was rejected because `change` is a recording command -- its responsibility is creating changesets, not validating their presence. Adding a read-only verification mode to a write-oriented command muddies its semantics. A dedicated subcommand is more discoverable and keeps each command's responsibility clear.

### Build in a commit-message escape hatch

Allow a marker like `[skip changeset]` in any commit message on the branch to cause `verify` to exit 0 even when no changeset is present. This was rejected because it embeds CI policy decisions into Cursus itself. Different teams have different conventions for when changesets are optional, and a single hardcoded marker cannot accommodate them all. The exit code contract provides a clean interface for CI pipelines to implement their own skip logic using whatever mechanism they prefer (labels, file patterns, commit messages, path filters).

### Parse and validate changeset contents

Have `verify` not only check for presence but also parse the changeset files and validate that they reference known packages with valid change types. This was rejected because it would require loading project configuration and enumerating package managers, adding complexity and configuration dependencies to what should be a fast, lightweight check. Content validation is already performed by `prepare` when it consumes changesets. Duplicating that validation in `verify` creates a maintenance burden and risks the two validation paths diverging.
