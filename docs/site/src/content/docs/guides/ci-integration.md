---
title: CI Integration
description: Using Cursus in continuous integration pipelines
---

Cursus is designed to work seamlessly in CI. The `ci` subcommand auto-detects your repository state and runs the appropriate action, and the `verify` subcommand ensures PRs include changesets. The forge-specific bits — runner image, token names, and how Verified commits are produced — live on the per-forge pages below.

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

## Automating dependency update changesets

For PRs created by tools like Renovate or Dependabot, you can automatically derive a changeset from the Conventional Commit message. This works best with [git integration enabled](/cursus/reference/configuration/#git), which lets Cursus commit and push the changeset back to the PR branch without any extra steps. The relevant command is:

```bash
cursus change --no-interactive --auto
```

See the per-forge guide for an end-to-end workflow example.

## Forge-specific guides

The release and verify workflows themselves are forge-agnostic — the differences are in the runner config, token names, and how each forge handles things like Verified commits. Pick the guide for the forge you target:

- [**GitHub Actions**](/cursus/guides/ci-integration/github-actions/) — sample release workflow, GitHub App setup for Verified release commits, dependency-update changesets via Renovate/Dependabot.
- [**GitLab**](/cursus/guides/ci-integration/gitlab/) — `[gitlab]` config schema, `GITLAB_TOKEN` vs `CI_JOB_TOKEN`, self-managed instance setup, Generic Package Registry asset uploads.
