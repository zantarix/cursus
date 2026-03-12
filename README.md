# Chronicle

A release management CLI for software. Chronicle provides a structured workflow
for recording changes, bumping semantic versions, generating changelogs, and
publishing packages to registries.

Designed to truly run anywhere, and distributed as static binaries for most major
platforms. If rust can compile to it, and it's not already available then please
open a request if you need it to run a new platform.

## Overview

Chronicle breaks the release process into three distinct steps:

1. **Record changes** — developers describe what changed and how it affects the
   version (`major`, `minor`, or `patch`)
2. **Prepare** — Chronicle aggregates pending changes, bumps versions, generates
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
cargo make release                 # All targets, can be quite slow
cargo make release-linux-x86_64    # example; see supported targets below
```

Supported targets: `linux-x86_64`, `linux-aarch64`, `linux-riscv64`,
`macos-x86_64`, `macos-aarch64`, `windows-x86_64`, `windows-aarch64`.

## Quick start

```bash
# Initialise Chronicle in your repository
chronicle init

# Record a change
chronicle

# Or non-interactively
chronicle change -t minor -m "Add user authentication"

# When ready to release
chronicle prepare

# Publish to registries
chronicle publish
```

## Commands

### Global flags

These flags apply to all subcommands and can be placed before or after the
subcommand name.

| Flag | Description |
|------|-------------|
| `--interactive` | Enable interactive TUI (default) |
| `--no-interactive` | Disable interactive prompts |
| `-v, --verbose` | Increase log verbosity; repeat (`-vv`) for trace output |
| `-s, --silent` | Suppress all output except errors |
| `-n, --dry-run` | Preview changes without modifying any files or running registry commands |

### `chronicle init`

Creates a `.chronicle/config.toml` at the repository root. In interactive mode a
TUI wizard guides you through setup; in non-interactive mode pass
`--no-interactive`. You can check which options are available with `--help`.

| Flag | Description |
|------|-------------|
| `-p, --package-manager <pm>` | Package manager to configure: `cargo` or `npm` (required in non-interactive mode) |

### `chronicle change`

Records a semantic version change as a changeset file in `.chronicle/`. This is
the default command when no subcommand is given.

| Flag | Description |
|------|-------------|
| `-t, --change-type <type>` | `major`, `minor`, or `patch` (required in non-interactive mode) |
| `-m, --message <text>` | Change description (opens editor if omitted; required in non-interactive mode) |
| `-p, --project <name>` | Target project(s); repeatable, defaults to all |

### `chronicle prepare`

Consumes pending changesets and applies the highest change type per package to
bump versions, generate changelog entries, and update lock files.

| Flag | Description |
|------|-------------|
| `-p, --package <name>` | Prepare specific package(s); repeatable |
| `--no-git` | Skip git lifecycle automation even if enabled in config |
| `--branch <name>` | Override the release branch name (branch strategy only) |

### `chronicle publish`

Publishes packages to their respective registries in dependency order.
"Version already exists" errors are treated as success for idempotency.

| Flag | Description |
|------|-------------|
| `-p, --package <name>` | Publish specific package(s); repeatable |
| `--no-git` | Skip git tag creation, tag pushing, and GitHub Releases even if enabled in config |

### `chronicle ci`

Auto-detects the current repository state and dispatches to `prepare` or
`publish` as needed. Intended for use in CI/CD pipelines. Always runs
non-interactively.

Detection logic:

1. Pending changesets found → run `prepare`
2. No changesets, git enabled, and at least one expected tag is absent → run `publish`
3. Otherwise → do nothing and exit successfully

| Flag | Description |
|------|-------------|
| `-p, --package <name>` | Limit to specific package(s); repeatable |
| `--branch <name>` | Override the release branch name passed to `prepare` (branch strategy only) |
| `--no-git` | Skip git/GitHub operations; also disables tag-based publish detection |

## Package manager support

| Ecosystem | Workspaces | Lock file | Registry |
|-----------|-----------|-----------|----------|
| Cargo | `[workspace].members` | `Cargo.lock` | crates.io |
| npm | `"workspaces"` in `package.json` | `package-lock.json` | npm |
| pnpm | `pnpm-workspace.yaml` | `pnpm-lock.yaml` | npm |

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
[global]
# Suppress warnings about circular dependencies in monorepos.
disable_dependency_cycle_warnings = false

[npm]
enabled = true
# Subdirectory containing package.json, relative to the git root.
path = "packages"
# Custom command to update the lock file after version bumps.
# Auto-detected from the lock file present if omitted.
lock_command = "pnpm install"
# Access level for scoped packages: "public" or "restricted" (default).
access = "public"

[cargo]
enabled = true
# Subdirectory containing Cargo.toml, relative to the git root.
path = "rust"

[git]
# Commit, push, and tag as part of prepare/publish.
# Defaults to true when [github] is enabled.
enabled = true
# "push": commit directly to the current branch.
# "branch": create a release branch and push it (default when [github] enabled).
strategy = "branch"
# Prefix for release branch names in the "branch" strategy.
release_branch_prefix = "chronicle-release/"
# Tag format: "auto" (v{ver} for single-package, {pkg}@{ver} for monorepos),
# "simple" (always v{ver}), or "prefixed" (always {pkg}@{ver}).
tag_format = "auto"
# Extra files to stage before committing, relative to the git root.
# Useful when a custom lock_command writes files Chronicle doesn't know about.
extra_files = ["custom.lock"]

[github]
# Create a GitHub Release for each package after publish.
# Requires GITHUB_TOKEN or GH_TOKEN to be set.
enabled = true
# Owner and repo are auto-detected from the git remote if omitted.
owner = "my-org"
repo = "my-app"
# Shell command to build release artifacts before uploading (optional).
build_command = "make release"
# Pull request title used in the "branch" git strategy.
pull_request_title = "Release updates"

[github.artifacts]
# Map of asset display names to file paths relative to the git root.
"linux-amd64.tar.gz" = "dist/app-linux-amd64.tar.gz"
"macos-arm64.tar.gz" = "dist/app-macos-arm64.tar.gz"
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

[Mozilla Public License 2.0](LICENSE.md)
