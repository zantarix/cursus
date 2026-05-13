---
title: Configuration
description: Complete reference for .cursus/config.toml
---

Cursus is configured via `.cursus/config.toml` in your repository root. Run `cursus init` to generate a starting configuration interactively.

## `[global]`

Top-level settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `disable_dependency_cycle_warnings` | bool | `false` | Suppress warnings about circular dependencies between two or more packages during publish. Self-loops (a package listing itself as a dependency) are never warned about. |
| `ignore` | list of strings | `[]` | Glob patterns matching package names to exclude from all Cursus operations. Matching packages are dropped before version bumps, publish, and changelog generation. Cursus warns if a pattern matches nothing. |

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
| `prepare_commit_message` | string | `"ci(release): version packages"` | Commit message used for the prepare step |
| `publish_private_packages` | list | `[]` | Private package names that receive git tags and GitHub Releases without registry publish |
| `signed_commits` | string | `"auto"` | Whether to create the prepare commit via the GitHub Git Data API for a Verified badge. `"auto"`: enabled when `GITHUB_ACTIONS=true` and a token is present. `"force"`: enabled whenever a token is present (experimental). `"off"`: always use the local `git` binary. |

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
publish_private_packages = ["my-github-action"]
```

## `[github]`

GitHub integration for releases, pull requests, and asset uploads.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable GitHub integration |
| `owner` | string | auto-detected | GitHub repository owner |
| `repo` | string | auto-detected | GitHub repository name |
| `build_command` | string | `""` | Shell command to build release artifacts |
| `artifacts` | table of tables | `{}` | Per-package artifact maps: `[github.artifacts.<package-name>]` sections mapping display names to file paths |
| `pull_request_title` | string | `"Release updates"` | Title for release pull requests (branch strategy only) |

`owner` and `repo` are auto-detected from your Git remote URL if not specified.

```toml
[github]
enabled = true
build_command = "cargo make release"
pull_request_title = "chore: release updates"

[github.artifacts.cursus]
"cursus-linux-x86_64" = "target/x86_64-unknown-linux-musl/release/cursus"
"cursus-macos-aarch64" = "target/aarch64-apple-darwin/release/cursus"
```

## `[gitlab]`

GitLab integration for releases, merge requests, and asset uploads.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable GitLab integration |
| `group` | string | auto-detected | GitLab namespace (user or group). Subgroup paths like `acme/sub` are supported |
| `project` | string | auto-detected | GitLab project name (the final path segment) |
| `host` | string | `""` (→ `gitlab.com`) | Base URL of the GitLab instance for self-managed deployments. Non-standard ports are supported (e.g. `https://gitlab.example.com:8443`). Overridden by `CI_API_V4_URL` when running under GitLab CI |
| `build_command` | string | `""` | Shell command to build release artifacts |
| `artifacts` | table of tables | `{}` | Per-package artifact maps: `[gitlab.artifacts.<package-name>]` sections mapping display names to file paths |
| `merge_request_title` | string | `"Release updates"` | Title for release merge requests (branch strategy only) |

`group` and `project` are auto-detected from your Git remote URL if not specified. Both fields must be set together (or both omitted for auto-detection).

```toml
[gitlab]
enabled = true
group = "acme"
project = "my-app"
host = "https://gitlab.example.com"
build_command = "cargo make release"
merge_request_title = "chore: release updates"

[gitlab.artifacts.cursus]
"cursus-linux-x86_64" = "target/x86_64-unknown-linux-musl/release/cursus"
"cursus-macos-aarch64" = "target/aarch64-apple-darwin/release/cursus"
```

Release artifacts on GitLab are uploaded to the project's [Generic Package Registry](https://docs.gitlab.com/user/packages/generic_packages/) and attached as release asset links. If your instance or project has the Generic Package Registry disabled, asset uploads will fail.

See the [GitLab integration guide](/cursus/guides/ci-integration/gitlab/) for a complete walkthrough.

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
ignore = ["internal-*"]

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

[github.artifacts.cursus]
"linux-x86_64" = "target/x86_64-unknown-linux-musl/release/cursus"
"macos-aarch64" = "target/aarch64-apple-darwin/release/cursus"

[prepare]
dependency_bump = "auto"

[linked-versions]
enabled = true

[[linked-versions.groups]]
packages = ["my-*"]
```

## Environment variables

Cursus reads the following environment variables in addition to the `config.toml` settings:

| Variable | Description |
|----------|-------------|
| `CURSUS_LOCALE` | Override the locale used for user-visible messages. Accepts a BCP 47 language tag (e.g. `en`, `fr`, `zh-TW`). If unset, the system locale is detected automatically with `en` as the fallback. |
| `CARGO_REGISTRY_TOKEN` | Token for publishing to crates.io (Cargo adapter). Equivalent to `cargo login`. |
| `NODE_AUTH_TOKEN` | Token for publishing to the npm registry (npm adapter). Equivalent to `npm login` for token-based auth. |
| `GH_TOKEN` / `GITHUB_TOKEN` | Token for GitHub API operations (releases, PRs, asset uploads). Checked in this order. |
| `GITLAB_TOKEN` | Project- or group-access token for GitLab API operations. Required for `prepare` (merge-request creation); falls back to `CI_JOB_TOKEN` when only release/publish operations are needed. |
| `CI_JOB_TOKEN` | GitLab CI predefined job token. Sufficient for `publish` (releases, assets) but **cannot** create merge requests — `prepare` requires `GITLAB_TOKEN`. |
| `CI_API_V4_URL` | GitLab CI predefined variable; when present, overrides `[gitlab].host` so the client targets the same instance the CI job runs on. |

## Limits

`config.toml` must not exceed **256 KiB**. This limit exists to prevent excessive memory use when loading configuration. In practice, even large monorepo configurations are well under this limit.
