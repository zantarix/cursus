---
title: Publishing
description: How Cursus publishes packages to registries
---

The `publish` step takes a prepared release and pushes it to package registries, creates Git tags, and optionally creates releases on the configured forge (GitHub or GitLab).

## Running publish

```bash
cursus publish
```

Or for specific packages:

```bash
cursus publish -p my-package
```

## What happens during publish

1. **Publish to registries** — each package is published to its configured registry (crates.io for Cargo, npm registry for npm packages)
2. **Create Git tags** — version tags are created for each published package
3. **Push tags** — tags are pushed to the remote
4. **Forge releases** — if a forge integration (GitHub or GitLab) is enabled, a release is created on that forge with changelog notes and any configured build artifacts

## Idempotency

Publish is designed to be safely re-runnable across all three stages:

- **Registry publish** — if a package is already at the target version on the registry, Cursus skips it rather than failing.
- **Git tags** — if the tag for a package already exists, Cursus skips creating it.
- **Forge releases** — if a published release already exists for a tag on the configured forge, Cursus skips creating it.

This means re-running `cursus publish` after a partial failure automatically completes any missing tags or forge releases for packages that were successfully published in a previous run.

**Draft releases block recovery (GitHub only).** If Cursus finds an existing *draft* GitHub Release for a tag, it will not modify it — it reports an actionable error instead. Finalise or delete the draft (e.g. via the GitHub UI or `gh release delete <tag>`) and re-run `cursus publish`. GitLab has no draft-release concept; releases are created in their final state, so this case cannot arise there.

## Skipping Git operations

To publish without creating tags or forge releases:

```bash
cursus publish --no-git
```

## GitHub Releases

When `[github]` is enabled in your configuration, publish will:

1. Run your `build_command` to produce artifacts
2. Create a GitHub Release with the changelog as the body
3. Upload any files listed in `artifacts`

See the [configuration reference](/cursus/reference/configuration/#github) for details on setting up GitHub integration.

## GitLab Releases

When `[gitlab]` is enabled in your configuration, publish will:

1. Run your `build_command` to produce artifacts
2. Create a GitLab Release with the changelog as the body
3. Upload any files listed in `artifacts`

See the [configuration reference](/cursus/reference/configuration/#gitlab) for details on setting up GitLab integration.

## Authentication

- **Cargo** — uses your existing `cargo login` credentials or the `CARGO_REGISTRY_TOKEN` environment variable
- **npm** — uses your existing `npm login` credentials or the `NODE_AUTH_TOKEN` environment variable
- **GitHub** — uses the `GH_TOKEN` or `GITHUB_TOKEN` environment variable (checked in that order)

### crates.io trusted publishing

crates.io supports OIDC-based trusted publishing on GitHub Actions and GitLab CI. Unlike npm (which exchanges the OIDC token internally), `cargo publish` does not perform the exchange itself — you must add a dedicated step to your workflow that exchanges the CI-issued OIDC token for a short-lived `CARGO_REGISTRY_TOKEN`.

A minimal GitHub Actions publish workflow looks like this:

```yaml
jobs:
  publish:
    runs-on: ubuntu-latest
    permissions:
      id-token: write
    steps:
      - uses: actions/checkout@v6
      - uses: rust-lang/crates-io-auth-action@v1
      - run: cursus publish --no-interactive
```

The `rust-lang/crates-io-auth-action` step exchanges the workflow's OIDC identity for a short-lived token and exports it as `CARGO_REGISTRY_TOKEN` — the same environment variable `cargo publish` (and Cursus) already use for classic token auth. No long-lived secret is required.

Cursus will warn before publishing in the following situations:

- **`CARGO_REGISTRY_TOKEN` is not set, no OIDC environment detected** — no Cargo authentication is configured; the publish is likely to fail. Set `CARGO_REGISTRY_TOKEN` or run `cargo login` locally.
- **`CARGO_REGISTRY_TOKEN` is not set, OIDC environment detected** — an OIDC-capable CI environment is present but no token has been exchanged. Add a token exchange step (such as `rust-lang/crates-io-auth-action`) before `cursus publish`.

When `CARGO_REGISTRY_TOKEN` is present — whether it came from a long-lived secret or from an exchange action — Cursus publishes without warning. There is no conflict between OIDC and a token: the exchange action's job is to produce that token.

:::note[Setting up trusted publishing on crates.io]
Before your first trusted-publishing run you must configure a trusted publisher on crates.io for your crate (your GitHub repository, workflow filename, and optionally a deployment environment). See the [crates.io trusted publishing documentation](https://crates.io/docs/trusted-publishing) for step-by-step setup instructions.
:::

:::caution[First publish of a new crate]
crates.io requires a crate to already be published before a trusted publisher can be configured for it. The very first publish of a brand-new crate must use a `CARGO_REGISTRY_TOKEN`.
:::

### npm OIDC trusted publishing

On GitHub Actions (with `id-token: write` permission) and GitLab CI (with OIDC configured), Cursus detects the OIDC environment automatically. npm exchanges the CI identity token for a short-lived publish credential — no `NODE_AUTH_TOKEN` secret is required.

Cursus will warn before publishing in the following situations:

- **`NODE_AUTH_TOKEN` is also set** — the classic token takes precedence over OIDC token exchange. The publish may not use trusted publishing. This is intentional if you are publishing to a registry that does not support OIDC, but is often an accidental leftover secret.
- **Neither OIDC nor `NODE_AUTH_TOKEN` is present** — no recognised npm authentication is configured; the publish is likely to fail.
- **OIDC is active, `access = "public"`, but `publishConfig.provenance` is not `true` in `package.json`** — npm attaches provenance attestations automatically via trusted publishing, but declaring `publishConfig.provenance = true` in your `package.json` makes the intent explicit and ensures provenance is attached even in non-OIDC publish scenarios.

:::note[Setting up trusted publishing on npmjs.com]
Before your first trusted-publishing run you must configure a trusted publisher on npmjs.com for your package (your GitHub repository, workflow filename, and optionally an environment). See the [npm trusted publishing documentation](https://docs.npmjs.com/trusted-publishers/) for step-by-step setup instructions.
:::

:::caution[First publish of a new package]
npm requires a package to exist on the registry before a trusted publisher can be configured for it. The very first publish of a brand-new package must use a `NODE_AUTH_TOKEN`.
:::

## Private packages

By default, packages marked as private by their manifest (`"private": true` in npm, `publish = false` in Cargo) are silently skipped during `cursus publish`. This is the right behavior for internal packages that should never reach a registry.

Some packages, however, are private from a registry perspective but still need release artifacts — GitHub Actions, CLIs distributed as GitHub Release attachments, or any git-tag-distributed software. For these, use `publish_private_packages` in the `[git]` section:

```toml
[git]
enabled = true
publish_private_packages = ["my-github-action", "my-cli"]
```

Listed packages receive the non-registry parts of the publish workflow:

1. **Git tag** — same tag format as registry-published packages
2. **Forge release** — when `[github].enabled = true` or `[gitlab].enabled = true`, a release is created with changelog notes and any configured artifacts

They do not have any registry publish command invoked. Packages that are listed but not actually marked private follow the normal registry publish path.

## Dry run

Preview what publish would do:

```bash
cursus publish --dry-run
```
