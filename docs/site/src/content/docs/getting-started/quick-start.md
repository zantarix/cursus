---
title: Quick Start
description: Get up and running with Cursus in minutes
---

This guide walks you through the core Cursus workflow: initialise, record changes, prepare a release, and publish.

## 1. Initialise

Run the setup wizard in the root of your Git repository:

```bash
cursus init
```

This creates a `.cursus/config.toml` file and the `.cursus/` directory where changesets are stored. The wizard asks which package managers you use, whether to enable git automation, and which forge (GitHub, GitLab, or neither) to use for releases. See the [CLI reference](/cursus/reference/cli/#cursus-init) for the full screen-by-screen walkthrough.

## 2. Record a change

After making changes to your code, record what changed:

```bash
cursus change
```

With no subcommand, `change` is the default. The interactive TUI will ask you to select the affected packages, choose a change type (major, minor, or patch), and write a description.

For non-interactive use (e.g., in scripts or CI):

```bash
cursus change --no-interactive -t minor -m "Add support for linked versions"
```

This creates a changeset file in `.cursus/` — commit it alongside your code changes.

## 3. Prepare a release

When you're ready to release, aggregate all pending changesets into a version bump:

```bash
cursus prepare
```

This will:

- Read all pending changeset files
- Determine the next version for each package based on the highest change type
- Update version numbers in your package files (`Cargo.toml`, `package.json`, etc.)
- Generate or update `CHANGELOG.md` entries
- Remove the consumed changeset files

## 4. Publish

Once a release is prepared, publish to your registries:

```bash
cursus publish
```

This publishes each package to its registry (crates.io, npm), creates Git tags, and optionally creates releases on the configured forge (GitHub or GitLab) with build artifacts.

## Automate with CI

For CI pipelines, the `ci` subcommand handles everything automatically:

```bash
cursus ci --no-interactive
```

It detects the repo state and runs either `prepare` or `publish` as needed. See the [CI Integration](/cursus/guides/ci-integration/) guide for details.
