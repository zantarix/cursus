# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build                    # Build the project
cargo run                      # Run the application
cargo test                     # Run tests
cargo test <test_name>         # Run a specific test
cargo make coverage            # Check test coverage (90% threshold)
cargo clippy                   # Lint the code
cargo fmt                      # Format the code

# Generate static binaries
cargo make release             # Build all release targets
cargo make release-x86_64      # x86_64 Linux (musl static)
cargo make release-aarch64     # ARM64 Linux (musl static)
cargo make release-macos       # ARM64 macOS (via cargo-zigbuild)
```

## Development Environment

This project uses Nix flakes and direnv for development. The flake only supports three systems: x86_64-linux, aarch64-linux, and aarch64-darwin. You should be running inside a dev shell already. If something appears missing then prompt the user to restart you.

The dev shell provides: rustc (nightly), cargo, rustfmt, clippy, rust-analyzer, cargo-make, cargo-llvm-cov, and musl cross-compilation toolchain for static binaries (Linux only).

## Code Style

Format code before making any commit. Prefer functional style over imperative.

Uses Rust 2024 edition.

## Testing

Integration tests live in `tests/` and should always use the `--no-interactive` flag to ensure the TUI never runs during tests. They should be full end-to-end tests calling `chronicle::run()` as the entrypoint.

## Architecture

Chronicle is a Rust CLI tool for release management. It uses an interactive TUI for setup and change recording.

**Key modules:**
- `src/cli/` - clap-based CLI with `GlobalArgs` (`--interactive`/`--no-interactive`) and subcommands (`init`, `change`, `release`). `change` is the default when no subcommand is given.
- `src/tui/` - ratatui/crossterm terminal UI wizards
- `src/model/` - Core domain types:
  - `config.rs` - `Config` and `PackageManager` types, TOML persistence in `.chronicle/config.toml`
  - `changeset.rs` - Changeset file I/O: Hugo-style `+++` TOML frontmatter format, parsing, writing to `.chronicle/`, and editor integration
  - `changelog.rs` - Changelog generation and formatting for releases
- `src/package_manager/` - Adapter pattern (`PackageManagerAdapter` trait) for Cargo and npm/yarn/pnpm workspace enumeration

**TUI pattern:** Each TUI wizard uses a `Screen` enum for state, a pure `handle_key()` function for state transitions (testable without a terminal), and separate `ui()`/`render_*()` functions. Tests use `ratatui::backend::TestBackend`.

**Changeset file format:**
```
+++
package-name = "minor"
+++

Description message here
```

## Non-functional Requirements

All new changes should meet the 90% test coverage threshold.

All functions which are made public from a module should be documented.
