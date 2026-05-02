# ADR-054: Add cargo-binstall Support for Prebuilt Binary Installation

## Status

Accepted (2026-05-02)

## Context

Cursus is a Rust CLI tool whose distribution strategy was established by [ADR-022](022-distribution-strategy.md): GitHub Releases serves as the primary, registry-agnostic channel and a `@zantarix/cursus` npm package serves Node.js users via a postinstall-download flow. The Rust ecosystem itself has only one first-class install path today: `cargo install cursus-bin`, which builds the entire crate tree from source. On a typical workstation this is a multi-minute compile that involves downloading and building the full async dependency graph (tokio, octocrab, ratatui, sigstore-related crates, and more). Users on slower hardware, in containers without a Rust toolchain warm cache, or in CI environments that just want a working binary feel this cost on every fresh install.

Meanwhile, the cursus release pipeline already produces fully static prebuilt binaries for all seven supported targets ([ADR-022](022-distribution-strategy.md), amended by [ADR-048](048-native-windows-build-runner.md)) and uploads them to each GitHub Release ([ADR-005](005-github-releases.md)) under a stable naming convention:

- `cursus-linux-x86_64` (`x86_64-unknown-linux-musl`)
- `cursus-linux-aarch64` (`aarch64-unknown-linux-musl`)
- `cursus-linux-riscv64gc` (`riscv64gc-unknown-linux-musl`)
- `cursus-osx-x86_64` (`x86_64-apple-darwin`)
- `cursus-osx-aarch64` (`aarch64-apple-darwin`)
- `cursus-windows-x86_64.exe` (`x86_64-pc-windows-msvc`)
- `cursus-windows-aarch64.exe` (`aarch64-pc-windows-msvc`)

These artifacts are attached to release tags using the `cursus@<version>` format (note the `@`, not the more common `v` prefix), so the canonical download URL for each binary is `https://github.com/zantarix/cursus/releases/download/cursus@<version>/<artifact-name>`. Linux artifacts are statically linked against musl and run correctly on glibc systems without modification.

cargo-binstall is the de facto Rust-ecosystem tool for installing prebuilt binaries. By adding a `[package.metadata.binstall]` section to a binary crate's `Cargo.toml`, project maintainers tell cargo-binstall where to find prebuilt artifacts and how to map the user's host triple to a downloadable file. With that metadata in place, `cargo binstall cursus-bin` resolves the matching artifact, downloads it, verifies the TLS certificate of the host, and installs it into `~/.cargo/bin/`, taking seconds instead of minutes. cargo-binstall also underpins per-project binary pinning tools such as cargo-run-bin, which transparently delegates to cargo-binstall when it is available — so adding binstall metadata also unlocks the cargo-run-bin path for downstream projects that pin tooling versions in their own `Cargo.toml`.

[ADR-022](022-distribution-strategy.md) explicitly considered cargo-binstall and explicitly *deferred* it: "This was not rejected — it is complementary to this ADR and may be added later." The deferral is now being acted on. [ADR-053](053-npm-package-node-spawner.md), the in-flight redesign of the npm package's bin entry, also forward-references this decision in two places ("an upcoming proposed cargo-binstall ADR for the Rust ecosystem"); accepting this ADR closes that loop.

A non-trivial part of this design is the integrity story. cargo-binstall has its own opinions about how cryptographic verification should work, and they do not align cleanly with the verification infrastructure cursus already operates:

- cargo-binstall's only native cryptographic verification mechanism is `[package.metadata.binstall.signing]` with `algorithm = "minisign"`. Sigstore is listed as a future direction in its `SIGNING.md` ("we're especially interested in Sigstore for a better implementation of just-in-time signing") but is not implemented today.
- cargo-binstall does not consume a `SHA256SUMS` file from a GitHub Release. Its security posture for the GitHub Release fetch is HTTPS with TLS 1.2 or higher, and that is the trust root unless the project opts into minisign signing.
- Cursus already invested in a different, stronger trust infrastructure for binary artifacts in [ADR-049](049-signed-release-artifacts.md): every release binary is signed via Sigstore (Fulcio for keyless certificates, Rekor for the public transparency log) using GitHub Actions OIDC, and the npm postinstall script verifies that attestation against an identity-pinned expected workflow before writing the binary. `gh attestation verify --repo zantarix/cursus <binary>` is the user-facing CLI that exercises the same verification path on demand.

That asymmetry — binstall's signing model versus cursus's existing Sigstore investment — drives the integrity decision below. Adopting binstall does not justify either standing up a parallel minisign infrastructure or publishing a `SHA256SUMS` file that binstall would not consume.

## References

- [cargo-binstall configuration schema](https://github.com/cargo-bins/cargo-binstall/blob/main/SUPPORT.md)
- [cargo-binstall signing model](https://github.com/cargo-bins/cargo-binstall/blob/main/SIGNING.md)
- [cargo-binstall security overview](https://github.com/cargo-bins/cargo-binstall/blob/main/README.md)
- [cargo-run-bin](https://github.com/dustinblackman/cargo-run-bin)
- [GitHub artifact attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds)
- [`gh attestation verify` CLI reference](https://cli.github.com/manual/gh_attestation_verify)

## Decision

We will add `[package.metadata.binstall]` configuration to `packages/cursus-bin/Cargo.toml` so that `cargo binstall cursus-bin` downloads the matching prebuilt binary from the corresponding GitHub Release instead of compiling cursus from source. We will not introduce a new signing infrastructure for this channel; instead we will rely on the existing GitHub TLS trust root for the download itself and document `gh attestation verify` as the opt-in manual verification ladder for users who want to exercise the [ADR-049](049-signed-release-artifacts.md) attestation chain against a binstall-installed binary.

### Binstall metadata layout

The binstall metadata in `packages/cursus-bin/Cargo.toml` shall:

- Set `pkg-url` to a template that resolves to `https://github.com/zantarix/cursus/releases/download/cursus@{version}/<artifact-name>`, reflecting the actual `cursus@<version>` tag format used by the publish pipeline.
- Set `pkg-fmt = "bin"` because the artifacts are raw uncompressed binaries, not archives. cargo-binstall must not attempt tarball or zip extraction.
- Provide explicit `[package.metadata.binstall.overrides.<triple>]` entries for all seven natively-built targets so the host-triple → artifact-name mapping is unambiguous and matches the actual filenames produced by the release workflows:
  - `x86_64-unknown-linux-musl` → `cursus-linux-x86_64`
  - `aarch64-unknown-linux-musl` → `cursus-linux-aarch64`
  - `riscv64gc-unknown-linux-musl` → `cursus-linux-riscv64gc`
  - `x86_64-apple-darwin` → `cursus-osx-x86_64`
  - `aarch64-apple-darwin` → `cursus-osx-aarch64`
  - `x86_64-pc-windows-msvc` → `cursus-windows-x86_64.exe`
  - `aarch64-pc-windows-msvc` → `cursus-windows-aarch64.exe`

### Linux glibc → musl mapping

In addition to the seven musl/MSVC/Apple targets above, the metadata shall include explicit overrides that map the three glibc Linux triples to the corresponding musl artifacts:

- `x86_64-unknown-linux-gnu` → `cursus-linux-x86_64`
- `aarch64-unknown-linux-gnu` → `cursus-linux-aarch64`
- `riscv64gc-unknown-linux-gnu` → `cursus-linux-riscv64gc`

A standard Rust toolchain on a typical Linux distribution targets the gnu variant by default. Without these overrides, `cargo binstall cursus-bin` on the most common Linux configuration would either fail to find an artifact or fall back to compiling from source — defeating the purpose of adding binstall support. The musl-static binaries cursus already ships run correctly on glibc systems because they have no dynamic libc dependency, so the override is purely a routing decision and adds no new build matrix entries.

### Integrity model

The integrity model for the binstall channel is:

1. **Transport trust**: GitHub TLS (≥ 1.2) for the artifact download. This is the same trust root [ADR-022](022-distribution-strategy.md) originally established for direct GitHub Release downloads, and it is the strongest guarantee cargo-binstall offers natively without minisign signing.
2. **Opt-in cryptographic verification**: users who want identity-pinned provenance verification of a binstall-installed binary can run `gh attestation verify --repo zantarix/cursus <binary>` against the binary in `~/.cargo/bin/`. This exercises the [ADR-049](049-signed-release-artifacts.md) Sigstore attestation chain (Fulcio certificate, Rekor inclusion proof, expected workflow identity) directly, without binstall needing to integrate it.

We will not add `[package.metadata.binstall.signing]` with minisign. We will not publish a `SHA256SUMS` file. The release workflow at `.github/workflows/release-artifacts.yml` is unchanged.

The asymmetry with the npm channel — which auto-verifies the same Sigstore attestation in postinstall — is intentional and is documented under Consequences below.

### Documentation

The installation page at `docs/site/src/content/docs/getting-started/installation.md` will be extended to add two new sections immediately after the existing "From source" block:

- A `cargo binstall cursus-bin` section that points at the binstall channel and notes that cargo-binstall must already be installed.
- A `cargo-run-bin` section that documents the per-project binary pinning path: downstream projects can pin a cursus version in their own `Cargo.toml` via cargo-run-bin, which will delegate to cargo-binstall when available, picking up this ADR's metadata for fast installs.

No cursus-side code or workflow changes are needed for cargo-run-bin beyond the binstall metadata.

## Consequences

### Positive

- `cargo binstall cursus-bin` becomes a fast install path for the Rust ecosystem, matching the user-experience promise that GitHub Releases (manual download) and the npm channel already offer. Install time drops from a multi-minute compile to a single artifact download.
- The cargo-run-bin path becomes viable transparently. Projects that pin their tool versions per-repo in `Cargo.toml` can now adopt cursus without forcing every contributor through a slow `cargo install` rebuild.
- The deferred alternative explicitly called out in [ADR-022](022-distribution-strategy.md) is now resolved, removing a long-standing "TODO" from the distribution strategy.
- Linux glibc users — the majority of the Rust-on-Linux population — get a binary install path out of the box rather than the awkward "no prebuilt binary, falling back to compile" failure mode that a musl-only binstall configuration would produce.
- No new release-pipeline work is needed. The existing seven artifacts and the existing `cursus@<version>` tag format are the binstall channel's substrate.

### Negative

- Asymmetric trust UX between distribution channels. The npm channel auto-verifies the [ADR-049](049-signed-release-artifacts.md) Sigstore attestation in postinstall; the binstall channel does not, and reaching the same identity-pinned guarantee requires a separate `gh attestation verify` invocation. Users who think of cargo-binstall as "the secure, attested install path" may have a mistaken mental model unless the documentation is explicit. This ADR codifies that asymmetry rather than hiding it.
- The binstall channel relies on GitHub Release artifact filenames remaining stable. Renaming any of the seven artifacts in the `[github.artifacts]` config (for example, changing `cursus-linux-riscv64gc` to drop the `gc`) is now a coordinated change that also requires updating `[package.metadata.binstall.overrides]` in `packages/cursus-bin/Cargo.toml`. The same coupling already exists for the npm postinstall script, so this is a known maintenance pattern rather than a new one.
- Extending support to a new target later requires three coordinated changes: a new release artifact, a new binstall override entry, and (for Linux) the corresponding gnu→musl override. A missed override is silent — users on the new triple just see "no prebuilt binary" — rather than a hard failure.

### Neutral

- The number of Cargo.toml manifest entries for `cursus-bin` grows by one `[package.metadata.binstall]` block plus one `[package.metadata.binstall.overrides.<triple>]` block per supported target (ten in total: seven natively-built plus three glibc aliases). This is metadata only and has no compile-time, runtime, or install-time cost for users who do not use cargo-binstall.
- Direct `cargo install cursus-bin` continues to work unchanged. Users who prefer compile-from-source (for instance, to apply local patches or to verify reproducibility) still have that path.
- The `gh attestation verify` command requires the GitHub CLI and an authenticated GitHub session; this is a soft prerequisite for the manual verification ladder, but it is an opt-in path so users who do not need it are unaffected.

## Alternatives Considered

### Add minisign signing for the binstall channel

cargo-binstall's `[package.metadata.binstall.signing]` block with `algorithm = "minisign"` is the only mechanism by which cargo-binstall itself verifies a downloaded artifact's signature today. Adopting it would mean: generating a minisign keypair, storing the secret key as a GitHub Actions secret, adding a per-platform signing step to `release-artifacts.yml`, publishing the public key alongside the binstall metadata, and committing to a key-rotation and incident-response process for that key.

This was rejected. It would duplicate the trust infrastructure [ADR-049](049-signed-release-artifacts.md) already established (Sigstore via `actions/attest`, Fulcio, Rekor) for the same artifacts, and it would introduce a new long-lived secret with all the custody concerns Sigstore was specifically chosen to avoid. The cost-benefit is poor for what is, by design, a secondary install channel: the user-visible verification surface of the binstall channel is at most "cargo-binstall said the signature checked out" — a strictly weaker statement than the SAN-pinned Sigstore identity check the npm channel performs. Users who want stronger guarantees already have `gh attestation verify` against the same binary.

### Publish a SHA256SUMS file to the GitHub Release

Add a `SHA256SUMS` file to each GitHub Release containing the digests of all seven binaries, signed or unsigned. Users could then run `sha256sum -c SHA256SUMS` after downloading.

This was rejected. cargo-binstall does not consume `SHA256SUMS` files as part of its native verification flow, so the file would do no work on the binstall install path. Users who manually wanted to check digests already have a strictly stronger option in `gh attestation verify`, which gives identity-pinned provenance rather than just a content hash. Publishing `SHA256SUMS` would be theatre that adds release-pipeline complexity without adding meaningful protection against the threat model [ADR-049](049-signed-release-artifacts.md) was designed to address.

### Provide musl overrides only (no glibc aliases)

A more minimal binstall configuration would list only the seven natively-built targets and leave it to the user to specify `--target x86_64-unknown-linux-musl` (or equivalent) when they want a binstall install on Linux.

This was rejected. The default Rust toolchain on the vast majority of Linux distributions targets the gnu triple, not musl. Without the explicit gnu overrides, `cargo binstall cursus-bin` on a stock Linux developer machine would fail to find a matching artifact and fall back to compilation — exactly the outcome this ADR exists to prevent. The musl-static binaries already run correctly on glibc systems, so the override is a free routing decision; the only cost is three additional metadata entries in `Cargo.toml`.

### Rely on cargo-binstall's `crate-meta-data` strategy without explicit overrides

cargo-binstall has heuristics for guessing artifact URLs from a crate's repository metadata. In principle, a binstall-friendly project could leave `[package.metadata.binstall]` unset and let those heuristics try to find binaries.

This was rejected. The cursus release pipeline uses a non-default tag format (`cursus@<version>` rather than `v<version>`) and a non-default artifact naming scheme (`cursus-linux-x86_64`, not `cursus-bin-x86_64-unknown-linux-musl.tar.gz`). The default heuristics will not match. Explicit `pkg-url` and per-target overrides are the correct and supported way to expose this layout to cargo-binstall.
