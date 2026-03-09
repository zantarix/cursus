# ADR Inventory

- **ADR-001** (Accepted): Project Initialisation
- **ADR-002** (Accepted): Changeset Recording
- **ADR-003** (Accepted): Release Command. Errata: ADR-006 adds optional git hooks.
- **ADR-004** (Accepted): Publish Command
- **ADR-005** (Accepted): GitHub Releases. Errata: ADR-011 is authoritative reference for `build_command` execution semantics.
- **ADR-006** (Accepted): Git Lifecycle Hooks. `run_until: GitStep` enum, `TagFormat`, `extra_files`, `--no-git` CLI flag. Push is `#[coverage(off)]` / `#[mutants::skip]`.
- **ADR-007** (Accepted): Honor Private Package Markers During Publish
- **ADR-008** (Accepted): Dry-Run Must Be Strictly Local-Only. Errata added to ADR-004.
- **ADR-009** (Accepted): JavaScript Package Manager Strategy for Lockfiles and Publishing. Errata: ADR-011 supersedes `lock_command` whitespace-splitting execution; now uses `/bin/sh -c`.
- **ADR-010** (Accepted): Scoped Release Changeset Consumption — fix silent data loss when using `--package` flag
- **ADR-011** (Accepted): Command Execution Strategy — standardize shell execution via `/bin/sh -c` for all user-configurable commands, migrating `lock_command` from whitespace splitting. Covers dry-run, error handling, and working directory conventions.
- **ADR-012** (Accepted): Skip workspace: Protocol Dependencies During Intra-Workspace Version Propagation — skip and warn on `workspace:` protocol entries in npm dependency version propagation during release.
