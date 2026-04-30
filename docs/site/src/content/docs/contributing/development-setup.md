---
title: Development Setup
description: How to set up a development environment for contributing to Cursus
---

## Option A: Dev Container

The quickest way to get started is with the included Dev Container, which works in VS Code, JetBrains IDEs, and GitHub Codespaces.

1. Install the [Dev Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers) (VS Code) or equivalent
2. Open the repository and select **Reopen in Container**
3. The container includes all required tools via Nix

For GitHub Codespaces, simply open the repository on GitHub and click **Code > Codespaces > New codespace**.

## Option B: Local with Nix

Cursus uses [Nix flakes](https://nixos.wiki/wiki/Flakes) and [direnv](https://direnv.net/) for a reproducible development environment.

1. Install Nix with flakes enabled
2. Install direnv and hook it into your shell
3. Clone the repository and `cd` into it
4. Run `direnv allow` when prompted

The Nix flake supports: `x86_64-linux`, `aarch64-linux`, `aarch64-darwin`.

The dev shell provides: `rustc` (nightly), `cargo`, `rustfmt`, `clippy`, `rust-analyzer`, `cargo-make`, `cargo-llvm-cov`, `zig`, and `cargo-zigbuild` for cross-compilation to Linux and macOS targets. Windows targets require a native Windows host with MSVC and are not buildable from the dev shell.

## Build commands

| Command | Description |
|---------|-------------|
| `cargo build` | Build the project |
| `cargo run` | Run the application |
| `cargo test` | Run tests |
| `cargo test <name>` | Run a specific test |
| `cargo make coverage` | Check test coverage |
| `cargo clippy` | Lint the code |
| `cargo fmt` | Format the code |

## JavaScript/TypeScript

`packages/npm` (the `@zantarix/cursus` npm wrapper) and `docs/site` (this documentation site) are independent npm projects — there is no workspace root. Install their dependencies separately:

```bash
cd packages/npm && npm install   # npm wrapper (sigstore verifier, download script)
cd docs/site && npm install      # documentation site (Astro/Starlight)
```

The Nix dev shell provides Node.js, so no separate Node installation step is needed under Option B.

## Code style

- **Rust 2024 edition**
- Prefer functional style over imperative
- Format code before every commit
- **No panicking in production code** — avoid `unwrap()`, `expect()`, `panic!()`, and `unreachable!()` outside of tests. Use `anyhow::Result`, `.context()`, or `bail!()` to propagate errors.

## Testing

- Integration tests live in `tests/` and must use `--no-interactive` to prevent the TUI from running
- Coverage thresholds: 90% for lines/regions/functions, 80% for branches
- Run `cargo make coverage` to check coverage locally

## Architecture decisions

Significant design decisions are documented as Architecture Decision Records (ADRs) in [`docs/adr/`](https://github.com/zantarix/cursus/tree/main/docs/adr#readme). Consult the [ADR index](https://github.com/zantarix/cursus/tree/main/docs/adr#readme) for an overview of all decisions and their statuses.
