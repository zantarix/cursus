---
title: Recording Changes
description: How to record changes with Cursus changesets
---

Cursus uses a changeset-based workflow where each change is recorded as a small file. This avoids merge conflicts in shared changelog files and gives you full control over how changes are described.

## Interactive mode

The simplest way to record a change is with the interactive TUI:

```bash
cursus change
```

The wizard walks you through:

1. **Select packages** — choose which packages are affected by this change
2. **Change type** — pick major, minor, or patch for each selected package
3. **Description** — write a human-readable summary of the change

The resulting changeset file is saved to `.cursus/` with a random filename.

## Non-interactive mode

For scripting and CI, you can pass all options as flags:

```bash
cursus change --no-interactive -t minor -m "Add support for linked versions"
```

To scope to specific packages:

```bash
cursus change --no-interactive -t patch -m "Fix publish retry logic" -p my-package
```

The `-p` flag can be repeated to select multiple packages.

## Deriving from Conventional Commits

If your project uses [Conventional Commits](https://www.conventionalcommits.org/), Cursus can derive changesets automatically:

```bash
cursus change --no-interactive --auto
```

This requires exactly one commit ahead of `origin/HEAD`. It parses that commit message and maps:

- `feat:` to a **minor** change
- `fix:` to a **patch** change
- Any commit with a `BREAKING CHANGE:` footer or `!` suffix to a **major** change

The `--auto` flag is particularly useful in CI to automatically generate changesets for dependency update PRs. See [Automating dependency update changesets](/cursus/guides/ci-integration/#automating-dependency-update-changesets) for an example workflow.

## What gets created

Each `cursus change` invocation creates a single file in `.cursus/` with a random name. See the [Changeset Format](/cursus/reference/changeset-format/) reference for the file structure.

Commit these files alongside your code changes. They accumulate until a release is prepared, at which point they are consumed and deleted.
