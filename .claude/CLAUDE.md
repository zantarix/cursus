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

This project uses Nix flakes and direnv for development. The flake only supports three systems: x86_64-linux, aarch64-linux, and aarch64-darwin. You should be running inside a dev shell already. If something appears missing then prompt the user to restart you.

The dev shell provides: rustc (nightly), cargo, rustfmt, clippy, rust-analyzer, cargo-make, cargo-llvm-cov, zig, and cargo-zigbuild for cross-compilation to all targets.

## Code Style

Format code before making any commit. Prefer functional style over imperative.

Uses Rust 2024 edition.

Never write production code that panics. Avoid `unwrap()`, `expect()`, `panic!()`, and `unreachable!()` outside of tests. Use `anyhow::Result`, `context()`, or `bail!()` to propagate errors instead.

## Architecture Decision Register

Big decisions are documented in ADR format in the `docs/adr/` folder. To see what ADRs exist, their titles, statuses, and summaries, consult `docs/adr/README.md`.

Any changes to the `docs/adr/` folder should be handled by delegating to the `@adr-architect` agent.

## Testing

Integration tests live in `tests/` and should always use the `--no-interactive` flag to ensure the TUI never runs during tests. They should be full end-to-end tests calling `cursus::run()` as the entrypoint.

**Non-interactive CLI flags for tests:**

- `change`: `--change-type/-t` (major/minor/patch), `--message/-m`, `--project/-p` (repeatable, defaults to all)
- `prepare`: `--package/-p` (repeatable, filters which packages to prepare)
- `--dry-run` is a global flag on `GlobalArgs` and can be passed to any subcommand

**Shared test utilities:** `tests/common/mod.rs` provides helpers shared across integration test files. Two categories of git helpers exist: `temp_git_repo*` (fake `.git` folder, fast, for tests not needing real git operations) and `temp_real_git_repo*` (proper repo with commits, required when `rev-list`, `diff-tree`, or push/fetch operations run). Use `run_cursus_subprocess` instead of `run_cursus` when testing clap-generated output (help, version, invalid flags).

**Git root discovery:** `run()` walks up the directory tree to find the `.git` directory. Integration tests must set up a git repo in their temp directory.

## Architecture

Cursus is a Rust CLI tool for release management. It uses an interactive TUI for setup and change recording.

**Key modules:**

- `src/cli/` - clap-based CLI with `GlobalArgs` (`--interactive`/`--no-interactive`, `-v`/`-s`, `--dry-run`) and subcommands (`init`, `change`, `prepare`, `publish`, `ci`, `verify`). `change` is the default when no subcommand is given. `ci` auto-detects repo state and dispatches to `prepare` or `publish`. `verify` checks that the current branch adds at least one changeset vs a base ref (default `origin/HEAD`), returning exit code 2 if none found.
- `src/tui/` - ratatui/crossterm terminal UI wizards
- `src/model/` - Core domain types:
  - `config.rs` - `Config` and `PackageManager` types, TOML persistence in `.cursus/config.toml`
  - `changeset.rs` - Changeset file I/O: Hugo-style `+++` TOML frontmatter format, parsing, writing to `.cursus/`, and editor integration
  - `changelog.rs` - Changelog generation and formatting for releases
- `src/package_manager/` - Adapter pattern (`PackageManagerAdapter` trait: `enumerate_projects`, `write_version`, `update_lock_file`, `publish`, `registry_name`) for Cargo and npm/yarn/pnpm workspace enumeration. Versions are returned via `ProjectInfo` from `enumerate_projects()`.
- `src/git/` - Git lifecycle management (config and operations)
- `src/github/` - GitHub release creation, PRs, and asset uploads
- `src/command.rs` - `CommandRunner` trait with `run`/`run_mut`/`run_shell` variants; `DryRunCommandRunner` decorator implements the ADR-017 late-guard dry-run pattern
- `src/env.rs` - Dependency injection and runner composition
- `src/conventional_commit.rs` - Parser for Conventional Commits; maps `feat`→Minor, `fix`→Patch, breaking→Major via `ConventionalCommit::change_type()`
- `src/path.rs` - `AbsolutePath` newtype wrapping validated absolute `PathBuf`

**TUI pattern:** Each TUI wizard uses a `Screen` enum for state, a pure `handle_key()` function for state transitions (testable without a terminal), and separate `ui()`/`render_*()` functions. Tests use `ratatui::backend::TestBackend`.

**Changeset file format:**

```
+++
package-name = "minor"
+++

Description message here
```

## Mutation Testing

Mutation tests are run manually by the user with `cargo mutants`. The results appear in `mutants.out/missed.txt`. Use the `analyse-mutations` skill to work through them.

There are two valid ways to address a missed mutant — adding tests is not always the right answer:

1. **Add a test** that exercises the mutated code path and would fail if the mutation were applied.
2. **Simplify the code** if the mutation is equivalent (i.e. the condition behaves identically either way). For example, a redundant guard condition can simply be removed, or a manual `if x < y { v = x }` pattern can be replaced with `v = v.min(x)`. Prefer this when the code genuinely has no meaningful distinction between the original and the mutant.

Use `#[mutants::skip]` only for entry points like `main()` that cannot meaningfully be tested.

## Non-functional Requirements

All new changes should meet the coverage thresholds:

- 90% for lines, regions, and functions
- 80% for branches

All functions which are made public from a module should be documented.

All significant changes as described by that agents description should be checked with the `code-reviewer` subagent. This check is separate from any plan approvals by the user as it is intended to validate the implementation of the plan. You should automatically fix all critical or major issues before providing the user a summary of the review.

When summarising changes made where the `code-reviewer` subagent was involved, you must include a list of fixes which were applied, as well as a list of any fixes not applied. Any other feedback should be included in it's own section. A summary of each of the points raised by the reviewer should end up in one of these three sections.
