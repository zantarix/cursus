# ADR-045: Support crates.io OIDC Trusted Publishing

## Status

Accepted (2026-04-19)

## Context

crates.io reached general availability on OIDC-based trusted publishing in mid-2025. As with npm ([ADR-028](028-npm-oidc-trusted-publishing.md)) and PyPI before it, trusted publishing allows CI pipelines to publish crates without long-lived `CARGO_REGISTRY_TOKEN` secrets. A short-lived token is obtained by exchanging a CI-issued OIDC id-token for a registry-scoped credential. GitHub Actions has been supported since GA; GitLab CI support was added in January 2026 (currently public beta, GitLab.com only -- self-hosted GitLab is not yet supported).

There is a fundamental asymmetry between crates.io and npm that shapes this decision:

- **npm**: `npm publish` performs the OIDC token exchange itself when it detects an OIDC-capable environment and no overriding token. The exchange is transparent to cursus and to workflow authors.
- **crates.io**: `cargo publish` does **not** perform any OIDC exchange. Users must add a separate workflow step that exchanges the id-token for a `CARGO_REGISTRY_TOKEN` (the canonical action being `rust-lang/crates-io-auth-action`). That step injects the token into the environment before `cargo publish` runs.

From cursus's point of view, the trusted-publishing flow on crates.io therefore looks identical to the traditional token flow: `CARGO_REGISTRY_TOKEN` is present in the environment when `cargo publish` is invoked. The only difference is where the token came from (a long-lived secret vs. a just-issued short-lived exchange).

Cursus already has the environment-detection primitives needed to reason about this:

- `Env::oidc_environment()` reports whether the process is running in an OIDC-capable CI environment (GitHub Actions, GitLab CI), detected via `ACTIONS_ID_TOKEN_REQUEST_URL` / `CI_JOB_JWT_V2`. Introduced by [ADR-028](028-npm-oidc-trusted-publishing.md).
- `Env::cargo_registry_token_present()` reports whether `CARGO_REGISTRY_TOKEN` is set. Introduced by [ADR-029](029-cargo-publish-authentication-warning.md).

Per [ADR-030](030-bin-lib-crate-separation.md), all environment detection happens at the binary boundary; the library consumes these flags via `Env`. No new detection code is required to implement the decision below.

[ADR-029](029-cargo-publish-authentication-warning.md) currently asserts that *"Cargo has no OIDC mechanism -- token-based authentication is the only supported method for CI publishing."* That statement pre-dates crates.io's trusted-publishing GA and is now inaccurate. This ADR supersedes the warning behaviour described in [ADR-029](029-cargo-publish-authentication-warning.md) and an erratum on that ADR points forward to this one.

Like npm, crates.io requires a crate to already exist on the registry before a trusted publisher can be configured for it. The initial publish of a brand-new crate therefore still requires a classic `CARGO_REGISTRY_TOKEN`; trusted publishing only covers subsequent releases. This matches the npm constraint documented in [ADR-028](028-npm-oidc-trusted-publishing.md).

## Decision

### 1. Detect-and-warn only; no OIDC exchange inside cursus

Cursus will not perform the OIDC token exchange itself. Users remain responsible for invoking an external action (e.g. `rust-lang/crates-io-auth-action`) before `cursus publish`, which injects `CARGO_REGISTRY_TOKEN`. This preserves the credential-delegation principle from [ADR-004](004-publish-command.md): cursus detects the environment and surfaces potential misconfiguration, but never modifies the authentication environment and never talks to the registry's credential-exchange surface.

### 2. Warning set keyed on token presence, refined by OIDC presence

The warning logic in the Cargo adapter's `publish` path will be keyed on whether `CARGO_REGISTRY_TOKEN` is detected. OIDC-environment detection is used only to refine the advisory copy when no token is present, pointing the user at the correct remediation for their environment.

The full matrix is:

| `CARGO_REGISTRY_TOKEN` | OIDC env | Behaviour |
|------------------------|----------|-----------|
| absent | absent | Warn: no authentication detected; publish is likely to fail. Suggest running `cargo login` locally or setting `CARGO_REGISTRY_TOKEN` in CI. |
| absent | present | Warn: no authentication detected; publish is likely to fail. Note that an OIDC-capable CI environment was detected and that trusted publishing requires running an exchange action (e.g. `rust-lang/crates-io-auth-action`) before `cursus publish`. |
| present | absent | Silent. Traditional token flow. |
| present | present | Silent. Trusted-publishing happy path -- the exchange action has already run. |

### 3. Warning set is deliberately NOT symmetric with the npm warning set

On npm, the combination of OIDC-capable environment plus `NODE_AUTH_TOKEN` produces a warning because the token overrides npm's internal OIDC exchange -- the user's stated intent (OIDC) is being silently defeated by a stale secret.

On crates.io, the equivalent combination (OIDC env plus `CARGO_REGISTRY_TOKEN`) is the **normal, intended state** for trusted publishing: the exchange action's entire job is to produce that token. Warning here would fire on every successful trusted-publishing run and train users to ignore warnings.

This ADR therefore explicitly rejects npm symmetry. Future contributors should not "fix" the asymmetry without understanding this difference.

### 4. First-publish limitation matches npm

crates.io shares the same first-publish limitation as npm: the crate must already exist on the registry before a trusted publisher can be configured for it, so the initial publish of a new crate requires a `CARGO_REGISTRY_TOKEN`. Cursus adds no special behaviour for this case -- the existing token-absent warning already points users at the correct remediation (set `CARGO_REGISTRY_TOKEN` or run `cargo login`) -- but user-facing documentation must call out the limitation so users setting up a brand-new crate are not surprised when trusted publishing alone is insufficient for their first release.

### 5. User-facing documentation expectations

User-facing documentation (README, any workflow examples shipped with the project) will show a GitHub Actions snippet that runs the exchange action before `cursus publish`, and will note that `CARGO_REGISTRY_TOKEN` must be exported to the shell that runs `cursus publish`. Documentation will also call out the first-publish limitation so users setting up a brand-new crate understand that the initial release requires a classic `CARGO_REGISTRY_TOKEN`, and trusted publishing only applies to subsequent releases.

## Consequences

### Positive

- Users who configure crates.io trusted publishing get the full benefit (no long-lived `CARGO_REGISTRY_TOKEN` secret) with zero cursus-side configuration changes.
- The warning copy in the token-absent case now tells users exactly what to do based on their environment, instead of only pointing at `cargo login` / `CARGO_REGISTRY_TOKEN`.
- Cursus gains no new dependency on crates.io's trusted-publishing HTTP surface, so the decision is robust against upstream changes in the exchange protocol.
- The inaccuracy in [ADR-029](029-cargo-publish-authentication-warning.md) is corrected via an erratum and a superseding warning behaviour, keeping the historical record honest without rewriting accepted ADRs.

### Negative

- Users who set up trusted publishing incorrectly (e.g. forget to add the exchange action) still only get the generic "no authentication detected" warning; cursus cannot distinguish between "CI has no OIDC at all" and "CI has OIDC but the exchange step was skipped" beyond checking for an OIDC-capable environment.
- Two moving parts (the exchange action plus `cursus publish`) is more friction than the npm flow, but this friction lives upstream in cargo and is not something cursus can paper over without violating the credential-delegation principle.

### Neutral

- No new fields on `Env`, `CargoConfig`, or `ProjectInfo`. The decision is implemented entirely in terms of flags that already exist on `Env`.
- The Cargo adapter's `publish` method retains one warning call site; its message selection becomes conditional on the OIDC flag.
- Applies only to Cargo / crates.io; npm behaviour described in [ADR-028](028-npm-oidc-trusted-publishing.md) is unchanged.
- crates.io trusted publishing supports GitHub Actions (GA) and GitLab CI (public beta, GitLab.com only). The existing `Env::oidc_environment()` flag already detects both environments, so no cursus change is needed to support either provider; only the token-exchange mechanism differs (a GitHub Action on GHA vs. a script invoking the crates.io API on GitLab).

## Alternatives Considered

### Cursus performs the OIDC exchange itself

Cursus could detect the OIDC environment, call the crates.io token-exchange endpoint directly, and inject `CARGO_REGISTRY_TOKEN` into the subprocess environment before `cargo publish`. Rejected: this breaks the credential-delegation principle from [ADR-004](004-publish-command.md) and the environment-boundary principle from [ADR-030](030-bin-lib-crate-separation.md); it creates a hard dependency on the shape of crates.io's trusted-publishing HTTP surface; it significantly complicates testing (a whole new external surface to mock); and it duplicates functionality that already exists as a well-maintained GitHub Action.

### Hybrid: default detect-and-warn, optionally enable built-in exchange via config

A `[cargo].trusted_publishing_exchange` boolean could opt users into the built-in exchange path. Rejected: supporting two code paths for the same concern doubles the test and documentation surface for marginal benefit. The external-action path is strictly sufficient and works identically whether cursus is involved or not.

### Full npm symmetry: warn when both OIDC env and token are present

Treat the OIDC-plus-token combination the same way for Cargo as for npm. Rejected: this is the normal happy path for crates.io trusted publishing, not a misconfiguration. Warning here would fire on every successful trusted-publishing run and train users to ignore warnings -- the opposite of the intent behind the npm warning.

### No-op until cargo gains native OIDC support

Defer this decision until a future cargo release performs the OIDC exchange internally. Rejected: the refined warning is cheap, the documentation note about the first-publish limitation is immediately useful to users, and the inaccuracy in [ADR-029](029-cargo-publish-authentication-warning.md) needs to be corrected on the record regardless. Waiting for upstream provides no benefit to current users.
