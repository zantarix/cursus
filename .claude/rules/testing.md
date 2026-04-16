---
paths:
  - "**/tests/**"
  - "**/*_test.rs"
---

# Integration Testing

Integration tests live in `packages/cursus/tests/` (library) and `packages/cursus-bin/tests/` (subprocess/clap tests). Always pass `--no-interactive` to ensure the TUI never runs.

Library tests call `cursus::run()` as the entrypoint. Subprocess tests use `run_cursus_subprocess` (from `packages/cursus-bin/tests/common/`) which spawns the actual binary — use this when testing clap-generated output (help, version, invalid flags).

**Non-interactive CLI flags:**

- `change`: `--change-type/-t` (major/minor/patch), `--message/-m`, `--project/-p` (repeatable, defaults to all)
- `prepare`: `--package/-p` (repeatable, filters which packages to prepare)
- `--dry-run` is a global flag on `GlobalArgs` and can be passed to any subcommand

**Shared test utilities:** `packages/cursus/tests/common/mod.rs` provides helpers. Two categories of git helpers exist:

- `temp_git_repo*` — fake `.git` folder, fast, for tests that don't need real git operations
- `temp_real_git_repo*` — proper repo with commits, required when `rev-list`, `diff-tree`, or push/fetch operations run

**`test-support` feature:** The `cursus` crate exposes a `test-support` Cargo feature that gates test helpers compiled into the library (e.g. mock impls). Enable it in tests that need it.

**Git root discovery:** `run()` walks up the directory tree to find the `.git` directory. Integration tests must set up a git repo in their temp directory.
