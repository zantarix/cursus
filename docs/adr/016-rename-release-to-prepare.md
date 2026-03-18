# ADR-016: Rename `release` Subcommand to `prepare`

## Status

Accepted

## Context

Cursus's three-step release workflow has evolved across several ADRs ([ADR-003](003-release-command.md), [ADR-006](006-git-lifecycle-hooks.md), [ADR-015](015-ci-managed-release-workflow.md)) into a clear separation of concerns: the first step prepares release artifacts (version bumps, changelogs, changeset deletion, commits, branch management), and the second step makes them public (registry publishing, tag creation, GitHub Releases). The subcommands that drive these steps are `cursus release` and `cursus publish`.

The name `release` for the first step is a misnomer. The `release` subcommand does not release anything -- it prepares the repository for a release. The actual releasing happens during `publish`, which pushes packages to registries, creates tags, and creates GitHub Releases. [ADR-015](015-ci-managed-release-workflow.md) made this distinction even sharper by stating "the `release` command is strictly concerned with filesystem changes and branch management" and "`publish` is the single place where all 'make this release public' actions happen."

The consequence is a confusing user-facing vocabulary. The phrase "`release` prepares, `publish` releases" is an accurate description of the current architecture, but it is counterintuitive: users reasonably expect a command named `release` to perform a release. This naming friction will compound as Cursus gains adoption, appearing in documentation, CI configurations, error messages, and support conversations. Renaming the subcommand now, before Cursus reaches a stable release, eliminates this source of confusion permanently.

## Decision

We will rename the `cursus release` subcommand to `cursus prepare`.

### What `prepare` does

`cursus prepare` performs exactly the same operations as the current `release` subcommand. No behavior changes:

1. Gathers pending changesets from `.cursus/*.md`.
2. Aggregates changes per package, determining the highest bump level.
3. Reads current versions from manifest files.
4. Computes next versions using semver bumping rules.
5. Writes new versions to manifest files and updates lock files.
6. Generates changelog entries.
7. Deletes consumed changeset files (with scoped rewriting per [ADR-010](010-scoped-release-changeset-consumption.md)).
8. When `[git].enabled = true`: commits changes, pushes to the current branch or a release branch (per `[git].strategy`), and optionally creates a PR (per [ADR-015](015-ci-managed-release-workflow.md)).
9. Prints a summary.

All existing flags are preserved: `--dry-run`, `--no-git`, `--package/-p`, `--branch`, `--no-interactive`.

### How `cursus ci` is affected

`cursus ci` internally dispatches to either `prepare` or `publish` based on repository state. The state detection logic is unchanged ([ADR-015](015-ci-managed-release-workflow.md)):

- If pending changesets exist, run `prepare`.
- If no pending changesets exist and current manifest versions are untagged/unpublished, run `publish`.
- Otherwise, exit successfully.

The only change is the internal function name and any user-facing output that references the action being taken (e.g., `"Running prepare phase..."` instead of `"Running release phase..."`).

### CLI help text

The top-level `--help` output will list `prepare` instead of `release`:

```text
Usage: cursus <COMMAND>

Commands:
  init       Initialize Cursus in the current repository
  change     Record a changeset (default command)
  prepare    Prepare a release: bump versions, generate changelogs, manage branches
  publish    Publish packages to registries, create tags and GitHub Releases
  ci         Smart CI entrypoint: infers whether to prepare or publish
```

The `prepare` subcommand's help text will describe its purpose without using the word "release" for the command itself, while still using "release" to describe the broader concept where appropriate (e.g., "Prepare a release by bumping versions...").

### Knock-on naming changes

The rename of the subcommand does not require renaming configuration fields or git concepts that use "release" in a broader sense:

- **`release_branch_prefix`** (`[git]` config field): Retained as-is. The branch carries release changes, regardless of which subcommand created it. The prefix `cursus-release/` describes the branch's purpose (it is a release branch), not the subcommand that created it.
- **Prepare commit message** ([ADR-006](006-git-lifecycle-hooks.md)): Now a static configurable string via `[git].prepare_commit_message`, defaulting to `ci(release): version packages`. The previous dynamic format (`chore(release): <pkg1>@<version1>, ...`) has been replaced. No template interpolation is supported -- the configured value is used verbatim. The commit message still describes what the commit contains (release changes), not which subcommand produced it.
- **`[github].pull_request_title`** ([ADR-015](015-ci-managed-release-workflow.md)): The default value `"chore: release"` is retained. PR titles describe the content of the PR (a release), not the CLI command.
- **`Release {package} version {version}` tag message** ([ADR-006](006-git-lifecycle-hooks.md)): Retained. Tags describe releases.

The guiding principle is that "release" as a *noun* (describing what is being prepared) remains correct throughout configuration and git metadata. Only "release" as a *verb* (the subcommand action) is misleading, and that is what this rename addresses.

### Non-interactive CLI flags for tests

The CLAUDE.md testing documentation references `release: --dry-run, --package/-p`. This will be updated to reference `prepare` instead. Integration tests that invoke `cursus release` will be updated to use `cursus prepare`.

## Consequences

### Positive

- The subcommand name accurately describes its function: `prepare` prepares, `publish` publishes. There is no longer a confusing gap between the command name and its behavior.
- The vocabulary is self-documenting. New users can understand the workflow from the command names alone: `change` (record what changed), `prepare` (prepare a release), `publish` (make it public).
- Renaming before a stable release avoids a breaking change later. There are no external users relying on `cursus release` in CI scripts or documentation.
- The three-command mental model (`change` -> `prepare` -> `publish`) is clearer than (`change` -> `release` -> `publish`), where the distinction between `release` and `publish` must be learned.

### Negative

- Internal code references (`src/cli/release.rs`, test files, function names like `run_release()`) will need renaming. This is a moderate amount of mechanical churn.
- The word "prepare" is less evocative than "release" for marketing and first-impression purposes. A user scanning a list of CLI tools might find `cursus release` more immediately appealing than `cursus prepare`. However, accuracy is more important than marketing appeal for a CLI subcommand name.

### Neutral

- The `release` subcommand name could be preserved as a hidden alias (accepted by clap but not shown in `--help`) for a transition period or for backward compatibility with any early adopters. Whether to include this alias is an implementation detail, not an architectural decision.
- This rename does not affect the `cursus ci` subcommand's external interface. `ci` is the recommended entrypoint for automated workflows and abstracts over both `prepare` and `publish`.

## Alternatives Considered

### Keep `release` and add documentation

Continue using `cursus release` but add prominent documentation explaining that `release` does not actually release packages. This was rejected because no amount of documentation overcomes a misleading command name. Users form expectations from names before they read documentation, and the mismatch would be a recurring source of confusion in issues, discussions, and onboarding.

### Keep `release` as a visible alias alongside `prepare`

Register both `cursus release` and `cursus prepare` as first-class subcommands, shown in `--help`, pointing to the same implementation. This was rejected because having two names for the same command doubles the documentation burden and creates ambiguity about which name is canonical. Users would encounter both names in different contexts (CI scripts, tutorials, stack overflow answers) and wonder if they differ. A single canonical name with an optional hidden alias is cleaner.

### Use `stage` instead of `prepare`

Name the subcommand `cursus stage`, evoking the idea of staging changes for release. This was rejected because `stage` has a strong existing meaning in the git ecosystem (`git add` stages files), and using it for a different concept in a git-adjacent tool would create confusion. A user might expect `cursus stage` to interact with the git staging area.

### Use `cut` instead of `prepare`

Name the subcommand `cursus cut`, as in "cut a release." This was rejected because `cut` is jargon that is not self-explanatory to users unfamiliar with the idiom. `prepare` is a plain English word whose meaning is immediately clear. Additionally, `cut` implies finality (the release has been cut), when in fact the subcommand only prepares changes that still need to be published.

### Use `bump` instead of `prepare`

Name the subcommand `cursus bump`, focusing on the version bump as the primary action. This was rejected because version bumping is only one of several things the command does. It also generates changelogs, deletes changesets, commits changes, pushes to branches, and optionally creates PRs. Naming it after a single sub-operation understates its scope. Furthermore, `bump` does not pair well with `publish` in the workflow narrative: "bump then publish" omits the changelog and branch management aspects.
