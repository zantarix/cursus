# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build                    # Build the project
cargo run                      # Run the application
cargo test                     # Run tests
cargo test <test_name>         # Run a specific test
cargo make coverage            # Check test coverage
cargo clippy                   # Lint the code
cargo fmt                      # Format the code

# Generate static binaries (all via cargo-zigbuild)
cargo make release                 # Build all release targets
cargo make release-linux-x86_64    # x86_64 Linux (musl static)
cargo make release-linux-aarch64   # ARM64 Linux (musl static)
cargo make release-linux-riscv64   # RISC-V Linux (musl static)
cargo make release-macos-x86_64    # x86_64 macOS
cargo make release-macos-aarch64   # ARM64 macOS
cargo make release-windows-x86_64  # x86_64 Windows (GNULLVM)
cargo make release-windows-aarch64 # ARM64 Windows (GNULLVM)
```

## Development Environment

The flake only supports three systems: x86_64-linux, aarch64-linux, and aarch64-darwin.

The dev shell provides: rustc (nightly), cargo, rustfmt, clippy, rust-analyzer, cargo-make, cargo-llvm-cov, zig, and cargo-zigbuild for cross-compilation to all targets.

## Workspace Structure

This is a Cargo workspace with two members:

- `packages/cursus` — library crate; all domain logic lives here
- `packages/cursus-bin` — binary crate; entry point, logging setup, environment detection, tokio runtime

All module paths below are relative to `packages/cursus/src/`.

## Testing

Integration tests live in `packages/cursus/tests/` (library) and `packages/cursus-bin/tests/` (subprocess/clap tests). Always pass `--no-interactive` to prevent the TUI from running. The `cursus` crate exposes a `test-support` feature flag for mock implementations.

## Architecture

Cursus is a Rust CLI tool for release management. The library is fully async (tokio). Dependencies are injected via `Env` rather than globals; environment detection happens only at the binary boundary in `main.rs`.

**Key modules:**

- `cli/` - clap-based CLI with `GlobalArgs` (`--interactive`/`--no-interactive`, `-v`/`-s`, `--dry-run`) and subcommands (`init`, `change`, `prepare`, `publish`, `ci`, `verify`). `change` is the default when no subcommand is given. `ci` auto-detects repo state and dispatches to `prepare` or `publish`. `verify` checks that the current branch adds at least one changeset vs a base ref (default `origin/HEAD`), returning exit code 2 if none found. `prepare/` is a multi-file submodule covering versioning, dependency propagation, linked versions, changelog, git lifecycle, and GitHub PRs.
- `tui/` - ratatui/crossterm terminal UI wizards
- `model/` - Core domain types:
  - `config/` - `Config` and `PackageManager` types, TOML persistence in `.cursus/config.toml`
  - `changeset/` - Changeset file I/O: Hugo-style `+++` TOML frontmatter format, parsing, writing to `.cursus/`, and editor integration
  - `changelog/` - Changelog generation and formatting for releases
- `package_manager/` - Adapter pattern (`PackageManagerAdapter` trait: `enumerate_projects`, `write_version`, `update_lock_file`, `publish`, `registry_name`) for Cargo and npm/yarn/pnpm workspace enumeration. Versions are returned via `ProjectInfo` from `enumerate_projects()`.
- `git/` - `Git` trait abstracting all git operations; `GitWorkdir` is the production impl that delegates to the `git` binary via `CommandRunner`.
- `github/` - `CodeForgeClient` trait; `OctocrabGitHubClient` is the production impl. Handles release creation, PRs, and asset uploads.
- `filesystem.rs` - `Filesystem` trait abstracting all file I/O; `LocalFilesystem` is the production impl using `tokio::fs`.
- `command/` - `CommandRunner` trait with `run`/`run_mut`/`run_shell` variants; `DryRunCommandRunner` decorator implements the ADR-017 late-guard dry-run pattern (skips mutating ops, forwards read-only); `VerboseCommandRunner` logs invocations.
- `env.rs` - `Env` struct: dependency injection container holding `Arc<dyn CommandRunner>`, `Arc<dyn Filesystem>`, `Arc<dyn Git>`, `Option<Arc<dyn CodeForgeClient>>`, editor, locale, and environment flags. Builder methods compose runners (e.g. `with_dry_run_runner()`). All command execution and file I/O goes through `Env`.
- `conventional_commit.rs` - Parser for Conventional Commits; maps `feat`→Minor, `fix`→Patch, breaking→Major via `ConventionalCommit::change_type()`
- `locale.rs` - i18n via `fluent-templates`; messages are embedded at compile time (ADR-034).
- `path.rs` - `AbsolutePath` newtype wrapping validated absolute `PathBuf`

**TUI pattern:** Each TUI wizard uses a `Screen` enum for state, a pure `handle_key()` function for state transitions (testable without a terminal), and separate `ui()`/`render_*()` functions. Tests use `ratatui::backend::TestBackend`.

**Changeset file format:**

```
+++
package-name = "minor"
+++

Description message here
```
