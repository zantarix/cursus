---
title: CLI Reference
description: Complete reference for all Cursus commands and flags
---

## Global flags

These flags can be used with any subcommand.

| Flag | Short | Description |
|------|-------|-------------|
| `--interactive` | | Enable interactive TUI (default) |
| `--no-interactive` | | Disable interactive prompts |
| `--verbose` | `-v` | Increase log verbosity. Repeat (`-vv`) for trace output |
| `--silent` | `-s` | Suppress all output except errors |
| `--dry-run` | `-n` | Preview changes without modifying files or running registry commands |

`--verbose` and `--silent` are mutually exclusive.

## `cursus init`

Initialise a new Cursus configuration using the interactive setup wizard.

```bash
cursus init
```

Creates `.cursus/config.toml` with your chosen package managers and settings. This command is interactive-only.

## `cursus change`

Record a change to the project. This is the **default subcommand** — running `cursus` with no subcommand is equivalent to `cursus change`.

```bash
cursus change [OPTIONS]
```

| Flag | Short | Description |
|------|-------|-------------|
| `--change-type <TYPE>` | `-t` | Change type: `major`, `minor`, or `patch` |
| `--message <TEXT>` | `-m` | Description of the change |
| `--project <NAME>` | `-p` | Package(s) to apply to. Repeatable. Defaults to git-changed projects, or all if none detected |
| `--auto` | | Derive changeset from the most recent Conventional Commit |
| `--no-git` | | Skip commit and push (requires `--auto`) |

`--auto` is mutually exclusive with `--change-type` and `--message`.

### Examples

```bash
# Very simple
cursus

# Interactive
cursus change

# Non-interactive, specific package
cursus change --no-interactive -t minor -m "Add retry logic" -p my-lib

# From Conventional Commit
cursus change --no-interactive --auto
```

## `cursus prepare`

Prepare a release: aggregate changesets, bump versions, generate changelogs, and manage branches.

```bash
cursus prepare [OPTIONS]
```

| Flag | Short | Description |
|------|-------|-------------|
| `--package <NAME>` | `-p` | Package(s) to prepare. Repeatable. Defaults to all |
| `--no-git` | | Skip git lifecycle automation |
| `--branch <NAME>` | | Override release branch name (branch strategy only) |

### Examples

```bash
# Prepare all packages
cursus prepare --no-interactive

# Prepare specific packages
cursus prepare --no-interactive -p my-lib -p my-cli

# Preview without changes
cursus prepare --no-interactive --dry-run
```

## `cursus publish`

Publish packages to their registries, create Git tags, and optionally create GitHub Releases.

```bash
cursus publish [OPTIONS]
```

| Flag | Short | Description |
|------|-------|-------------|
| `--package <NAME>` | `-p` | Package(s) to publish. Repeatable. Defaults to all |
| `--no-git` | | Skip Git tags, tag pushing, and GitHub Releases |

### Examples

```bash
# Publish all packages
cursus publish --no-interactive

# Publish without Git operations
cursus publish --no-interactive --no-git
```

## `cursus ci`

Auto-detect repository state and run `prepare` or `publish` as needed. Designed for CI pipelines.

This command is always non-interactive.

```bash
cursus ci [OPTIONS]
```

| Flag | Short | Description |
|------|-------|-------------|
| `--package <NAME>` | `-p` | Package(s) to operate on. Repeatable. Defaults to all |
| `--branch <NAME>` | | Override release branch name |
| `--no-git` | | Skip git/GitHub operations |

**Detection logic:**

1. If changeset files exist in `.cursus/` → runs **prepare**
2. If no changesets but packages have versions without matching Git tags → runs **publish**
3. Otherwise → exits successfully (no-op)

## `cursus verify`

Verify that the current branch adds at least one changeset compared to a base ref. Designed for CI to enforce changeset discipline on pull requests.

```bash
cursus verify [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--base <REF>` | Base Git ref to compare against (default: `origin/HEAD`) |

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | Changeset(s) found |
| 1 | Error |
| 2 | No changesets found |
