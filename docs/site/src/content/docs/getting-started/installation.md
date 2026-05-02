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

## npm

If you use Node.js, you can install Cursus via npm. The package downloads the appropriate static binary for your platform during `postinstall`. All downloads are verified using a GitHub attestation to ensure that the downloaded builds are official builds. See the [security policy](https://github.com/zantarix/cursus/blob/main/docs/SECURITY.md) for details on the verification chain and how to audit it manually.

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

This downloads the matching static binary for your host triple (including glibc Linux, which is mapped to the equivalent musl artifact) and installs it to `~/.cargo/bin/`. cargo-binstall verifies the download via HTTPS; for stronger identity-pinned provenance verification, run `gh attestation verify --repo zantarix/cursus ~/.cargo/bin/cursus` against the installed binary.

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
