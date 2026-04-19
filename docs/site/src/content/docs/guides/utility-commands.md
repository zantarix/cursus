---
title: Utility Commands
description: Additional Cursus commands for verification and initialisation
---

Beyond the core release workflow, Cursus provides a few utility commands for setup and quality enforcement.

## `cursus init`

Initialises a new Cursus configuration in your repository:

```bash
cursus init
```

This runs an interactive TUI wizard that asks which package managers you use and writes a `.cursus/config.toml` with sensible defaults. It also creates the `.cursus/` directory if it doesn't already exist.

`init` is interactive-only. Projects that need to generate config programmatically can write `.cursus/config.toml` directly — see the [configuration reference](/cursus/reference/configuration/) for the full schema.

## `cursus verify`

Checks that the current branch has at least one new changeset relative to a base ref:

```bash
cursus verify --no-interactive
```

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | At least one changeset found |
| 1 | Error |
| 2 | No changesets found |

By default the base ref is `origin/HEAD`. Override it with `--base`:

```bash
cursus verify --no-interactive --base origin/main
```

### Enforcing changesets on pull requests

`verify` is designed to be used as a CI gate so that every PR that touches releasable code includes a changeset. Add it as a required status check:

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

The `fetch-depth: 0` is important — a shallow clone won't have the history needed to compare against the base ref.

Note that PRs which use `--auto` to derive their changeset automatically (e.g. dependency update PRs) will satisfy this check without any manual intervention. See [Automating dependency update changesets](/cursus/guides/ci-integration/#automating-dependency-update-changesets).
