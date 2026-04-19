---
title: CI Integration
description: Using Cursus in continuous integration pipelines
---

Cursus is designed to work seamlessly in CI. The `ci` subcommand auto-detects your repository state and runs the appropriate action, and the `verify` subcommand ensures PRs include changesets.

## The `ci` subcommand

```bash
cursus ci --no-interactive
```

This inspects the repository and decides what to do:

| State | Action |
|-------|--------|
| Pending changeset files exist | Runs **prepare** |
| No changesets, but packages have versions without matching Git tags | Runs **publish** |
| Neither condition | No-op (exits successfully) |

This makes your CI pipeline simple — just run `cursus ci` on every push to your main branch and it does the right thing.

## Verifying changesets on PRs

Use the `verify` subcommand to enforce that every PR includes at least one changeset:

```bash
cursus verify --no-interactive
```

Exit codes:

- **0** — changeset(s) found
- **1** — error
- **2** — no changesets found

By default, `verify` compares against `origin/HEAD`. To use a different base:

```bash
cursus verify --no-interactive --base origin/main
```

## Example GitHub Actions workflow

```yaml
name: Release
on:
  push:
    branches: [main]

jobs:
  release:
    runs-on: ubuntu-latest
    permissions:
      contents: write   # create tags and GitHub Releases
      id-token: write   # trusted publishing
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      # Exchange the GitHub Actions OIDC token for a short-lived CARGO_REGISTRY_TOKEN.
      # Only consumed when cursus ci dispatches to publish; harmless on prepare runs.
      - uses: rust-lang/crates-io-auth-action@v1
      - run: cursus ci --no-interactive
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

For projects not using trusted publishing, you can omit the `id-token: write` permission and the exchange step, and set login tokens such as `CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}` in the `env` block instead. See the [publishing guide](/cursus/guides/publishing/#authentication) for details on trusted publishing setup.

For PR changeset verification:

```yaml
name: CI
on:
  pull_request:

jobs:
  verify-changeset:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      - run: cursus verify --no-interactive
```

## Automating dependency update changesets

For PRs created by tools like Renovate or Dependabot, you can automatically derive a changeset from the Conventional Commit message. This works best with [git integration enabled](/cursus/reference/configuration/#git), which lets Cursus commit and push the changeset back to the PR branch without any extra steps:

```yaml
name: Auto Changeset
on:
  pull_request:
    types: [opened, synchronize]

jobs:
  changeset:
    if: contains(github.event.pull_request.labels.*.name, 'dependencies')
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - run: cursus change --no-interactive --auto
```
