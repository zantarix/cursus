---
title: GitHub Actions
description: Running Cursus from GitHub Actions workflows
---

This page covers the GitHub Actions specifics: a starting workflow, the GitHub App setup that produces **Verified** release commits, and a sample auto-changeset workflow for dependency-update PRs. The general `ci` / `verify` semantics live on the [CI Integration overview](/cursus/guides/ci-integration/).

## Example release workflow

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

## Verified commits with a GitHub App

By default, commits made by the release workflow appear as **Unverified** on GitHub because they are produced by the local `git` binary on the runner. To get the green **Verified** badge on your release commits, use a GitHub App installation token instead of `secrets.GITHUB_TOKEN`.

When Cursus detects it is running on GitHub Actions with a token available, it automatically routes the prepare commit through the GitHub Git Data API (`signed_commits = "auto"` is the default). GitHub signs any commit created this way using its web-flow GPG key — but this signing only activates when the request is authenticated as a GitHub App, not a personal access token or the default `GITHUB_TOKEN`. With `signed_commits` enabled, Cursus also creates the release tag through the same API rather than `git push`, so the git remote needs no push access (the tag remains annotated but unsigned).

### Setting up a GitHub App

1. Go to **Settings → Developer settings → GitHub Apps → New GitHub App**.
2. Give it a name (e.g. `my-org-release-bot`) and uncheck **Webhook active**.
3. Under **Repository permissions**, set **Contents** and **Pull Requests** to **Read and write**.
4. Install the app on your repository.
5. Note the **App ID** from the app's settings page.
6. Generate a **private key** (downloaded as a `.pem` file).
7. Find the app's **user ID**: visit `https://api.github.com/users/{app-name}[bot]` and note the `id` field.

Store these in your repository:

| Item | Where |
|------|-------|
| App ID | Repository variable `APP_ID` |
| Private key (`.pem` contents) | Repository secret `APP_PRIVATE_KEY` |
| User ID | Repository variable `APP_USER_ID` |

### Updated release workflow

```yaml
name: Release
on:
  push:
    branches: [main]

jobs:
  release:
    runs-on: ubuntu-latest
    permissions:
      id-token: write   # trusted publishing
      contents: read    # read-only clone/fetch via the default GITHUB_TOKEN
    steps:
      - name: Generate GitHub App token
        id: app-token
        uses: actions/create-github-app-token@v3
        with:
          app-id: ${{ vars.APP_ID }}
          private-key: ${{ secrets.APP_PRIVATE_KEY }}

      # A read-only clone is enough: the release commit and tag are both created
      # through the GitHub API with the App token, so the git remote is never
      # pushed to. `fetch-depth: 0` lets `cursus ci` see existing tags.
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - uses: rust-lang/crates-io-auth-action@v1

      - run: cursus ci --no-interactive
        env:
          GITHUB_TOKEN: ${{ steps.app-token.outputs.token }}
```

Both the release commit and the release tag are created through the GitHub API using the App token, so the workflow needs no local `git commit`/`git tag` and therefore no committer-identity step. The git remote is only cloned (read-only), so the job's `permissions` block grants `contents: read` rather than `contents: write` — the App token, passed as `GITHUB_TOKEN` on the `cursus ci` step, carries the write access for the API operations (verified commit, tag, PR, release). No changes to `.cursus/config.toml` are required — `signed_commits = "auto"` is the default.

This couples the workflow to the API-commit path being active. If you set `signed_commits = "off"`, the local `git commit`/`git tag` run again and need both a committer identity and a token authorised to push code.

:::note
GitHub Apps can be configured to bypass branch protection rules (e.g. requiring PRs). This is useful if your main branch is protected but you want the release commit to land directly. Enable **Allow bypass** for your app under the branch protection settings if needed.
:::

## Auto-changeset workflow

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
