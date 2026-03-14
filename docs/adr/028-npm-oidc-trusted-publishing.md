# ADR-028: Support npm OIDC Trusted Publishing

## Status

Accepted

## Context

npm introduced OIDC-based trusted publishing (GA as of July 2025), requiring npm CLI 11.5.1 or later. The mechanism mirrors PyPI's trusted publishers: a CI environment's OIDC identity token is exchanged for a short-lived npm publish credential, eliminating the need for long-lived `NPM_TOKEN` secrets. The trusted publisher relationship must be pre-configured per-package on npmjs.com.

npm's OIDC support is currently limited to GitHub Actions and GitLab CI. Self-hosted runners and other CI providers are not supported by npm, so detection is intentionally scoped to these two environments.

Cursus's authentication philosophy (documented in [ADR-004](004-publish-command.md) and [ADR-005](005-github-releases.md)) is to delegate credential management entirely to the user and environment. Cursus never stores, generates, or rotates tokens. This ADR extends that philosophy to OIDC: cursus detects the OIDC-capable environment and surfaces potential misconfiguration as warnings, but never modifies the authentication environment.

Per [ADR-009](009-javascript-package-manager-strategy.md), cursus always uses `npm publish` for registry publishing regardless of which JavaScript package manager (npm, yarn, pnpm) is configured. OIDC trusted publishing therefore applies uniformly to all JS package manager configurations.

## Decision

### 1. OIDC environment detection in `Env`

We will detect OIDC capability early, during `Env` construction, and store it as a boolean field on the `Env` struct. Detection checks the following environment variables:

- **GitHub Actions**: `ACTIONS_ID_TOKEN_REQUEST_URL` is set (indicates the workflow has `id-token: write` permission).
- **GitLab CI**: `CI_JOB_JWT_V2` is set (the actual OIDC token variable, present only when OIDC is available for the job).

In both cases, detection checks for the variable that carries or gates the OIDC credential itself, rather than a general CI indicator like `GITLAB_CI` or `GITHUB_ACTIONS`. This makes the detection resilient to future provider changes: if a provider restructures how OIDC tokens are surfaced, the flag will correctly stop reporting OIDC capability rather than producing false positives.

The flag is populated once and made available to any downstream component via an accessor method, avoiding repeated environment lookups.

### 2. Pre-publish warning for `NODE_AUTH_TOKEN` interference

The presence of `NODE_AUTH_TOKEN` will be captured as a boolean flag on `Env` at construction time, alongside the OIDC flag. If `Env` reports an OIDC environment AND `NODE_AUTH_TOKEN` was detected, the npm adapter will emit a `warn!()` before invoking `npm publish`. The warning will explain that `NODE_AUTH_TOKEN` takes precedence over OIDC token exchange and the publish may not use trusted publishing. No remediation is performed -- the user may intentionally be using a classic token in an OIDC-capable environment.

### 3. Pre-publish warning for missing authentication

If `Env` does not report an OIDC environment AND `Env` does not report `NODE_AUTH_TOKEN` presence, the npm adapter will emit a `warn!()` before invoking `npm publish`. The warning will note that no recognised authentication mechanism is configured and the publish is likely to fail. This catches the common case of running `cursus publish` locally or in an unsupported CI environment without credentials. As with all warnings, no remediation is performed -- the publish proceeds and npm's own error will confirm the failure.

### 4. No `provenance` config flag

We will not add a cursus configuration option for provenance. Users control provenance via `publishConfig.provenance` in their `package.json`, following the upstream convention reuse principle. When using npm trusted publishing, npm automatically attaches provenance attestations. However, provenance is unavailable for packages published from private source repositories, so cursus must not assume or require it.

### 5. Pre-publish warning for missing `publishConfig.provenance`

If in an OIDC environment AND `npm.access` is configured as `"public"` AND the package's `ProjectInfo` does not report `publishconfig_provenance` as `true`, the npm adapter will emit a `warn!()`. The warning will note that npm attaches provenance automatically via trusted publishing, but that setting `publishConfig.provenance = true` makes the intent explicit and ensures provenance is attached even in non-OIDC publish scenarios.

`ProjectInfo` will gain an optional boolean field (e.g. `publishconfig_provenance: Option<bool>`) populated by the npm adapter's `enumerate_projects` during its existing `package.json` parse. The overhead of carrying this field on all `ProjectInfo` instances is negligible, and it avoids re-reading `package.json` at publish time. Non-npm adapters leave this field as `None`.

### 6. Initial release limitation acknowledgement

npm has no "pending publisher" concept. A trusted publisher can only be configured for a package that already exists on the registry. The first publish of a new package must use a classic npm token. This is a limitation of npm's implementation and is outside cursus's control. Cursus does not attempt to detect or work around this -- the resulting npm error will surface naturally.

## Consequences

### Positive

- Users in CI environments benefit from OIDC without any cursus configuration changes -- npm handles the token exchange transparently.
- The `NODE_AUTH_TOKEN` interference warning prevents a common misconfiguration where a stale token silently overrides OIDC.
- The missing-authentication warning catches the common mistake of running `cursus publish` without any npm credentials, providing early guidance before npm's own error.
- The provenance warning nudges users toward explicit provenance declarations without mandating them.
- No new config fields means no migration burden and no config surface area to maintain.

### Negative

- Detection is limited to GitHub Actions and GitLab CI. Users on unsupported CI providers receive no OIDC-related warnings or guidance.
- The `NODE_AUTH_TOKEN` warning may be noisy for users who intentionally use classic tokens in OIDC environments. Since this is a `warn!()`, it can be suppressed with `-s` (silent mode) if needed.
- `ProjectInfo` gains a field that is only meaningful for npm packages; non-npm adapters leave it as `None`.

### Neutral

- `Env` gains boolean flags for OIDC environment and `NODE_AUTH_TOKEN` presence, both populated at construction time.
- `ProjectInfo` gains an optional `publishconfig_provenance` field populated during npm project enumeration.
- The npm adapter's `publish` method gains three conditional `warn!()` calls before the existing `npm publish` invocation (token interference, missing authentication, missing provenance).
- The first publish of any new npm package continues to require a manual step outside cursus, unchanged from today.
- This decision applies to all JS package manager variants (npm, yarn, pnpm) since publishing always goes through `npm publish` per [ADR-009](009-javascript-package-manager-strategy.md).

## Alternatives Considered

### Add a `provenance` config field to `NpmConfig`

A dedicated `[npm].provenance` boolean in cursus config would let users control provenance without editing `package.json`. This was rejected because it duplicates npm's own `publishConfig.provenance` field, violating the upstream convention reuse principle. It also creates a divergence risk: the `package.json` and cursus config could disagree, requiring reconciliation logic.

### Inject `--provenance` flag into `npm publish`

Cursus could append `--provenance` to the `npm publish` command when OIDC is detected. This was rejected because it mangles the environment -- cursus's philosophy is to surface issues as warnings but never alter the publish command based on detected conditions. Users who want provenance should declare it in `package.json` where it is visible to all tooling, not just cursus.

### Detect OIDC lazily at publish time instead of in `Env`

OIDC detection could happen inside `NpmAdapter::publish` rather than during `Env` construction. This was rejected because the OIDC flag is an environment-level concern, not a package-manager-specific one. Storing it on `Env` makes it available to future adapters or features that may also need to reason about OIDC (e.g., future PyPI trusted publishing support for a hypothetical Python adapter).

### Read `publishConfig.provenance` at publish time instead of during enumeration

The provenance field could be read from `package.json` as a targeted read inside `NpmAdapter::publish` rather than extending `ProjectInfo`. This was rejected because `enumerate_projects` already parses `package.json` for every npm package, so extracting one additional field there is virtually free. Re-reading the file at publish time would add redundant I/O and split `package.json` parsing across two call sites.

### Block publish when `NODE_AUTH_TOKEN` conflicts with OIDC

Instead of warning, cursus could refuse to publish when both OIDC and `NODE_AUTH_TOKEN` are present. This was rejected because the user may have a valid reason for the configuration (e.g., publishing to a private registry that does not support OIDC while running in a GitHub Actions workflow). Cursus warns but never blocks legitimate workflows.
