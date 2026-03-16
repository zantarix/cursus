---
title: Package Managers
description: Supported package managers and their configuration
---

Cursus uses an adapter pattern to support multiple package managers. Each adapter knows how to enumerate projects in a workspace, read and write versions, update lock files, and publish to a registry.

Any combination of package managers can be enabled simultaneously. Cursus enumerates packages from all enabled adapters and manages them together — changesets can reference packages from any registry, and versions are bumped and published in dependency order across ecosystems.

```toml
[cargo]
enabled = true

[npm]
enabled = true
access = "public"
```

## Cargo

Cursus supports Cargo workspaces and standalone Cargo packages.

**What it does:**

- Enumerates packages from `Cargo.toml` workspace members (or a single package)
- Writes version updates to each package's `Cargo.toml`, including workspace dependency references
- Updates `Cargo.lock` via `cargo generate-lockfile`
- Publishes to crates.io via `cargo publish`

**Registry:** crates.io (authenticated via `cargo login` or `CARGO_REGISTRY_TOKEN`)

```toml
[cargo]
enabled = true
```

If your Cargo workspace is in a subdirectory:

```toml
[cargo]
enabled = true
path = "rust/"
```

## npm

Cursus supports npm, pnpm, and Yarn workspaces. The correct lock file command is auto-detected from the lock file present in your repository.

**What it does:**

- Enumerates packages from `package.json` workspace definitions
- Writes version updates to each package's `package.json`
- Updates the lock file automatically
- Publishes to the npm registry via `npm publish`

**Registry:** npm (authenticated via `npm login` or `NPM_TOKEN`)

```toml
[npm]
enabled = true
access = "public"
```

### Access levels

The `access` field controls the npm publish access level:

- `"public"` — published packages are publicly visible
- `"restricted"` (default) — packages are scoped/private

### Unsupported package managers

If you need to use a package manager that Cursus doesn't officially support yet, the `lock_command` option lets you provide a custom command to update the lock file after version bumps:

```toml
[npm]
enabled = true
lock_command = "bun install --frozen-lockfile"
```

This is an escape hatch — officially supported package managers (npm, pnpm, Yarn) don't need it. If your package manager isn't supported, please [open a request](https://github.com/zantarix/cursus/issues).
