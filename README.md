# Chronicle

A release management CLI for software. Chronicle provides a structured workflow
for recording changes, bumping semantic versions, generating changelogs, and
publishing packages to registries.

## Overview

Chronicle breaks the release process into three distinct steps:

1. **Record changes** — developers describe what changed and how it affects the
   version (`major`, `minor`, or `patch`)
2. **Release** — Chronicle aggregates pending changes, bumps versions, generates
   changelogs, and updates lock files
3. **Publish** — packages are published to registries in dependency order

Each step can be run interactively (TUI) or non-interactively for CI/CD
pipelines.

## Installation

### From source

Requires Rust (nightly) and Cargo:

```bash
cargo install --path .
```

### Static binaries

Pre-built static binaries can be produced with [cargo-make](https://github.com/sagiegurari/cargo-make):

```bash
cargo make release             # All targets
cargo make release-x86_64      # x86_64 Linux (musl)
cargo make release-aarch64     # ARM64 Linux (musl)
cargo make release-macos       # ARM64 macOS (via cargo-zigbuild)
```

## Quick start

```bash
# Initialise Chronicle in your repository
chronicle init

# Record a change
chronicle

# Or non-interactively
chronicle change -t minor -m "Add user authentication"

# When ready to release
chronicle release

# Publish to registries
chronicle publish
```

## Commands

### `chronicle init`

Creates a `.chronicle/config.toml` at the repository root. In interactive mode a
TUI wizard guides you through setup; in non-interactive mode pass
`--no-interactive`. You can check which options are available with `--help`.

### `chronicle change`

Records a semantic version change as a changeset file in `.chronicle/`. This is
the default command when no subcommand is given.

| Flag | Description |
|------|-------------|
| `-t, --change-type <type>` | `major`, `minor`, or `patch` |
| `-m, --message <text>` | Change description (opens editor if omitted) |
| `-p, --project <name>` | Target project(s); repeatable, defaults to all |

### `chronicle release`

Consumes pending changesets and applies the highest change type per package to
bump versions, generate changelog entries, and update lock files.

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview changes without writing to disk |
| `-p, --package <name>` | Release specific package(s); repeatable |

### `chronicle publish`

Publishes packages to their respective registries in dependency order.
"Version already exists" errors are treated as success for idempotency.

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview without making any remote calls |
| `-p, --package <name>` | Publish specific package(s); repeatable |

### Global flags

| Flag | Description |
|------|-------------|
| `--interactive` | Enable interactive TUI (default) |
| `--no-interactive` | Disable interactive prompts |

## Package manager support

| Ecosystem | Workspaces | Lock file | Registry |
|-----------|-----------|-----------|----------|
| Cargo | `[workspace].members` | `Cargo.lock` | crates.io |
| npm | `"workspaces"` in `package.json` | `package-lock.json` | npm |
| pnpm | `pnpm-workspace.yaml` | `pnpm-lock.yaml` | npm |
| Yarn | `"workspaces"` in `package.json` | `yarn.lock` | npm |

Chronicle auto-detects the JavaScript package manager from the lock file present
in the repository.

## Changeset format

Changesets are stored as Markdown files with TOML frontmatter in `.chronicle/`:

```markdown
+++
my-package = "minor"
"@my-org/lib" = "patch"
+++

Added a new API endpoint for user profiles.
```

A single changeset can span multiple packages. The highest change type across all
pending changesets determines the version bump for each package.

## Configuration

Chronicle stores its configuration in `.chronicle/config.toml`:

```toml
[npm]
enabled = true
path = "packages"          # optional subdirectory

[cargo]
enabled = true
```

| Section | Key | Description |
|---------|-----|-------------|
| `[global]` | `disable_dependency_cycle_warnings` | Suppress cycle warnings |
| `[npm]` | `enabled` | Enable npm/pnpm/Yarn workspace support |
| `[npm]` | `path` | Subdirectory containing `package.json` |
| `[npm]` | `lock_command` | Custom lock file update command |
| `[cargo]` | `enabled` | Enable Cargo workspace support |
| `[cargo]` | `path` | Subdirectory containing `Cargo.toml` |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

[Mozilla Public License 2.0](LICENSE.md)
