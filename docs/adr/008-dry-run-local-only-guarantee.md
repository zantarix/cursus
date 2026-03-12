# ADR-008: Dry-Run Must Be Strictly Local-Only

## Status

Accepted

## Context

Chronicle provides a `--dry-run` flag across multiple commands (`release`, `publish`, and future commands) that allows users to preview what an operation would do without performing it. Several existing ADRs describe dry-run behavior for individual commands:

- **[ADR-003](003-release-command.md)** (`release --dry-run`): Prints the release summary and changelog entries without writing any changes to disk.
- **[ADR-004](004-publish-command.md)** (`publish --dry-run`): Passes the `--dry-run` flag through to the underlying package manager (`cargo publish --dry-run`, `npm publish --dry-run`).
- **[ADR-006](006-git-lifecycle-hooks.md)** (`release --dry-run` with git hooks): Skips both filesystem modifications and all git operations; only prints what would have happened.

There is currently no overarching policy that defines what `--dry-run` guarantees across all commands. Each ADR defines dry-run behavior locally, and the approaches differ: [ADR-003](003-release-command.md) and [ADR-006](006-git-lifecycle-hooks.md) implement dry-run by skipping operations entirely, while [ADR-004](004-publish-command.md) delegates to external tools. This inconsistency creates a gap.

The delegation approach in [ADR-004](004-publish-command.md) is problematic. While `cargo publish --dry-run` and `npm publish --dry-run` are documented as local-only operations today, Chronicle is trusting third-party tools to uphold a safety guarantee that users attribute to Chronicle's own `--dry-run` flag. If a package manager's `--dry-run` implementation were to change, or if a future package manager adapter's dry-run mode performed network operations (e.g., validating credentials against a remote registry, uploading metadata for validation), Chronicle would silently violate the user's expectation that `--dry-run` is safe and non-destructive.

Users have a strong mental model for `--dry-run`: it means "show me what would happen, but do not do anything." This expectation extends beyond "do not write to disk" -- it includes "do not push to remotes," "do not publish to registries," "do not create GitHub Releases," and "do not make any network requests that have side effects." A `--dry-run` flag that might contact external services undermines user trust in the tool.

This is particularly important in CI environments where `--dry-run` is used for validation steps. A dry-run that accidentally publishes a package or pushes a tag could cause irreversible damage. The blast radius of getting this wrong is high: published packages cannot be unpublished from most registries, and pushed tags or GitHub Releases create visible artifacts that must be manually cleaned up.

## Decision

We will enforce a project-wide invariant: when `--dry-run` is active, Chronicle will never perform any remote or externally-visible operation. This applies to all current and future commands.

### Definition of "remote operation"

A remote operation is any action that communicates with an external service or modifies state outside the local filesystem and process. This includes but is not limited to:

- **Git push**: Pushing commits or tags to a remote repository
- **Registry publish**: Uploading packages to crates.io, npm, or any other registry
- **GitHub API calls**: Creating releases, tags, or any other GitHub resource
- **Network validation**: Contacting a remote service to validate credentials, check version availability, or perform any server-side operation
- **Webhook triggers**: Initiating any callback or notification to an external system

### Implementation rule

When `--dry-run` is active, Chronicle will skip any operation that would contact a remote service and instead print a description of what would have happened. Chronicle will not delegate dry-run behavior to external tools if those tools might perform network operations.

Concretely, this changes the approach described in [ADR-004](004-publish-command.md) for `publish --dry-run`:

- **Before ([ADR-004](004-publish-command.md))**: Chronicle passes `--dry-run` through to the package manager (e.g., `cargo publish --dry-run`), trusting the external tool to handle it safely.
- **After (this ADR)**: Chronicle skips the publish invocation entirely and prints a summary of what would have been published. No subprocess is spawned for publish operations during dry-run.

This means `publish --dry-run` loses the local validation that `cargo publish --dry-run` provides (such as checking that the package builds and the manifest is valid). This is an acceptable trade-off: local validation can be performed separately with `cargo build` or `cargo package`, and the safety guarantee of `--dry-run` is more valuable than the convenience of bundled validation.

### Scope

This invariant applies to:

- `chronicle release --dry-run`: No filesystem writes, no git operations, no remote operations (already compliant via [ADR-003](003-release-command.md) and [ADR-006](006-git-lifecycle-hooks.md))
- `chronicle publish --dry-run`: No registry uploads, no GitHub Release creation, no subprocess invocations that contact remotes
- Any future command that accepts `--dry-run`: Must follow this invariant

### Dry-run output contract

When `--dry-run` is active, Chronicle will print a human-readable summary of all operations that would have been performed, clearly prefixed to indicate they are hypothetical. The exact format is command-specific, but all dry-run output must make it unambiguous that no real action was taken.

## Consequences

### Positive

- Users can trust that `--dry-run` is completely safe in all contexts: local development, CI pipelines, shared environments, and production release workflows.
- Eliminates an entire class of potential incidents where a dry-run accidentally publishes a package or pushes a tag due to a third-party tool's behavior change.
- Creates a clear, enforceable invariant that is easy to reason about when implementing new commands or adapters: if dry-run is set, do not call anything remote.
- Simplifies testing of dry-run paths: no need to mock external services or set up credentials for dry-run integration tests.

### Negative

- `publish --dry-run` no longer performs local validation that `cargo publish --dry-run` would provide (e.g., verifying the package builds, checking manifest completeness). Users who want this validation must run it separately.
- Future package manager adapters cannot leverage their tool's native `--dry-run` mode, even if that mode is genuinely local-only. The policy is conservative by design.
- The dry-run output for `publish` becomes less detailed since Chronicle is not invoking the underlying tool. It can only report what it would invoke, not what the tool would have reported.

### Neutral

- This ADR does not change the behavior of `release --dry-run` or the git hooks dry-run path, as they already comply with this invariant.
- An errata note will be added to [ADR-004](004-publish-command.md) to document that `publish --dry-run` no longer delegates to the underlying package manager, referencing this ADR.
- Local filesystem reads (e.g., reading manifest files to determine what would be published) are still permitted during dry-run. Only writes and remote operations are prohibited.

## Alternatives Considered

### Continue delegating to external tools' dry-run modes

Maintain the [ADR-004](004-publish-command.md) approach of passing `--dry-run` through to `cargo publish --dry-run` and `npm publish --dry-run`. This was rejected because it makes Chronicle's safety guarantee dependent on third-party behavior. While these tools' dry-run modes are currently local-only, Chronicle cannot enforce that contract, and a change in behavior would silently violate user expectations. The risk is disproportionate to the benefit.

### Allow network operations that are read-only

Permit dry-run to make read-only network requests (e.g., checking if a version already exists on a registry) while prohibiting write operations. This was rejected because the distinction between read-only and write network operations is difficult to verify for external tools, and even read-only requests can leak information (e.g., revealing that a release is being prepared) or trigger rate limits. A blanket prohibition is simpler to enforce and reason about.

### Make the behavior configurable

Add a flag like `--dry-run=local` vs `--dry-run=validate` to let users choose between a strict local-only mode and a mode that delegates to external tools for validation. This was rejected because it adds complexity to the CLI surface, splits the mental model of what `--dry-run` means, and creates a risk that the less-safe mode becomes the default in practice. A single, strict `--dry-run` is easier to document, teach, and trust.

## Errata

**2026-03-09**: [ADR-015](015-ci-managed-release-workflow.md) adds git tag creation and pushing to the `chronicle publish` workflow when `[git].enabled = true`. The Scope section of this ADR lists `chronicle publish --dry-run` as skipping "No registry uploads, no GitHub Release creation, no subprocess invocations that contact remotes." Tag creation and tag pushing must now also be skipped during dry-run. The invariant itself ("no remote operations during dry-run") is unchanged; the set of operations it covers has expanded. See [ADR-015](015-ci-managed-release-workflow.md).

**2026-03-09**: [ADR-016](016-rename-release-to-prepare.md) renames the `chronicle release` subcommand to `chronicle prepare`. References to `chronicle release` in this ADR now refer to `chronicle prepare`. The behavior and dry-run guarantees are unchanged. See [ADR-016](016-rename-release-to-prepare.md) for details.
