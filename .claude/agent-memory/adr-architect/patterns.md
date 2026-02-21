# Architectural Patterns from ADRs

## Three-Step Release Workflow
1. `chronicle release` — filesystem only (version bumps, changelogs, changeset deletion)
2. Git operations — manual by default, opt-in automation via ADR-006
3. `chronicle publish` — registry publishing + optional GitHub Releases

This separation is a core principle. Chronicle defaults to filesystem-only changes.

## Adapter Trait Pattern
`PackageManagerAdapter` trait handles: `enumerate_projects`, `read_version`, `write_version`, `update_lock_file`
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

## Dry-Run Convention
- `--dry-run` supported on release and publish commands
- Prints what would happen without modifying anything
- Passed through to underlying tools where applicable (cargo publish --dry-run)

## Upstream Convention Reuse
- Prefer reading existing ecosystem fields over inventing Chronicle-specific config
- npm `"private": true` and Cargo `publish = false` honored during publish (ADR-007)
- This avoids config duplication and divergence risk
- New adapters should follow the same pattern: check native "do not publish" markers
