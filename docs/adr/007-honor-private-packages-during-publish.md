# ADR-007: Honor Private Package Markers During Publish

## Status
Proposed

## Context

Chronicle's `chronicle publish` command (ADR-004) publishes all configured packages to their respective registries. It iterates over every project enumerated by the enabled package manager adapters and invokes the registry-specific publish command for each one.

Some packages are not intended for registry publication. A prominent example is GitHub Actions: they are developed as standard npm packages (TypeScript/JavaScript with `package.json`, versioned with semver), but they are consumed directly from git repositories via tags and GitHub Releases (`uses: owner/repo@v1.2.3`), not from the npm registry. Publishing such a package to npm would be incorrect.

Both npm and Cargo have established conventions for marking packages as unpublishable:

- **npm**: The `"private": true` field in `package.json` prevents `npm publish` from executing. npm itself refuses to publish private packages, treating them as not intended for public consumption.
- **Cargo**: The `publish = false` field in `[package]` in `Cargo.toml` prevents `cargo publish` from uploading the crate. Cargo also supports `publish = ["registry-name"]` to restrict publishing to specific registries.

Currently, Chronicle does not read these fields. When `chronicle publish` runs, it attempts to publish every enumerated package. For a private npm package like a GitHub Action, this means Chronicle would invoke `npm publish`, which would then fail because npm itself enforces the `private` field. The publish fails with an error rather than being silently skipped, which is noisy and misleading in CI pipelines.

The desired workflow for a GitHub Action repository is:

1. `chronicle release` -- bumps version in `package.json`, generates changelog, deletes changesets (works today)
2. Git operations -- commit, tag, push (manual or via ADR-006)
3. GitHub Release creation -- creates release with changelog (ADR-005)
4. `chronicle publish` -- runs without error because the private package is silently excluded

In a mixed monorepo (e.g., a publishable npm library alongside a GitHub Action), the publish step should publish the library to npm and silently ignore the GitHub Action.

## Decision

We will read upstream package manager privacy/publish markers and silently exclude private packages from `chronicle publish`.

### npm: Honor `"private": true`

When the npm adapter's `publish()` method is called for a project, it will first read the `package.json` and check the `"private"` field. If `"private": true` is set, the adapter will return `Ok(PublishOutcome::Skipped)` without invoking `npm publish`.

This reuses the existing npm convention. Developers already understand that `"private": true` means "do not publish to npm." No Chronicle-specific configuration is needed.

### Cargo: Honor `publish = false`

When the Cargo adapter's `publish()` method is called for a project, it will first read the `Cargo.toml` and check the `[package].publish` field. If `publish = false` is set, the adapter will return `Ok(PublishOutcome::Skipped)` without invoking `cargo publish`.

The `publish` field in Cargo can also be a list of allowed registries (e.g., `publish = ["my-registry"]`). Chronicle will not interpret registry lists in this initial implementation; only the boolean `false` value triggers the skip. An empty list (`publish = []`) is equivalent to `publish = false` in Cargo's semantics and will also be treated as a skip.

### New `PublishOutcome` variant

A new `Skipped` variant will be added to the `PublishOutcome` enum:

```rust
pub enum PublishOutcome {
    Published,
    AlreadyPublished,
    Skipped,
}
```

This distinguishes "private package, intentionally not published" from "already published" and "successfully published." The distinction matters for the summary output and for potential future machine-readable output.

### Behavior in `chronicle publish`

Private packages will be silently excluded from the publish summary. They will produce no output lines -- no "Published," no "Skipped," no "Failed." From the user's perspective, private packages do not exist in the publish workflow.

The summary counts at the end will also exclude private packages. If all packages in a repository are private, the output will show zero published and zero skipped, with a successful exit code.

### Interaction with `--package` flag

If a user explicitly names a private package with `--package`, the package will still be silently skipped. This is consistent with the principle that the upstream manifest is the source of truth for publishability. Explicitly requesting a private package is not an error because:

- The user may be running a script that passes all package names without filtering.
- The `private` field is the package's own declaration that it should not be published, regardless of who requests it.
- Erroring would make CI scripts more fragile, requiring them to maintain a separate list of publishable packages.

### Interaction with `--dry-run`

Dry-run mode will also silently exclude private packages. Since the purpose of `--dry-run` is to preview what would happen during a real publish, and a real publish silently skips private packages, the dry-run should mirror that behavior.

### Scope: Publish only

The private/publish markers affect only `chronicle publish`. They have no effect on `chronicle release` or `chronicle change`. Private packages are still:

- Enumerated as projects (they appear in `chronicle init` and `chronicle change`)
- Eligible for version bumps via `chronicle release`
- Given changelog entries

This is intentional. A GitHub Action still needs version bumps and changelogs. The only thing it does not need is registry publication.

### No new configuration

This decision introduces no new fields in `.chronicle/config.toml`. The behavior is derived entirely from the upstream manifest files (`package.json` and `Cargo.toml`). This follows the project's principle of reusing existing ecosystem conventions rather than inventing Chronicle-specific ones.

## Consequences

### Positive
- GitHub Actions and other git-distributed packages work naturally with Chronicle's full workflow without requiring workarounds or publish errors.
- Reuses existing npm and Cargo conventions that developers already know. No new Chronicle-specific configuration to learn.
- Per-package granularity allows mixed repositories where some packages publish to registries and others do not.
- Silent exclusion keeps CI output clean. Private packages do not clutter the publish summary with skip messages that would appear on every run.
- The `PublishOutcome::Skipped` variant provides a clean internal distinction for future use (e.g., machine-readable output, metrics).

### Negative
- Chronicle now reads and interprets an additional field from each manifest file during publish, adding a small amount of coupling to the manifest schema. Both fields (`private` in npm, `publish` in Cargo) are stable and well-established.
- Silent skipping means that if a user accidentally sets `"private": true`, they will get no indication from Chronicle that the package was excluded. However, this matches the behavior developers expect from these fields, and `npm publish` itself would also refuse to publish.
- The Cargo `publish` field supports registry lists (`publish = ["my-registry"]`), which this implementation does not interpret. A crate restricted to a specific non-default registry will still be published to whatever registry `cargo publish` targets by default. This can be addressed in a future enhancement if needed.

### Neutral
- This decision is complementary to ADR-005 (GitHub Releases). A GitHub Action repository would typically enable `[github]` for releases and set `"private": true` in `package.json` to skip npm publishing. The two features compose naturally.
- Future package manager adapters should follow the same pattern: check for the ecosystem's native "do not publish" marker before attempting to publish.

## Alternatives Considered

### Chronicle-specific configuration in `.chronicle/config.toml`
A field like `[npm] skip_publish = true` or a per-package `[[packages]]` section with publish control. This was rejected because it duplicates information that already exists in the upstream manifest, requires users to learn a Chronicle-specific mechanism, and creates a risk of the two configurations diverging (e.g., `package.json` says private but Chronicle config says publish). The upstream manifest should be the single source of truth for publishability.

### Auto-detection based on GitHub Action metadata
Chronicle could detect the presence of `action.yml` or `action.yaml` in a package directory and infer that the package should not be published to npm. This was rejected because it couples Chronicle to GitHub Actions specifically rather than honoring a general-purpose convention. The `"private": true` field covers GitHub Actions and any other non-publishable npm package (internal tools, monorepo root packages, etc.) without special-casing.

### Printing a "Skipped (private)" message during publish
Rather than silently excluding private packages, Chronicle could print a line like `Skipped my-action@1.2.0 (private)` in the publish output. This was rejected because it adds noise to every publish run for packages that are never intended to be published. The information is not actionable. Users who want to verify which packages are private can inspect their manifest files directly.

### Erroring when `--package` explicitly names a private package
Chronicle could treat an explicit `--package my-private-pkg` as an error, on the theory that the user made a mistake. This was rejected because it makes CI scripts fragile. A common pattern is to pass all package names to `--package` without filtering, and erroring on private packages would force scripts to maintain a separate exclusion list that duplicates information already in the manifest.
