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

## Verify installation

```bash
cursus --version
```
