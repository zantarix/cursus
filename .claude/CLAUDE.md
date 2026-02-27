# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build                    # Build the project
cargo run                      # Run the application
cargo test                     # Run tests
cargo test <test_name>         # Run a specific test
cargo make coverage            # Check test coverage (90% for lines/regions/functions, 80% for branches)
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

Never write production code that panics. Avoid `unwrap()`, `expect()`, `panic!()`, and `unreachable!()` outside of tests. Use `anyhow::Result`, `context()`, or `bail!()` to propagate errors instead.

## Architecture Decision Register

Big decisions are documented in ADR format in the `docs/adr/` folder.

Any changes to the `docs/adr/` folder should be handled by delegating to the `@adr-architect` agent.

## Testing

Integration tests live in `tests/` and should always use the `--no-interactive` flag to ensure the TUI never runs during tests. They should be full end-to-end tests calling `chronicle::run()` as the entrypoint.

**Non-interactive CLI flags for tests:**
- `change`: `--change-type/-t` (major/minor/patch), `--message/-m`, `--project/-p` (repeatable, defaults to all)
- `release`: `--dry-run`, `--package/-p` (repeatable, filters which packages to release)

**Git root discovery:** `run()` walks up the directory tree to find the `.git` directory. Integration tests must set up a git repo in their temp directory.

## Architecture

Chronicle is a Rust CLI tool for release management. It uses an interactive TUI for setup and change recording.

**Key modules:**
- `src/cli/` - clap-based CLI with `GlobalArgs` (`--interactive`/`--no-interactive`) and subcommands (`init`, `change`, `release`). `change` is the default when no subcommand is given.
- `src/tui/` - ratatui/crossterm terminal UI wizards
- `src/model/` - Core domain types:
  - `config.rs` - `Config` and `PackageManager` types, TOML persistence in `.chronicle/config.toml`
  - `changeset.rs` - Changeset file I/O: Hugo-style `+++` TOML frontmatter format, parsing, writing to `.chronicle/`, and editor integration
  - `changelog.rs` - Changelog generation and formatting for releases
- `src/package_manager/` - Adapter pattern (`PackageManagerAdapter` trait: `enumerate_projects`, `read_version`, `write_version`, `update_lock_file`) for Cargo and npm/yarn/pnpm workspace enumeration

**TUI pattern:** Each TUI wizard uses a `Screen` enum for state, a pure `handle_key()` function for state transitions (testable without a terminal), and separate `ui()`/`render_*()` functions. Tests use `ratatui::backend::TestBackend`.

**Changeset file format:**
```
+++
package-name = "minor"
+++

Description message here
```

## Non-functional Requirements

All new changes should meet the coverage thresholds:
- 90% for lines, regions, and functions
- 80% for branches

All functions which are made public from a module should be documented.

All significant changes as described by that agents description should be checked with the `code-reviewer` subagent. This check is separate from any plan approvals by the user as it is intended to validate the implementation of the plan.
