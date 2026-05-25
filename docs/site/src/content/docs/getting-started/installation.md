---
title: Installation
description: How to install Cursus
---

## Static binaries

Pre-built static binaries are available from [GitHub Releases](https://github.com/zantarix/cursus/releases) for:

| Platform | Architecture |
|----------|-------------|
| Linux    | x86_64, aarch64, riscv64 |
| macOS    | x86_64, aarch64 |
| Windows  | x86_64, aarch64 |

Download the binary for your platform, make it executable, and place it on your `PATH`.

### Verifying a download

Every released binary is signed with a Sigstore-backed GitHub artifact attestation, and the attestation bundle is published as a Release asset named `<binary>.sigstore.json` alongside the binary. This lets you verify a download on any platform using [cosign](https://github.com/sigstore/cosign) v2.4.0 or later:

```bash
VERSION=<x.y.z>
BASE="https://github.com/zantarix/cursus/releases/download/cursus@${VERSION}"

# Download the binary and its co-located bundle
curl -fsSLO "${BASE}/cursus-linux-x86_64"
curl -fsSLO "${BASE}/cursus-linux-x86_64.sigstore.json"

# Verify offline against the Sigstore public-good trust root
cosign verify-blob-attestation \
  --bundle cursus-linux-x86_64.sigstore.json \
  --new-bundle-format \
  --certificate-identity "https://github.com/zantarix/cursus/.github/workflows/release-artifacts.yml@refs/tags/cursus@${VERSION}" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  cursus-linux-x86_64
```

Substitute the artifact name for your platform (for example `cursus-osx-aarch64`, `cursus-windows-x86_64.exe`, or `cursus-linux-riscv64gc` for riscv64); the bundle asset is always the binary name with `.sigstore.json` appended. Verification fails closed if the binary or bundle has been tampered with. This is the same bundle the npm package verifies against. See [ADR-061](https://github.com/zantarix/cursus/blob/main/docs/adr/061-token-free-cross-platform-artifact-verification.md) and the [security policy](https://github.com/zantarix/cursus/blob/main/docs/SECURITY.md) for the full trust chain.

## npm

If you use Node.js, you can install Cursus via npm. The package downloads the appropriate static binary for your platform during `postinstall`. Each download is verified against its co-located Sigstore attestation bundle (the `<binary>.sigstore.json` Release asset) to ensure the build is an official one. See the [security policy](https://github.com/zantarix/cursus/blob/main/docs/SECURITY.md) for details on the verification chain and how to audit it manually.

```bash
npm install --save-dev @zantarix/cursus
```

This makes `cursus` available via `npx cursus` or in npm scripts.

## From source

With a Rust toolchain installed:

```bash
cargo install cursus-bin
```

This builds from source and installs the binary to `~/.cargo/bin/`.

## cargo-binstall

If you have [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) installed, you can install the prebuilt binary from GitHub Releases without compiling from source:

```bash
cargo binstall cursus-bin
```

This downloads the matching static binary for your host triple (including glibc Linux, which is mapped to the equivalent musl artifact) and installs it to `~/.cargo/bin/`. cargo-binstall verifies the download via HTTPS; for stronger identity-pinned provenance verification, follow the token-free [cosign steps above](#verifying-a-download-without-a-github-token) against the installed binary (downloading the matching `.sigstore.json` bundle for its artifact), or run `gh attestation verify --repo zantarix/cursus ~/.cargo/bin/cursus` if you have a GitHub token available.

## cargo-run-bin

If you pin tooling versions per-repository using [cargo-run-bin](https://github.com/dustinblackman/cargo-run-bin), you can declare cursus as a project-scoped binary in your own `Cargo.toml`:

```toml
[package.metadata.bin]
cursus-bin = { version = "<desired-version>" }
```

When cargo-binstall is also installed, cargo-run-bin will delegate to it and pick up the prebuilt binary, so the install is a fast download rather than a from-source build.

## Verify installation

```bash
cursus --version
```
