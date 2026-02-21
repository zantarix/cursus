# ADR-004: `chronicle publish` Command

## Status

Accepted

## Context

After `chronicle release` updates versions and changelogs (ADR-003), and the user commits and pushes those changes, the affected packages need to be published to their respective registries (crates.io, npm, etc.).

Chronicle already knows which package managers are configured and which packages exist in the repository. It is the natural place to drive the publish step, rather than requiring users to manually invoke the correct publish command for each package manager and each affected package.

This is step 3 of the three-step release workflow described in ADR-003.

## Decision

Implement a `chronicle publish` subcommand that publishes all configured packages to their respective upstream registries.

### Scope

`chronicle publish` publishes the current version of every package known to Chronicle's configured package managers. It does not need to know which packages were recently bumped — it simply publishes what is in the manifest files. This makes the command stateless and simple: the user runs it after committing a release, and it publishes everything.

Specific packages can optionally be targeted: `chronicle publish <pkg-name>` to publish a single package instead of all.

### Execution per package manager

**Cargo:**

- Run `cargo publish --manifest-path <path/to/Cargo.toml>` for each affected crate.
- In monorepos with inter-crate dependencies, packages must be published in dependency order. Chronicle resolves this by reading workspace dependency graphs and publishing leaves first.
- After publishing a crate that other crates depend on, Chronicle waits for registry index propagation before publishing dependents. This is handled with a retry-with-backoff strategy on the dependent publish.

**npm:**

- Run `npm publish` (or the equivalent for the detected package manager) from each package's directory.
- Scoped packages (`@scope/name`) require `--access public` on their first publish. Chronicle passes this flag for scoped packages.
- Detection of which npm-compatible tool to use follows existing conventions: presence of `pnpm-lock.yaml` implies pnpm, `yarn.lock` implies yarn, otherwise npm.

### Authentication

Chronicle does not manage registry credentials. It expects the environment to be pre-configured:

- **Cargo** — `cargo login` or `CARGO_REGISTRY_TOKEN` environment variable
- **npm** — `.npmrc` or `NPM_TOKEN` environment variable

If a publish fails due to authentication, Chronicle reports the error clearly and exits with a non-zero status code.

### Idempotency

`chronicle publish` must be idempotent — re-running it after a partial failure should be safe. However, most package managers are not themselves idempotent: attempting to publish a version that already exists is treated as an error by `cargo publish`, `npm publish`, etc.

Chronicle provides the idempotency layer by detecting "version already exists" errors from the underlying package manager and treating them as success (the package is already published). This allows re-running the command to retry only the packages that genuinely failed, while skipping those that were already successfully published.

If all packages are either newly published or already published, the exit code is zero. If any package fails for a reason other than "already exists", the exit code is non-zero.

### Dry-run support

A `--dry-run` flag is supported. Chronicle passes the dry-run flag through to the underlying package manager:

- `cargo publish --dry-run` — builds and validates the package without uploading
- `npm publish --dry-run` — shows what would be published without uploading

### Error handling

- If any package fails to publish (for reasons other than "version already exists"), Chronicle reports the failure, continues attempting remaining packages, and exits with a non-zero status code.
- "Version already exists" errors are reported as skipped, not as failures.
- The summary output clearly indicates which packages were published, skipped, and failed.

### Interactive vs. non-interactive

Like `chronicle release`, the publish command does not require a TUI. It is a batch operation.

### Summary output

After publishing, Chronicle prints a summary:

```
Published chronicle-cli@0.2.0 to crates.io
Published @mscharley/chronicle@0.2.0 to npm
```

Or on partial failure:

```
Published chronicle-cli@0.2.0 to crates.io
Failed to publish @mscharley/chronicle@0.2.0 to npm: authentication required
```

## Consequences

- Chronicle becomes responsible for invoking `cargo publish` and `npm publish`. This couples it to the CLI interfaces of these tools, which are stable and well-established.
- Authentication is entirely delegated to the environment. Chronicle does not store, read, or manage tokens.
- The command is stateless — it does not track which packages were bumped by `chronicle release`. It publishes whatever version is currently in the manifest. This simplifies the design but means running `chronicle publish` without a preceding `chronicle release` will attempt to publish the current versions, which will be detected as already published and skipped.
- Chronicle must detect "version already exists" errors from each package manager's CLI output, which couples it to their error message formats. These messages are stable in practice but are not a formal API.
- Dependency-ordered publishing for Cargo workspaces adds complexity but is necessary for correctness. The npm ecosystem is more tolerant of publish ordering.
- Future package managers can be supported by implementing the publish logic on the `PackageManagerAdapter` trait.

## Errata

**2026-02-21**: ADR-008 establishes a project-wide invariant that `--dry-run` must never perform remote operations. This supersedes the dry-run approach described in this ADR's "Dry-run support" section: `publish --dry-run` no longer delegates to the underlying package manager (e.g., `cargo publish --dry-run`). Instead, Chronicle skips the publish invocation entirely and prints a summary of what would have been published. See ADR-008 for full details.
