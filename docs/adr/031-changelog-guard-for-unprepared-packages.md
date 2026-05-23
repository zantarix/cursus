# ADR-031: Guard Publish and CI Against Never-Prepared Packages Using CHANGELOG.md

## Status

Accepted

## Context

The `cursus ci` subcommand ([ADR-015](015-ci-managed-release-workflow.md)) auto-detects repository state and dispatches to either `prepare` or `publish`. It detects "post-release, pre-publish" state by checking whether each package's expected git tag for its current manifest version is missing. When a tag is missing, `ci` infers that the package has been prepared but not yet published, and dispatches to the publish workflow.

In normal usage, when a developer adds a new package to a monorepo they are also expected to file a changeset documenting its creation. If they do, `ci` will detect the pending changeset and dispatch to `prepare` -- not `publish` -- so the tag-missing heuristic is never evaluated for an unprepared package in the first place. This is the primary mechanism that keeps new packages on the correct path.

However, there are edge cases where a new package can exist in the workspace without a pending changeset: the developer may forget to file one, or a prior scoped `prepare` run may have consumed changesets without including this package. In these situations, the tag-missing heuristic breaks down. The package has a version in its manifest (e.g., `0.1.0`) but no corresponding git tag -- and never will have one until it goes through the full prepare-then-publish cycle. Because the tag is perpetually absent, `ci` would trigger `publish` on every run. The publish would then fail confusingly: the package has no changelog, no changeset history, and no registry presence that matches expectations.

A similar problem exists when `cursus publish` is invoked directly. A never-prepared package that passes the private-package check ([ADR-007](007-honor-private-packages-during-publish.md)) would be handed to the package manager for publishing, producing opaque failures unrelated to the actual problem (the package was never prepared).

The project needs a secondary safeguard: a reliable, local, zero-cost signal that distinguishes "package has been through prepare at least once" from "package has never been prepared," so that when the normal changeset-based flow is bypassed, Cursus fails with a clear, actionable message rather than an opaque registry error.

## Decision

We will use the presence of `CHANGELOG.md` in a package's directory as a secondary guard signal for whether a package has ever been prepared. The primary defence is the normal changeset-based flow: pending changesets cause `ci` to dispatch to `prepare`, keeping new packages on the correct path. This guard catches the edge cases where that flow was bypassed. Two guard points enforce it:

### ci tag-check filter

In `src/cli/ci.rs`, when building the set of packages whose missing tags indicate a publish is needed, packages without a `CHANGELOG.md` file in their directory will be excluded from the tag-missing predicate. They will return `false` from the check, causing `ci` to treat them as though their tags are present. This prevents `ci` from falsely dispatching to `publish` for never-prepared packages.

### publish skip guard

In `src/cli/publish/mod.rs`, during the `publish_projects` loop, after silently skipping private packages, public packages without a `CHANGELOG.md` file will be warned about and skipped. The warning message will direct the user to run `cursus prepare` first with an appropriate changeset.

Critically, these skipped packages will **not** be added to the `blocked` set that tracks publish failures. When a package fails to publish, its transitive dependents are blocked to avoid cascading registry errors from missing dependencies. Unprepared packages are different: they are skipped before any publish attempt occurs, so there is no registry failure to propagate. Their dependents may still be independently publishable -- for example, if an older compatible version of the unprepared package exists on the registry. If a dependent genuinely requires the unprepared package, its own publish will fail at the registry level, and that failure will correctly block further transitive dependents through the existing mechanism.

### Why CHANGELOG.md is the right signal

`CHANGELOG.md` satisfies every requirement for this guard:

- **Always created by prepare**: `cursus prepare` unconditionally creates or updates `CHANGELOG.md` in each package directory. There is no opt-out mechanism.
- **Persistent**: Unlike changeset files, which are consumed and deleted by `prepare`, the changelog is a permanent artifact.
- **Local**: No network access is required to check for the file, consistent with the project's offline-first design for `ci` state detection ([ADR-004](004-publish-command.md)).
- **Human-meaningful**: The file is not a hidden marker or metadata; it is a standard artifact that developers expect to find in a published package.

## Consequences

### Positive

- Provides a safety net for the edge case where the normal changeset-based flow is bypassed, eliminating a potential infinite-loop failure mode where `ci` would attempt to publish never-prepared packages on every run.
- The `publish` command now gives an actionable warning ("run `cursus prepare` first") instead of forwarding an opaque registry error to the user.
- No new files, configuration fields, or network dependencies are introduced. The guard piggybacks on an artifact that `prepare` already creates.
- Not blocking dependents of unprepared packages preserves maximum publishability in monorepos where some packages are ready and others are not.

### Negative

- Creates a coupling between `publish`/`ci` and the filesystem convention that `prepare` creates `CHANGELOG.md`. If `prepare` ever gains an opt-out for changelog generation, this guard would produce false negatives (treating prepared packages as unprepared).
- A user who manually creates `CHANGELOG.md` without running `prepare` could bypass the guard. This is an unlikely scenario and would still result in a normal publish attempt, which would succeed or fail on its own merits.
- The two guard points (in `ci.rs` and `publish/mod.rs`) must stay in sync conceptually, though they serve different purposes (state detection vs. runtime skip).

### Neutral

- The guard only applies to the *first* prepare cycle. Once a package has been prepared at least once, `CHANGELOG.md` exists permanently and the guard becomes transparent.
- Private packages continue to be handled by the existing silent-skip logic from [ADR-007](007-honor-private-packages-during-publish.md). The changelog guard runs after the private-package check and only applies to public packages.

## Alternatives Considered

### Check for existing changeset history

Changesets are consumed by `prepare` and deleted, so there is no persistent record of whether a package has ever had changesets. A package that went through `prepare` would be indistinguishable from one that never did, making this signal unreliable.

### Query the package registry

Checking whether a package version exists on crates.io or npm would definitively answer whether it has been published. However, this adds a network dependency to what is designed to be a local detection step. It would be inconsistent with the offline-first philosophy established in [ADR-004](004-publish-command.md) for `ci` state detection, and would introduce latency, rate-limiting concerns, and failure modes unrelated to the actual decision being made.

### Add an explicit "prepared" marker file

A dedicated file (e.g., `.cursus/prepared-packages.json`) could track which packages have been through `prepare`. This would require defining a new file format, ensuring it is committed to version control, and handling consistency edge cases (e.g., the file disagreeing with reality). `CHANGELOG.md` already serves as a reliable proxy with zero additional infrastructure, making a dedicated marker file unnecessary overhead.

## Errata

### 2026-04-27: Guard now applies to all releasable packages, not only public ones

The Decision section's framing of the guard as running "after silently skipping private packages" and applying to "public packages", and the matching Neutral bullet, are no longer accurate. After [ADR-043](043-publish-private-packages-to-github-releases.md) introduced tag-only release for private packages opted in via `[git].publish_private_packages`, a subsequent refactor consolidated `publish_projects` gating around `Project::is_releasable_under` and `Project::is_prepared_for_release`, so the changelog guard now applies to all *releasable* projects — including those opt-in private packages — before they receive git tags or GitHub Releases. Truly private packages (those not listed in `publish_private_packages`) continue to be silently excluded prior to the guard per [ADR-007](007-honor-private-packages-during-publish.md); the guard's semantics and rationale are otherwise unchanged.
