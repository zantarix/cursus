# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build              # Build the project
cargo run                # Run the application
cargo test               # Run tests
cargo test <test_name>   # Run a specific test
make coverage            # Check test coverage (90% threshold)
cargo clippy             # Lint the code
cargo fmt                # Format the code

# Generate static binaries
make release                 # Build all release targets
make release-x86_64          # x86_64 Linux (musl static)
make release-aarch64         # ARM64 Linux (musl static)
make release-macos           # ARM64 macOS (via cargo-zigbuild)
```

## Development Environment

This project uses Nix flakes and direnv for development. The flake only supports three systems: x86_64-linux, aarch64-linux, and aarch64-darwin. You should be running inside a dev shell already. If something appears missing then prompt the user to restart you.

The dev shell provides: rustc (nightly), cargo, rustfmt, clippy, rust-analyzer, cargo-llvm-cov, and musl cross-compilation toolchain for static binaries (Linux only).

## Code style

You should format code before making any commit.

Prefer functional style over imperative.

## Testing

Integration tests should always use the `--no-interactive` flag as part of the argument list they test in order to ensure that the TUI never runs during tests.

Integration tests should be full end-to-end tests and call `chronicle::run()` as the entrypoint of the test.

## Architecture

Chronicle is a Rust CLI tool for release management. It uses an interactive TUI for setup and change recording.

**Module structure:**
- `src/lib.rs` - Library entry point (`chronicle::run()`), git root detection
- `src/main.rs` - Binary entry point, error handling
- `src/cli/` - Command-line interface (clap)
  - `mod.rs` - `Cli` struct, `GlobalArgs` (`--interactive`/`--no-interactive`), `Command` enum
  - `init.rs` - `init` subcommand: creates `.chronicle/config.toml`
  - `change.rs` - `change` subcommand: records semantic version changes (default when no subcommand)
- `src/config.rs` - `Config` and `PackageManager` types, TOML persistence
- `src/tui/` - Terminal UI (ratatui/crossterm)
  - `init.rs` - Setup wizard with screen-based state machine
  - `change.rs` - Change type selector (major/minor/patch)
- `src/package_manager/` - Adapter pattern for package managers
  - `mod.rs` - `PackageManagerAdapter` trait, `Project` struct
  - `npm.rs` - npm/yarn/pnpm workspace support

**TUI pattern:**
- `Screen` enum represents wizard state
- `handle_key()` is a pure function for state transitions (testable without terminal)
- UI rendering in separate `ui()` and `render_*` functions
- Tests use `ratatui::backend::TestBackend`

Uses Rust 2024 edition.

## Non-functional requirements

All new changes should meet the 90% test coverage threshold.

All functions which are made public from a module should be documented.
