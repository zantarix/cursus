# Architectural Patterns from ADRs

## Three-Step Release Workflow

1. `chronicle prepare` (formerly `release`, renamed ADR-016) — filesystem changes + optional git branch management
2. Git operations — manual by default, opt-in automation via ADR-006/ADR-015
3. `chronicle publish` — registry publishing, tag creation/push, GitHub Releases

This separation is a core principle. Chronicle defaults to filesystem-only changes.

ADR-015 extends this with a CI-managed variant: `chronicle ci` infers which step to run based on repo state (changesets present = prepare, untagged/unpublished manifest versions = publish). `[git].strategy` field (`push` | `branch`, default derived from `[github].enabled`) controls how release changes are delivered. `branch` strategy: checkout release branch, commit there, push, auto-create PR if GitHub enabled, checkout back. Tags always created during `publish` after registry publishing -- never during prepare. Publish ordering: registry -> tags -> GitHub Releases (prevents state detection inconsistency on retry).

## Adapter Trait Pattern

`PackageManagerAdapter` trait handles: `enumerate_projects`, `read_version`, `write_version`, `update_lock_file`, `is_publishable`

- Cargo and npm adapters implement this
- GitHub is explicitly NOT a package manager adapter (ADR-005) — it's a post-publish action
- Future package managers extend via this trait

## Configuration Philosophy

- All config in `.chronicle/config.toml` (TOML format)
- Features are opt-in with sensible defaults
- `deny_unknown_fields` on config structs
- Config sections per concern: `[npm]`, `[cargo]`, `[github]`, `[git]`
- Each section has `enabled` toggle

## Interactive/Non-Interactive Duality

- Every command works in both interactive (TUI) and non-interactive (CLI flags) modes
- `--no-interactive` flag disables TUI; required flags substitute for TUI input
- Batch commands (release, publish) don't need TUI at all

## Authentication Strategy

- Chronicle NEVER manages credentials
- Delegates to environment: env vars (CARGO_REGISTRY_TOKEN, NPM_TOKEN, GITHUB_TOKEN) or tool config (.npmrc, cargo login)
- Auth failures produce clear error messages and non-zero exit codes

## Error Handling Philosophy

- Filesystem modifications are NOT rolled back on subsequent failures (e.g., git failures after release)
- "Version already exists" errors treated as success (idempotency layer in publish)
- Continue on partial failure (publish remaining packages even if one fails)
- Always exit non-zero on genuine failures

## File Format Conventions

- Changeset files: Hugo-style `+++` TOML frontmatter + markdown body
- Stored as `.chronicle/*.md` with random petname filenames
- Changelogs: Standard `## version` / `### Category` markdown format
- Categories ordered: Breaking Changes, Features, Bug Fixes

## Monorepo Support

- Multiple package managers can coexist (`[npm]` + `[cargo]`)
- Per-package changelogs adjacent to manifest files
- Tag format: `pkg-name@version` for multi-package, `v{version}` for single-package
- Dependency-ordered publishing for Cargo workspaces

## Dry-Run Convention (ADR-008)

- `--dry-run` is strictly local-only: no remote operations, no network calls, no subprocess invocations that contact external services
- Prints what would happen without modifying anything
- Chronicle does NOT delegate dry-run to external tools (e.g., `cargo publish --dry-run`) — it skips the operation entirely and prints a summary
- This is a safety/security invariant: users must be able to trust that `--dry-run` is completely non-destructive
- Trade-off: loses local validation that external tools' dry-run modes provide (e.g., build checks from `cargo publish --dry-run`)

## Command Execution Convention (ADR-011)

- All user-configurable command strings run via `/bin/sh -c "<command>"`
- Working directory is always the git repository root
- All commands are skipped during `--dry-run` (ADR-008 invariant)
- Default error policy: fail-fast (non-zero exit = abort), individual ADRs may override
- No environment variable injection (may be added later as backward-compatible enhancement)
- Single TOML string format only (dual string/array format deferred, not rejected)
- Hardcoded internal commands (cargo, npm, git) are NOT affected — they use explicit arg lists

## Upstream Convention Reuse

- Prefer reading existing ecosystem fields over inventing Chronicle-specific config
- npm `"private": true` and Cargo `publish = false` honored during publish (ADR-007)
- This avoids config duplication and divergence risk
- New adapters should follow the same pattern: check native "do not publish" markers
