# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build              # Build the project
cargo run                # Run the application
cargo run -- -i          # Run in interactive TUI mode
cargo run -- -n "Name"   # Run with a name argument
cargo test               # Run tests
cargo clippy             # Lint the code
cargo fmt                # Format the code

# Generate static binaries
make release                 # Build all release targets
make release-x86_64          # x86_64 Linux (musl static)
make release-aarch64         # ARM64 Linux (musl static)
make release-macos           # ARM64 macOS (via cargo-zigbuild)
```

## Development Environment

This project uses Nix flakes and direnv for development. You should be running inside a dev shell already. If something
appears missing then prompt the user to restart you.

The dev shell provides: rustc, cargo, rustfmt, clippy, rust-analyzer, and musl cross-compilation toolchain for static binaries.

## Architecture

Chronicle is a Rust CLI application with interactive TUI support built on ratatui and crossterm.

**Key dependencies:**
- `clap` - Command-line argument parsing with derive macros
- `ratatui` - Terminal UI framework
- `crossterm` - Cross-platform terminal manipulation
- `anyhow` - Error handling

**Application flow:**
- CLI arguments are parsed via clap's `Parser` derive
- If `-i/--interactive` flag is set or no name is provided, launches the TUI
- Otherwise, runs in simple CLI mode

**TUI structure (src/main.rs):**
- `App` struct holds application state (input text, submission status)
- `run_tui()` manages terminal setup/teardown and the event loop
- `ui()` renders the interface using ratatui's widget system
- Uses crossterm's alternate screen and raw mode for proper terminal handling

Uses Rust 2024 edition.
