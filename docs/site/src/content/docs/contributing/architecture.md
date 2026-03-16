---
title: Architecture
description: High-level overview of the Cursus codebase
---

Cursus is a Rust CLI application built with [clap](https://docs.rs/clap) for argument parsing and [ratatui](https://ratatui.rs/) for terminal UI. This page gives a high-level tour of the codebase — for detailed design rationale, see the [Architecture Decision Records](https://github.com/zantarix/cursus/tree/main/docs/adr).

## Module overview

| Module | Responsibility |
|--------|---------------|
| `src/cli/` | clap-based CLI with global flags and subcommands |
| `src/tui/` | Interactive terminal UI wizards (ratatui/crossterm) |
| `src/model/` | Core domain types: config, changesets, changelogs |
| `src/package_manager/` | Adapter trait for Cargo and npm workspace enumeration, versioning, and publishing |
| `src/git/` | Git lifecycle management (commit, tag, push, branch) |
| `src/github/` | GitHub API integration (releases, PRs, asset uploads) |
| `src/command.rs` | `CommandRunner` trait for shell command execution with dry-run support |
| `src/env.rs` | Dependency injection and runner composition |
| `src/conventional_commit.rs` | Conventional Commit parser |
| `src/path.rs` | `AbsolutePath` newtype for validated absolute paths |

## Key patterns

### Package manager adapters

The `PackageManagerAdapter` trait provides a uniform interface across package managers:

- `enumerate_projects()` — discover packages and their current versions
- `write_version()` — update a package's version in its manifest
- `update_lock_file()` — regenerate the lock file after version changes
- `publish()` — publish a package to its registry
- `registry_name()` — display name for the registry

### TUI wizards

Each TUI wizard follows a consistent pattern:

- A `Screen` enum tracks the current state
- A pure `handle_key()` function handles state transitions (testable without a terminal)
- Separate `ui()` / `render_*()` functions handle rendering

### Command execution

The `CommandRunner` trait abstracts shell command execution. The `DryRunCommandRunner` decorator wraps any runner and intercepts commands that would mutate state, logging them instead — implementing a late-guard dry-run pattern.

### Error handling

Cursus uses `anyhow::Result` throughout. Production code never panics — all errors are propagated with `.context()` or `bail!()`.

## API reference

For detailed type-level documentation, see the [rustdoc on docs.rs](https://docs.rs/cursus/latest/cursus/).
