# ADR-041: Rename GitHubClient Trait to CodeForgeClient

## Status

Accepted

## Context

The `GitHubClient` trait ([ADR-038](038-octocrab-github-client.md)) abstracts code forge operations -- release creation, pull requests, and asset uploads. The trait name couples the abstraction to a single forge vendor, even though the trait's method signatures are not GitHub-specific and the project intends to support additional forges (GitLab, Gitea, etc.) in the future.

## Decision

We will rename the `GitHubClient` trait to `CodeForgeClient`. The production implementation, `OctocrabGitHubClient`, keeps its current name because it accurately describes both the underlying library (octocrab) and the target forge (GitHub).

All references to the trait throughout the codebase -- `Env` fields, function signatures, test helpers, documentation -- will be updated to use `CodeForgeClient`. The `github/` module directory retains its name for now since it still only contains GitHub-oriented code; a future ADR may reorganise the module structure when a second forge is added.

## Consequences

### Positive

- The trait name accurately reflects its role as a forge-agnostic abstraction
- Adding a GitLab or Gitea implementation will not require renaming the trait again

### Negative

- One-time churn across the codebase to update all trait references

### Neutral

- `OctocrabGitHubClient` remains unchanged; new forge implementations will follow the same `<Library><Forge>Client` naming pattern (e.g. `ReqwestGitLabClient`)
- The `Option<Arc<dyn CodeForgeClient>>` field on `Env` replaces `Option<Arc<dyn GitHubClient>>`

## Alternatives Considered

### Keep the GitHubClient name

The trait works today and renaming is pure cosmetics until a second forge is actually implemented. Rejected because the rename is cheap now and prevents a larger, more confusing rename later when a second implementation is in-flight.

### Use a more generic name like ForgeClient or RemoteForge

Shorter names were considered but `CodeForgeClient` is more descriptive and avoids ambiguity with unrelated uses of "forge" (e.g. Minecraft Forge, metal forging).

## Errata

- **2026-05-13**: The Decision's closing remark that "the `github/` module directory retains its name for now [...] a future ADR may reorganise the module structure when a second forge is added" is realised by [ADR-056](056-gitlab-support-client-config-and-ci.md). The `github/` module is relocated to `forge::github`, and the `CodeForgeClient` trait now lives at `forge::client::CodeForgeClient` (re-exported from `forge`). The rename and the parent-module reorganisation are independent decisions; this ADR's trait rename stands unchanged.
- **2026-05-13**: The trait's "single slot in `Env`" semantics are now formally enforced at configuration load time by [ADR-059](059-forge-selection-runtime-rules.md). The `CodeForgeClient` abstraction was always singular by design, but the new rule makes it a hard load-time invariant: at most one forge config section may have `enabled = true`, so `Env`'s `Result<Arc<dyn CodeForgeClient>, String>` slot is always populated by exactly zero or one concrete implementation. This does not change the trait or its consumers; it only closes out an ambiguity in how the singular slot is reached from a multi-forge config schema.
