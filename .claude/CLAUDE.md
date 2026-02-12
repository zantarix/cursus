# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build              # Build the project
cargo run                # Run the application
cargo test               # Run tests
cargo test <test_name>   # Run a specific test
make coverage            # Check test coverage (80% threshold)
cargo clippy             # Lint the code
cargo fmt                # Format the code

# Generate static binaries
make release                 # Build all release targets
make release-x86_64          # x86_64 Linux (musl static)
make release-aarch64         # ARM64 Linux (musl static)
make release-macos           # ARM64 macOS (via cargo-zigbuild)
```

## Development Environment

This project uses Nix flakes and direnv for development. You should be running inside a dev shell already. If something appears missing then prompt the user to restart you.

The dev shell provides: rustc (nightly), cargo, rustfmt, clippy, rust-analyzer, cargo-llvm-cov, and musl cross-compilation toolchain for static binaries.

## Architecture

Chronicle is a Rust CLI tool that manages project configuration via an interactive TUI setup wizard.

**Module structure:**
- `src/main.rs` - Entry point, git root detection, orchestrates config loading/creation
- `src/config.rs` - Configuration types (`Config`, `PackageManager`) and TOML persistence to `.chronicle/config.toml`
- `src/tui/` - Terminal UI components
  - `init.rs` - Setup wizard with screen-based state machine (`Confirm` → `SelectPackageManager`)

**Application flow:**
1. Find git root by walking up from current directory
2. If no `.chronicle/config.toml` exists, launch TUI setup wizard
3. TUI auto-detects package manager (checks for `package.json` or `Cargo.toml`)
4. User confirms setup and selects package manager
5. Config is written and loaded

**TUI pattern:**
- `Screen` enum represents wizard state, `handle_key()` is a pure function for state transitions
- `setup()` manages terminal lifecycle (raw mode, alternate screen)
- UI rendering is separated into `render_*` functions per screen

Uses Rust 2024 edition.

## Non-functional requirements

All new changes should meet the 80% test coverage threshold.

All functions which are made public from a module should be documented.
