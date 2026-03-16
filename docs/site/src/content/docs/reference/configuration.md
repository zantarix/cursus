---
title: Configuration
description: Complete reference for .cursus/config.toml
---

Cursus is configured via `.cursus/config.toml` in your repository root. Run `cursus init` to generate a starting configuration interactively.

## `[global]`

Top-level settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `disable_dependency_cycle_warnings` | bool | `false` | Suppress warnings about dependency cycles during prepare |

## `[cargo]`

Cargo workspace configuration.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable Cargo package manager support |
| `path` | string | | Subdirectory containing the Cargo workspace, relative to the git root |

```toml
[cargo]
enabled = true
path = "rust/"
```

## `[npm]`

npm/pnpm workspace configuration.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable npm package manager support |
| `path` | string | | Subdirectory containing the npm workspace, relative to the git root |
| `lock_command` | string | | Override the lock file update command. Auto-detected from the lock file present; only needed for unsupported package managers |
| `access` | string | `"restricted"` | npm publish access level: `"public"` or `"restricted"` |

```toml
[npm]
enabled = true
access = "public"
lock_command = "pnpm install --lockfile-only"
```

## `[git]`

Git lifecycle management.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` (derived `true` when `[github].enabled`) | Enable git operations (commit, tag, push) |
| `strategy` | string | `"push"` (`"branch"` when github enabled) | Release strategy: `"push"` or `"branch"` |
| `release_branch_prefix` | string | `"cursus-release/"` | Prefix for release branch names (branch strategy only) |
| `tag_format` | string | `"auto"` | Tag naming: `"auto"`, `"prefixed"`, or `"simple"` |
| `extra_files` | list | `[]` | Additional file paths to stage before committing |

**Tag formats:**

| Format | Single package | Multi-package |
|--------|---------------|---------------|
| `auto` | `v1.2.3` | `my-package@1.2.3` |
| `prefixed` | `my-package@1.2.3` | `my-package@1.2.3` |
| `simple` | `v1.2.3` | `v1.2.3` |

```toml
[git]
strategy = "branch"
tag_format = "prefixed"
extra_files = ["docs/VERSION"]
```

## `[github]`

GitHub integration for releases, pull requests, and asset uploads.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable GitHub integration |
| `owner` | string | auto-detected | GitHub repository owner |
| `repo` | string | auto-detected | GitHub repository name |
| `build_command` | string | `""` | Shell command to build release artifacts |
| `artifacts` | table | `{}` | Map of display names to file paths for GitHub Release uploads |
| `pull_request_title` | string | `"Release updates"` | Title for release pull requests (branch strategy only) |

`owner` and `repo` are auto-detected from your Git remote URL if not specified.

```toml
[github]
enabled = true
build_command = "cargo make release"
pull_request_title = "chore: release updates"

[github.artifacts]
"cursus-linux-x86_64" = "target/x86_64-unknown-linux-musl/release/cursus"
"cursus-macos-aarch64" = "target/aarch64-apple-darwin/release/cursus"
```

## `[prepare]`

Settings that control the prepare step.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `dependency_bump` | string | `"auto"` | How to bump packages that depend on a bumped package |

**Dependency bump values:**

| Value | Behaviour |
|-------|-----------|
| `auto` | Propagates `major` upstream bumps as `major`; all others as `patch` |
| `match` | Bump dependents by the same level as the dependency |
| `patch` | Always bump dependents by patch |
| `minor` | Always bump dependents by minor |
| `major` | Always bump dependents by major |

```toml
[prepare]
dependency_bump = "auto"
```

## `[linked-versions]`

Link package versions so they always stay in sync.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | | Enable linked versions. If `true` with no groups, all packages are linked |
| `groups` | list | `[]` | Groups of packages that share a version |

Each group has:

| Key | Type | Description |
|-----|------|-------------|
| `packages` | list | Glob patterns matching package names |

When any package in a linked group is bumped, all packages in the group receive the same version — the highest bump wins.

```toml
[linked-versions]
enabled = true

[[linked-versions.groups]]
packages = ["my-core-*"]

[[linked-versions.groups]]
packages = ["my-plugin-*"]
```

## Full example

```toml
[global]
disable_dependency_cycle_warnings = false

[cargo]
enabled = true

[npm]
enabled = true
access = "public"
lock_command = "pnpm install --lockfile-only"

[git]
strategy = "branch"
tag_format = "auto"

[github]
enabled = true
build_command = "cargo make release"
pull_request_title = "chore: release updates"

[github.artifacts]
"linux-x86_64" = "target/x86_64-unknown-linux-musl/release/cursus"
"macos-aarch64" = "target/aarch64-apple-darwin/release/cursus"

[prepare]
dependency_bump = "auto"

[linked-versions]
enabled = true

[[linked-versions.groups]]
packages = ["my-*"]
```
