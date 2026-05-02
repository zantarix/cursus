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

The flake only supports three systems: x86_64-linux, aarch64-linux, and aarch64-darwin.

The dev shell provides: rustc (nightly), cargo, rustfmt, clippy, rust-analyzer, cargo-make, cargo-llvm-cov, zig, and cargo-zigbuild for cross-compilation to Linux and macOS targets. Windows targets require a native Windows host with MSVC and are not buildable from the dev shell.

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
  - `config/` - `Config` and `PackageManager` types, TOML persistence in `.cursus/config.toml`. `Config` exposes `load_projects` (applies `[global].ignore` filtering), `load_all_projects` (no filtering, for attribution scope), and `load_projects_partitioned` (returns both in a single enumeration pass — prefer this when you need both).
  - `changeset/` - Changeset file I/O: Hugo-style `+++` TOML frontmatter format, parsing, writing to `.cursus/`, and editor integration
  - `changelog/` - Changelog generation and formatting for releases
- `package_manager/` - Adapter pattern (`PackageManagerAdapter` trait: `enumerate_projects`, `write_version`, `update_dependency_version`, `update_lock_file`, `publish`, `registry_name`) for Cargo and npm/yarn/pnpm workspace enumeration. Versions are returned via `ProjectInfo` from `enumerate_projects()`. Both `write_version` and `update_dependency_version` return `Result<Vec<PathBuf>>` — the files actually written to disk (or that would be written in dry-run mode); callers must extend their `modified_files` list with these paths so they are staged for git. For Cargo crates using `version.workspace = true`, `write_version` returns the workspace-root `Cargo.toml` (not the member manifest). `Project` exposes two release-readiness query methods: `is_releasable_under(&Config)` (true if the project is publishable or listed in `[git].publish_private_packages`) and `is_prepared_for_release(&dyn Filesystem)` (true if `CHANGELOG.md` exists at the project root). The `matching` submodule exposes two public functions for file-to-project attribution: `match_files_to_projects` (primitive, pass the complete project list) and `match_files_to_projects_in_scope` (preferred when `projects` is a filtered subset — takes a wider `attribution_scope` so files inside ignored sub-projects are not mis-attributed to their releasable parents; used by `change`). The `name_validation` submodule (`pub(crate)`) provides `validate_cargo_package_name` and `validate_npm_package_name`, called during `enumerate_projects` for both adapters. Any new adapter must call the appropriate validator on each manifest-sourced package name before populating `ProjectInfo.name`.
- `git/` - `Git` trait abstracting all git operations; includes `head_sha()` returning the full SHA of the current HEAD commit. `GitWorkdir` is the production impl that delegates to the `git` binary via `CommandRunner`. `SignedCommitGit` is a decorator that wraps `GitWorkdir` and overrides `commit()`, `push()`, and `force_push_branch()` to route the prepare commit through the GitHub Git Data API, producing Verified commits via GitHub's web-flow GPG key (ADR-050). All alternative `Git` impls must implement `head_sha()`. The `ref_format` submodule (`pub(crate)`) provides `validate_branch_name`, `validate_tag_name`, and `validate_revision`; every `GitWorkdir` method that accepts a caller-supplied branch, tag, or revision string calls the appropriate validator before invoking the `CommandRunner`. Any new git operation that takes such a string must do the same. `diff_names` does not validate its `extra_args` slice — that is a caller contract documented on the method.
- `github/` - `CodeForgeClient` trait; `OctocrabGitHubClient` is the production impl. Handles release creation, PRs, asset uploads, and pre-creation idempotency checks (`find_release_by_tag` — returns `Ok(None)` on 404, `Ok(Some(ExistingRelease { is_draft, .. }))` on hit, error otherwise; every new impl must provide this method).
- `filesystem.rs` - `Filesystem` trait abstracting all file I/O; `LocalFilesystem` is the production impl using `tokio::fs`.
- `command/` - `CommandRunner` trait with `run` (read-only) and mutating variants (`run_mut`, `run_interactive`, `run_shell_interactive`, `run_streaming`). `run_streaming` executes user-configurable shell commands (`github.build_command`, `npm.lock_command`) with inherited stdout/stderr so output appears live; stdin is null. `DryRunCommandRunner` decorator implements the ADR-017 late-guard dry-run pattern (skips mutating ops, forwards read-only); `VerboseCommandRunner` logs invocations.
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
