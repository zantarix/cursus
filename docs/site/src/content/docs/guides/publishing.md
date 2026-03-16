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
- **npm** — uses your existing `npm login` credentials or the `NPM_TOKEN` environment variable
- **GitHub** — uses the `GH_TOKEN` or `GITHUB_TOKEN` environment variable (checked in that order)

## Dry run

Preview what publish would do:

```bash
cursus publish --dry-run
```
