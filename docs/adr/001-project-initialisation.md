# ADR-001: Project Initialisation

## Status

Accepted

## Context

Before Chronicle can record changesets or perform releases, it needs to know which package managers and packages exist in a repository. This configuration must be explicitly created so that Chronicle can discover packages consistently across all commands.

The tool must support monorepos with multiple packages managed by different package managers (Cargo and npm/yarn/pnpm), where package manifests may live in subdirectories. It must work in both interactive (local development) and non-interactive (scripting/CI) contexts.

## Decision

### The `chronicle init` command

A repository is initialised with `chronicle init`, which creates a `.chronicle/config.toml` file at the git repository root. The command fails if a configuration already exists, preventing accidental re-initialisation.

**Interactive mode** (default): A TUI wizard detects existing `package.json` or `Cargo.toml` files and guides the user through selecting a package manager.

**Non-interactive mode** (`--no-interactive`): Requires `-p <npm|cargo>` to specify the package manager directly.

### Configuration format

```toml
[npm]
enabled = true
path = "pkg/"      # optional: look for package.json here instead of git root

[cargo]
enabled = true
```

Each package manager section has:
- `enabled` (bool, default `false`) — whether this package manager is active
- `path` (optional string) — subdirectory for the package manager root, relative to the git root; omitted when not needed

Unknown fields are rejected (`deny_unknown_fields`), ensuring configuration errors are caught early.

### Package enumeration

Chronicle discovers packages by delegating to package manager adapters that implement the `PackageManagerAdapter` trait. Each adapter's `enumerate_projects()` method reads manifest files and returns a list of `ProjectInfo` structs.

**Cargo adapter:**
- Single crate: returns one project from `[package].name`
- Workspace with `[workspace].members`: expands glob patterns, reads each member's `Cargo.toml`, returns all non-virtual crates
- Root crate in a workspace is included if it has a `[package]` section

**npm adapter:**
- Single package: returns one project from `"name"` in `package.json`
- Monorepo with `"workspaces"`: expands glob patterns, reads each workspace's `package.json`
- pnpm support: also reads `pnpm-workspace.yaml`; pnpm workspace list takes precedence when present
- Root package is always included

Both adapters respect the optional `path` configuration to resolve manifest files relative to a subdirectory of the git root. Results are sorted by path for deterministic ordering.

### Storage

The `.chronicle/` directory at the git root serves as the home for both configuration and changeset files (see ADR-002). The directory is created automatically by `chronicle init`.

## Consequences

- Configuration is explicit and committed to source control, ensuring all contributors and CI use the same package manager settings.
- Multiple package managers can be enabled simultaneously, supporting polyglot monorepos.
- Package enumeration is decoupled from configuration via the adapter trait, making it straightforward to add new package managers in the future.
- The `deny_unknown_fields` constraint prevents silent misconfiguration from typos or stale fields.
- The optional `path` field allows Chronicle to work in repositories where package manager roots don't coincide with the git root (e.g., a Rust backend in `backend/` alongside a frontend).

## Errata

The original consequences section stated that `deny_unknown_fields` makes adding new configuration options a breaking change. This is incorrect. Adding new fields is non-breaking: existing config files simply won't contain the new field, and `serde(default)` provides sensible defaults. The constraint only prevents *users* from having fields in their config that Chronicle doesn't recognise, catching typos and stale configuration. *Removing* a previously supported field would be breaking, since existing configs referencing it would fail to parse.
