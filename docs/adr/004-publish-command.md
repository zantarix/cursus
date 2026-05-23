# ADR-004: `cursus publish` Command

## Status

Accepted

## Context

After `cursus release` updates versions and changelogs ([ADR-003](003-release-command.md)), and the user commits and pushes those changes, the affected packages need to be published to their respective registries (crates.io, npm, etc.).

Cursus already knows which package managers are configured and which packages exist in the repository. It is the natural place to drive the publish step, rather than requiring users to manually invoke the correct publish command for each package manager and each affected package.

This is step 3 of the three-step release workflow described in [ADR-003](003-release-command.md).

## Decision

Implement a `cursus publish` subcommand that publishes all configured packages to their respective upstream registries.

### Scope

`cursus publish` publishes the current version of every package known to Cursus's configured package managers. It does not need to know which packages were recently bumped — it simply publishes what is in the manifest files. This makes the command stateless and simple: the user runs it after committing a release, and it publishes everything.

Specific packages can optionally be targeted: `cursus publish <pkg-name>` to publish a single package instead of all.

### Execution per package manager

**Cargo:**

- Run `cargo publish --manifest-path <path/to/Cargo.toml>` for each affected crate.
- In monorepos with inter-crate dependencies, packages must be published in dependency order. Cursus resolves this by reading workspace dependency graphs and publishing leaves first.
- After publishing a crate that other crates depend on, Cursus waits for registry index propagation before publishing dependents. This is handled with a retry-with-backoff strategy on the dependent publish.

**npm:**

- Run `npm publish` (or the equivalent for the detected package manager) from each package's directory.
- Scoped packages (`@scope/name`) require `--access public` on their first publish. Cursus passes this flag for scoped packages.
- Detection of which npm-compatible tool to use follows existing conventions: presence of `pnpm-lock.yaml` implies pnpm, `yarn.lock` implies yarn, otherwise npm.

### Authentication

Cursus does not manage registry credentials. It expects the environment to be pre-configured:

- **Cargo** — `cargo login` or `CARGO_REGISTRY_TOKEN` environment variable
- **npm** — `.npmrc` or `NPM_TOKEN` environment variable

If a publish fails due to authentication, Cursus reports the error clearly and exits with a non-zero status code.

### Idempotency

`cursus publish` must be idempotent — re-running it after a partial failure should be safe. However, most package managers are not themselves idempotent: attempting to publish a version that already exists is treated as an error by `cargo publish`, `npm publish`, etc.

Cursus provides the idempotency layer by detecting "version already exists" errors from the underlying package manager and treating them as success (the package is already published). This allows re-running the command to retry only the packages that genuinely failed, while skipping those that were already successfully published.

If all packages are either newly published or already published, the exit code is zero. If any package fails for a reason other than "already exists", the exit code is non-zero.

### Dry-run support

A `--dry-run` flag is supported. Cursus passes the dry-run flag through to the underlying package manager:

- `cargo publish --dry-run` — builds and validates the package without uploading
- `npm publish --dry-run` — shows what would be published without uploading

### Error handling

- If any package fails to publish (for reasons other than "version already exists"), Cursus reports the failure, continues attempting remaining packages, and exits with a non-zero status code.
- "Version already exists" errors are reported as skipped, not as failures.
- The summary output clearly indicates which packages were published, skipped, and failed.

### Interactive vs. non-interactive

Like `cursus release`, the publish command does not require a TUI. It is a batch operation.

### Summary output

After publishing, Cursus prints a summary:

```text
Published cursus-cli@0.2.0 to crates.io
Published @mscharley/cursus@0.2.0 to npm
```

Or on partial failure:

```text
Published cursus-cli@0.2.0 to crates.io
Failed to publish @mscharley/cursus@0.2.0 to npm: authentication required
```

## Consequences

- Cursus becomes responsible for invoking `cargo publish` and `npm publish`. This couples it to the CLI interfaces of these tools, which are stable and well-established.
- Authentication is entirely delegated to the environment. Cursus does not store, read, or manage tokens.
- The command is stateless — it does not track which packages were bumped by `cursus release`. It publishes whatever version is currently in the manifest. This simplifies the design but means running `cursus publish` without a preceding `cursus release` will attempt to publish the current versions, which will be detected as already published and skipped.
- Cursus must detect "version already exists" errors from each package manager's CLI output, which couples it to their error message formats. These messages are stable in practice but are not a formal API.
- Dependency-ordered publishing for Cargo workspaces adds complexity but is necessary for correctness. The npm ecosystem is more tolerant of publish ordering.
- Future package managers can be supported by implementing the publish logic on the `PackageManagerAdapter` trait.

## Errata

### 2026-02-21: `publish --dry-run` no longer delegates to the package manager

The "Dry-run support" section's description of `publish --dry-run` as delegating to the underlying package manager (e.g. `cargo publish --dry-run`) is incorrect. [ADR-008](008-dry-run-local-only-guarantee.md) establishes a project-wide invariant that `--dry-run` must never perform remote operations, and `cargo publish --dry-run` does perform a remote authentication round-trip; `publish --dry-run` therefore now skips the publish invocation entirely and prints a summary of what would have been published.

### 2026-03-09: Publish ordering extended with tag and release stages

The publish ordering described here ("publish to registries, then summary") is incomplete once git lifecycle hooks are enabled. [ADR-015](015-ci-managed-release-workflow.md) adds tag creation and pushing plus GitHub Release creation to the `cursus publish` workflow when `[git].enabled = true`, making the real ordering: publish to registries, create and push tags, create GitHub Releases, then summary. The same ADR also extends the `--no-git` flag (originally defined for `release` by [ADR-006](006-git-lifecycle-hooks.md)) to `publish`.

### 2026-03-09: `cursus release` renamed to `cursus prepare`

References to `cursus release` in this ADR are incorrect: [ADR-016](016-rename-release-to-prepare.md) renames the subcommand to `cursus prepare`. The behaviour is unchanged; only the user-facing name differs.

### 2026-05-02: Skipped packages now flow into tag and release stages

The "Idempotency" section implies that packages reported as `Skipped` by the registry are terminal for that invocation. This is no longer accurate: [ADR-055](055-end-to-end-idempotent-publish-recovery.md) extends the model so that registry-skipped packages still flow into the tag and GitHub Release stages, allowing re-running `cursus publish` after a partial failure to complete those downstream stages. The registry-side contract ("treat 'version already exists' as success") is itself unchanged.
