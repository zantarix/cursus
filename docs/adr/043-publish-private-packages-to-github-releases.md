# ADR-043: Allow Private Packages to Publish via Git Tags and GitHub Releases

## Status

Accepted (2026-04-06)

## Context

[ADR-007](007-honor-private-packages-during-publish.md) established that packages marked as private by their upstream package manager (`"private": true` in npm, `publish = false` in Cargo) are silently skipped during `cursus publish`. This works well for packages that are genuinely internal and need no release artifacts at all. However, a significant class of private packages -- such as GitHub Actions, CLIs distributed via GitHub Releases, or other git-tag-distributed software -- need the non-registry parts of the publish workflow: git tags and GitHub Releases.

Today, a GitHub Action repository with `"private": true` in `package.json` gets version bumps and changelogs from `cursus prepare`, but `cursus publish` silently skips it entirely. No git tag is created, no GitHub Release is produced. The user must manually create tags and releases outside of Cursus, defeating the purpose of the automated workflow established in [ADR-015](015-ci-managed-release-workflow.md) and [ADR-005](005-github-releases.md).

The `cursus ci` subcommand ([ADR-015](015-ci-managed-release-workflow.md)) detects post-prepare state by checking for missing git tags against current manifest versions. Because private packages have `CHANGELOG.md` files (created by `prepare`), they pass the [ADR-031](031-changelog-guard-for-unprepared-packages.md) changelog guard and naturally participate in tag-missing detection. When such a package lacks a tag for its current version, `ci` dispatches to the publish workflow. This is the correct and expected behavior -- `ci` does not need modification.

The core tension is that [ADR-007](007-honor-private-packages-during-publish.md) treats "private" as a binary signal meaning "do not publish at all," when in practice it means "do not publish to a registry." These packages still need git tags and GitHub Releases so that consumers can discover and download them.

## Decision

We will add a `publish_private_packages` configuration field to the `[git]` section of `.cursus/config.toml` that explicitly opts private packages into the non-registry portions of the publish workflow.

### Configuration

The new field is a list of package names under the existing `[git]` section:

```toml
[git]
enabled = true
publish_private_packages = ["my-action", "my-other-action"]
```

When omitted, the field defaults to an empty list, preserving the current behavior where all private packages are silently skipped.

### Publish behavior for listed packages

During `cursus publish`, after the existing private-package check from [ADR-007](007-honor-private-packages-during-publish.md), packages that are both private and listed in `publish_private_packages` will receive:

1. **Git tag creation** -- the same tag that would be created for a registry-published package (per [ADR-015](015-ci-managed-release-workflow.md), tags are created during `publish` when `[git].enabled = true`).
2. **GitHub Release creation** -- when `[github].enabled = true`, a GitHub Release is created with changelog-derived release notes and any configured artifacts, following the same flow as [ADR-005](005-github-releases.md).

These packages will **not** have any registry publish command invoked. The registry publish step is skipped entirely, not attempted and caught. From the registry's perspective, the package does not exist in the publish workflow.

### Interaction with `--package` flag

When a user explicitly names a private package with `--package`:

- If the package is listed in `publish_private_packages`, it receives tags and GitHub Releases as described above.
- If the package is **not** listed in `publish_private_packages`, it is silently skipped per [ADR-007](007-honor-private-packages-during-publish.md). This preserves the existing behavior where the upstream manifest remains the source of truth for truly unpublishable packages.

### Interaction with `cursus ci`

No changes are needed to `ci`'s state detection logic. Private packages that have a `CHANGELOG.md` (from `prepare`) already participate in tag-missing detection via [ADR-031](031-changelog-guard-for-unprepared-packages.md). When a listed private package lacks a tag for its current version, `ci` dispatches to `publish`, and `publish` now creates the tag. This is the natural resolution: `ci` detects the work, `publish` performs it.

Private packages not in the list continue to be silently skipped by `publish`. The `ci` subcommand's behavior for these packages is unchanged -- it will continue to dispatch to `publish` when their tags are missing, and `publish` will continue to silently skip them. This is consistent with how `ci` handles other non-deployable state transitions.

### Publish summary output

Listed private packages will appear in the publish summary with output indicating the non-registry actions taken:

```text
Tagged my-action@1.2.0
Created GitHub Release for my-action@1.2.0
  Attached: my-action-linux-x86_64
```

This follows the existing summary format from [ADR-005](005-github-releases.md) but omits the "Published ... to ..." line since no registry publish occurred.

### Dry-run behavior

Under `--dry-run`, listed private packages will be reported in the dry-run output showing what tags and GitHub Releases would be created, consistent with [ADR-008](008-dry-run-local-only-guarantee.md). No tags are created, no API calls are made.

### Scope

This decision affects only `cursus publish`. The `ci`, `prepare`, `change`, and `verify` subcommands are unaffected. Private packages continue to participate in version bumps, changeset recording, and changelog generation regardless of whether they appear in `publish_private_packages`.

## Consequences

### Positive

- GitHub Actions and other git-distributed packages can use the full automated `ci` workflow without manual tag and release creation, closing the gap left by [ADR-007](007-honor-private-packages-during-publish.md).
- Listed private packages now receive git tags during `publish`, allowing `ci`'s tag-missing detection to naturally settle after a successful publish run.
- The existing private-package behavior is entirely preserved when `publish_private_packages` is not configured. No breaking changes for current users.
- The configuration is explicit and intentional -- users must name each private package they want to publish via tags/releases, preventing accidental inclusion.

### Negative

- Introduces a Cursus-specific configuration field that partially overlaps with upstream manifest markers. [ADR-007](007-honor-private-packages-during-publish.md) deliberately avoided Cursus-specific configuration for publishability, preferring the upstream manifest as the single source of truth. This ADR creates an exception: the upstream manifest says "do not publish" and Cursus configuration says "but do create tags and releases." The overlap is justified because upstream manifests have no concept of "publish to GitHub Releases but not to a registry."
- Users must keep `publish_private_packages` in sync with their private packages. If a package is removed from the workspace but remains in the list, Cursus will need to handle the mismatch gracefully (the package simply will not be found during enumeration, so the entry becomes inert).
- The `[git]` section gains a field that arguably relates more to the publish workflow than to git configuration. However, the field controls tag creation, which is a git operation, and the `[git]` section already owns tag-related configuration (`tag_format`, `tag`).

### Neutral

- The `build_command` and `[github.artifacts]` configuration from [ADR-005](005-github-releases.md) applies to listed private packages in the same way as registry-published packages. The build command runs once per publish invocation, and artifacts are attached to all GitHub Releases in that run. No per-package artifact distinction is introduced by this ADR.
- Packages listed in `publish_private_packages` that are not actually marked as private by their upstream manifest are ignored by this feature -- they follow the normal registry publish path. The list only affects packages that would otherwise be silently skipped.

## Alternatives Considered

### Automatic opt-in for all private packages

Rather than requiring an explicit list, Cursus could automatically create tags and GitHub Releases for all private packages when `[git].enabled = true` and `[github].enabled = true`. This was rejected because it violates the principle of least surprise established by [ADR-007](007-honor-private-packages-during-publish.md). Many monorepos have private packages (root workspace packages, internal tooling) that are genuinely internal and should produce no release artifacts. Automatic opt-in would create unexpected tags and GitHub Releases for these packages.

### A boolean flag per package manager section

A field like `[npm].publish_github_releases = true` or `[cargo].publish_github_releases = true` that applies to all private packages under that package manager. This was rejected because it lacks per-package granularity. A monorepo may have some private packages that need releases (a GitHub Action) and others that do not (an internal build tool). The per-package list provides the necessary control.

### A separate `[publish]` configuration section

A new top-level section like `[publish].private_packages = ["my-action"]` rather than placing the field under `[git]`. This was rejected because the primary action being enabled is git tag creation, which is squarely within the `[git]` section's domain. GitHub Release creation is a secondary consequence that follows from having a tag. Adding a new top-level section for a single field would be over-engineered.

### Glob patterns instead of explicit package names

Supporting glob patterns (e.g., `publish_private_packages = ["*-action"]`) to match private packages. This was rejected because the number of private packages needing releases in a typical workspace is small and explicit naming is clearer and less error-prone. Globs risk accidentally matching packages the user did not intend to publish.
