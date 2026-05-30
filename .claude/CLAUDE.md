# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build
cargo run
cargo test
cargo test <test_name>         # Run a specific test
cargo make coverage            # Check test coverage
cargo clippy
cargo fmt

# Generate static binaries
cargo make release                 # Build all release targets
cargo make release-linux-x86_64    # x86_64 Linux (musl static, via cargo-zigbuild)
cargo make release-linux-aarch64   # ARM64 Linux (musl static, via cargo-zigbuild)
cargo make release-linux-riscv64   # RISC-V Linux (musl static, via cargo-zigbuild)
cargo make release-macos-x86_64    # x86_64 macOS (via cargo-zigbuild)
cargo make release-macos-aarch64   # ARM64 macOS (via cargo-zigbuild)
cargo make release-windows-x86_64  # x86_64 Windows (MSVC + crt-static, Windows host only)
cargo make release-windows-aarch64 # ARM64 Windows (MSVC + crt-static, Windows host only)
```

## Development Environment

The flake supports three systems: x86_64-linux, aarch64-linux, and aarch64-darwin. The dev shell provides rustc (nightly), cargo, rustfmt, clippy, rust-analyzer, cargo-make, cargo-llvm-cov, zig, and cargo-zigbuild. Windows targets require a native Windows host with MSVC.

## Workspace Structure

Two-crate Cargo workspace:

- `packages/cursus` — library crate; all domain logic, fully async (tokio)
- `packages/cursus-bin` — binary crate; entry point, logging setup, environment detection, tokio runtime

## Testing

Integration tests live in `packages/cursus/tests/` (library) and `packages/cursus-bin/tests/` (subprocess/clap tests). Always pass `--no-interactive` to prevent the TUI from running. The `cursus` crate exposes a `test-support` feature flag for mock implementations.

## Architecture

Cursus is a release management CLI. Dependencies are injected via `Env` (`env.rs`) rather than globals; environment detection (forge selection, dry-run, interactive mode) happens only at the binary boundary in `main.rs`. All command execution and file I/O flows through `Env`.

Key abstractions in `packages/cursus/src/`:

- `Env` — DI container: `CommandRunner`, `Filesystem`, `Git`, `CodeForgeClient`
- `cli/` — clap subcommands: `init`, `change` (default), `prepare`, `publish`, `ci`, `verify`
- `model/` — domain types: `Config` (`.cursus/config.toml`), changesets (`.cursus/`), changelog
- `package_manager/` — `PackageManagerAdapter` trait; Cargo and npm/yarn/pnpm impls
- `git/` — `Git` trait; `GitWorkdir` production impl; signed-commit decorators for GitHub (ADR-050) and GitLab (ADR-058)
- `forge/` — `CodeForgeClient` trait; GitHub (octocrab) and GitLab impls
- `tui/` — ratatui/crossterm TUI wizards
- `locale.rs` — i18n via `fluent-templates`, messages embedded at compile time (ADR-034)
