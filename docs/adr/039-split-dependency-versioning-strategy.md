# ADR-039: Split Dependency Versioning Strategy Between Library and Binary Crates

## Status

Accepted

## Context

Cursus is published as two artifacts from a Cargo workspace: a library crate (`packages/cursus/`) published to crates.io for programmatic consumption, and a binary crate (`packages/cursus-bin/`) distributed through GitHub Releases as static binaries ([ADR-022](022-distribution-strategy.md)) and through crates.io via `cargo install cursus-bin`.

These two distribution channels impose opposing constraints on dependency version specifications:

- **Library on crates.io:** Cargo resolves a library's dependency versions at the consumer's build time. Exact-pinned versions (`=x.y.z`) in a published library's `Cargo.toml` force every downstream consumer onto that exact version, preventing them from receiving compatible patch or minor updates and creating version conflicts when multiple libraries pin different exact versions of the same crate.

- **Binary crate:** The primary distribution path (static binaries from GitHub Releases) compiles once in CI where `Cargo.lock` guarantees reproducibility regardless of what `Cargo.toml` specifies. However, `cargo install cursus-bin` is a supported secondary install path, and Cargo ignores the committed `Cargo.lock` during `cargo install`, resolving dependencies fresh from the version specs in `Cargo.toml`. With caret versions, a `cargo install` user could resolve newer compatible versions than what was tested in CI. With exact pins, they get precisely the dependency versions that were tested. Exact pins also make version intent explicit in the manifest and enable automated tooling to manage updates precisely.

The project uses Renovate for automated dependency updates. Renovate's Cargo manager operates on `[workspace.dependencies]` as a single `depType` with no per-crate granularity. When both crates share versions through `[workspace.dependencies]`, Renovate cannot apply different `rangeStrategy` values (e.g., `replace` for the library, `pin` for the binary). This is a fundamental limitation of Renovate's Cargo support, not a configuration oversight.

The bin/lib separation established by [ADR-030](030-bin-lib-crate-separation.md) already treats these crates as architecturally distinct. The dependency versioning strategy should reflect that same boundary.

## Decision

We will eliminate `[workspace.dependencies]` from the workspace root `Cargo.toml` and have each crate manage its own dependency versions directly.

The library crate (`packages/cursus/`) will use **caret versions** (e.g., `"1.0.102"`) for production `[dependencies]` and **exact-pinned versions** (e.g., `"=3.27.0"`) for `[dev-dependencies]`. Caret versions give downstream consumers flexibility to receive compatible updates. Dev-dependencies are pinned because they are never resolved by consumers and benefit from deterministic CI behavior.

The binary crate (`packages/cursus-bin/`) will use **exact-pinned versions** (e.g., `"=1.0.102"`) for both `[dependencies]` and `[dev-dependencies]`. Exact pins carry no downstream cost (nothing depends on the binary as a library), and they ensure that `cargo install cursus-bin` resolves the same dependency versions that were tested in CI -- since Cargo ignores the committed `Cargo.lock` during `cargo install`, the version specs in `Cargo.toml` are the only constraint on resolution. Exact pins also align with Renovate's `pin` range strategy.

`[workspace.package]` (version, edition, license) and `[workspace.lints]` will remain shared in the workspace root, as these are identity and policy fields unaffected by the versioning concern.

Renovate will be configured with `matchFileNames` rules to apply `rangeStrategy: "replace"` for `packages/cursus/Cargo.toml` and `rangeStrategy: "pin"` for `packages/cursus-bin/Cargo.toml`.

## Consequences

### Positive

- Downstream consumers of the library crate receive compatible patch and minor updates naturally through Cargo's semver resolution, rather than being locked to exact versions
- Renovate can apply crate-appropriate update strategies, producing correct PRs for each crate without manual intervention
- Version intent is explicit in each manifest -- a reader can immediately understand the versioning philosophy without consulting external documentation
- `cargo install cursus-bin` produces a binary with the same dependency versions tested in CI, because exact pins are the only constraint Cargo sees (it ignores the committed `Cargo.lock` during `cargo install`)
- Aligns with the architectural boundary established by [ADR-030](030-bin-lib-crate-separation.md): the library and binary are independent artifacts with independent concerns

### Negative

- Dependency versions may drift between crates, requiring attention when both crates depend on the same upstream crate at different versions (Cargo's resolver handles this, but divergent versions increase compile times and binary size)
- No single place to see all dependency versions at a glance; maintainers must check both crate manifests
- Updating a shared dependency requires editing two files instead of one (mitigated by Renovate automation)

### Neutral

- The workspace root `Cargo.toml` becomes smaller, containing only `[workspace]`, `[workspace.package]`, and `[workspace.lints]`
- `Cargo.lock` continues to be the source of truth for reproducible builds in both crates; this decision affects only the version ranges expressed in manifests
- The library's `[dev-dependencies]` use exact pins despite the library being published -- this is safe because dev-dependencies are never included in the published crate metadata

## Alternatives Considered

### Keep `[workspace.dependencies]` with caret versions everywhere

Use `[workspace.dependencies]` for deduplication and rely on `Cargo.lock` alone for binary reproducibility. Rejected because Renovate cannot distinguish library from binary dependencies within `[workspace.dependencies]` -- it applies a single `rangeStrategy` to the entire block. This means either the library gets undesirable exact pins or the binary loses explicit version control, with no way to satisfy both.

### Keep `[workspace.dependencies]` with exact pins everywhere

Pin all shared versions to exact in `[workspace.dependencies]`. Rejected because the library crate is published to crates.io, and exact pins in a published library's dependency metadata force downstream consumers onto those exact versions. This creates unnecessary version conflicts and prevents consumers from benefiting from compatible upstream updates.

### Duplicate only binary-unique dependencies

Keep shared dependencies in `[workspace.dependencies]` and only move binary-specific dependencies into the binary's own `Cargo.toml`. Rejected because the binary's dependencies are a near-complete subset of the library's dependencies (the binary primarily re-exports the library plus a thin CLI wrapper). There are no meaningful "binary-unique" dependencies that would benefit from this split, so the workspace-level block would still govern the majority of both crates' versions and the Renovate limitation would persist.
