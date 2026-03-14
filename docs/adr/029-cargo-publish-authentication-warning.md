# ADR-029: Warn on Missing Cargo Registry Token Before Publish

## Status

Accepted

## Context

Cargo publishing to crates.io requires authentication via `CARGO_REGISTRY_TOKEN` (or an equivalent token configured in `~/.cargo/credentials.toml`). Unlike npm, which now supports OIDC trusted publishing ([ADR-028](028-npm-oidc-trusted-publishing.md)), Cargo has no OIDC mechanism -- token-based authentication is the only supported method for CI publishing. If the token is absent, `cargo publish` will fail with an authentication error.

Cursus's authentication philosophy (documented in [ADR-004](004-publish-command.md) and [ADR-005](005-github-releases.md)) is to delegate credential management entirely to the user and environment. Cursus never stores, generates, or rotates tokens. The goal here is purely to surface a pre-publish warning when the token is undetectable, giving the user early context before the inevitable `cargo publish` failure.

## Decision

The presence of `CARGO_REGISTRY_TOKEN` will be captured as a boolean flag on `Env` at construction time, following the same pattern as the OIDC detection flag from [ADR-028](028-npm-oidc-trusted-publishing.md). The Cargo adapter's `publish` method will check this flag and emit a `warn!()` if the token was not detected. The warning will note that no registry token is configured and the publish is likely to fail.

The check is limited to the `CARGO_REGISTRY_TOKEN` environment variable. `~/.cargo/credentials.toml` is not inspected -- reading user home directory files is outside cursus's scope, would be fragile across environments (containers, CI runners with non-standard home directories), and the environment variable is the standard CI mechanism. Users who authenticate via `credentials.toml` will see the warning but publishing will succeed regardless.

No remediation is performed. The publish proceeds unconditionally and `cargo publish` produces its own error on authentication failure.

## Consequences

### Positive

- Users who forget to configure `CARGO_REGISTRY_TOKEN` in their CI pipeline receive an early, actionable warning before the `cargo publish` error.
- Consistent with the npm adapter's missing-authentication warning from [ADR-028](028-npm-oidc-trusted-publishing.md), establishing a pattern of pre-publish credential checks across all adapters.

### Negative

- Users who authenticate via `~/.cargo/credentials.toml` (common in local development) will see a spurious warning. Since this is a `warn!()`, it can be suppressed with `-s` (silent mode) if needed.

### Neutral

- The Cargo adapter's `publish` method gains one conditional `warn!()` call before the existing `cargo publish` invocation.
- `Env` gains a boolean flag for `CARGO_REGISTRY_TOKEN` presence, populated at construction time.
- No new fields on `CargoConfig` or `ProjectInfo`.

## Alternatives Considered

### Inspect `~/.cargo/credentials.toml`

Cursus could read the Cargo credentials file to avoid false-positive warnings for users who authenticate that way. This was rejected because it requires resolving the user's home directory (which varies across environments), parsing a TOML file that cursus does not own, and handling platform-specific path conventions. The complexity is disproportionate to the benefit of suppressing a non-blocking warning.

### Block publish when token is missing

Instead of warning, cursus could refuse to publish when no token is detected. This was rejected for the same reason as in [ADR-028](028-npm-oidc-trusted-publishing.md): the user may have a valid authentication mechanism that cursus cannot detect (e.g., `credentials.toml`, a credential helper, or a custom registry configuration). Cursus warns but never blocks legitimate workflows.
