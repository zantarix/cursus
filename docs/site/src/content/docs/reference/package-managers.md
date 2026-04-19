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

**Registry:** crates.io (authenticated via `cargo login`, `CARGO_REGISTRY_TOKEN`, or crates.io OIDC trusted publishing)

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

### Authentication

Cursus delegates Cargo authentication entirely to the environment:

- **`CARGO_REGISTRY_TOKEN`** — set this environment variable to a crates.io API token for local publishes or CI environments that do not use trusted publishing.
- **crates.io trusted publishing** — on GitHub Actions and GitLab CI, an exchange action (e.g. `rust-lang/crates-io-auth-action`) obtains a short-lived token and exports it as `CARGO_REGISTRY_TOKEN` before `cargo publish` runs. No long-lived secret is needed. Cursus detects the OIDC environment and emits warnings if no token is present. See the [publishing guide](/cursus/guides/publishing/#crates-io-trusted-publishing) for details.
- **`cargo login`** — interactive login credentials stored in `~/.cargo/credentials.toml` also work for local use.

## npm

Cursus supports npm, pnpm, and Yarn workspaces. The correct lock file command is auto-detected from the lock file present in your repository.

**What it does:**

- Enumerates packages from `package.json` workspace definitions
- Writes version updates to each package's `package.json`
- Updates the lock file automatically
- Publishes to the npm registry via `npm publish`

**Registry:** npm (authenticated via `npm login`, `NODE_AUTH_TOKEN`, or OIDC trusted publishing on GitHub Actions / GitLab CI)

```toml
[npm]
enabled = true
access = "public"
```

### Authentication

Cursus delegates npm authentication entirely to the environment:

- **`NODE_AUTH_TOKEN`** — set this environment variable to a classic npm access token for local publishes or CI environments that do not support OIDC.
- **OIDC trusted publishing** — on GitHub Actions (with `id-token: write` permission) and GitLab CI (with OIDC configured), npm exchanges the CI identity token for a short-lived publish credential automatically. No long-lived secret is needed. Cursus detects the OIDC environment and emits warnings for common misconfigurations (token interference, missing authentication, missing `publishConfig.provenance`). See the [publishing guide](/cursus/guides/publishing/#npm-oidc-trusted-publishing) for details.
- **`npm login`** — interactive login credentials stored in `.npmrc` also work for local use.

Note: if you are using yarn or pnpm, publishing still goes through `npm publish` and the same authentication mechanisms apply.

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
