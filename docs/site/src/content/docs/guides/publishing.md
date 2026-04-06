---
title: Publishing
description: How Cursus publishes packages to registries
---

The `publish` step takes a prepared release and pushes it to package registries, creates Git tags, and optionally creates GitHub Releases.

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
4. **GitHub Releases** — if GitHub integration is enabled, a release is created with changelog notes and any configured build artifacts

## Idempotency

Publish is designed to be safely re-runnable. If a package is already published at the target version, Cursus skips it rather than failing. This is important for CI where a job might be retried after a partial failure.

## Skipping Git operations

To publish without creating tags or GitHub Releases:

```bash
cursus publish --no-git
```

## GitHub Releases

When `[github]` is enabled in your configuration, publish will:

1. Run your `build_command` to produce artifacts
2. Create a GitHub Release with the changelog as the body
3. Upload any files listed in `artifacts`

See the [configuration reference](/cursus/reference/configuration/#github) for details on setting up GitHub integration.

## Authentication

- **Cargo** — uses your existing `cargo login` credentials or the `CARGO_REGISTRY_TOKEN` environment variable
- **npm** — uses your existing `npm login` credentials or the `NODE_AUTH_TOKEN` environment variable
- **GitHub** — uses the `GH_TOKEN` or `GITHUB_TOKEN` environment variable (checked in that order)

### npm OIDC trusted publishing

On GitHub Actions (with `id-token: write` permission) and GitLab CI (with OIDC configured), Cursus detects the OIDC environment automatically. npm exchanges the CI identity token for a short-lived publish credential — no `NODE_AUTH_TOKEN` secret is required.

Cursus will warn before publishing in the following situations:

- **`NODE_AUTH_TOKEN` is also set** — the classic token takes precedence over OIDC token exchange. The publish may not use trusted publishing. This is intentional if you are publishing to a registry that does not support OIDC, but is often an accidental leftover secret.
- **Neither OIDC nor `NODE_AUTH_TOKEN` is present** — no recognised npm authentication is configured; the publish is likely to fail.
- **OIDC is active, `access = "public"`, but `publishConfig.provenance` is not `true` in `package.json`** — npm attaches provenance attestations automatically via trusted publishing, but declaring `publishConfig.provenance = true` in your `package.json` makes the intent explicit and ensures provenance is attached even in non-OIDC publish scenarios.

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
2. **GitHub Release** — when `[github].enabled = true`, a release is created with changelog notes and any configured artifacts

They do not have any registry publish command invoked. Packages that are listed but not actually marked private follow the normal registry publish path.

## Dry run

Preview what publish would do:

```bash
cursus publish --dry-run
```
