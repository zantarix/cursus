---
title: Preparing Releases
description: How Cursus aggregates changesets and bumps versions
---

The `prepare` step is where changesets become a release. Cursus reads all pending changeset files, determines the next version for each package, and updates everything in one operation.

## Running prepare

```bash
cursus prepare
```

Or for specific packages only:

```bash
cursus prepare -p my-package -p my-other-package
```

## What happens during prepare

1. **Aggregate changesets** — all changeset files in `.cursus/` are read and grouped by package
2. **Determine versions** — for each package, the highest change type (major > minor > patch) determines the version bump
3. **Propagate dependencies** — packages that depend on a bumped package will also be bumped (see [configuration](/cursus/reference/configuration/#prepare))
4. **Update version files** — `Cargo.toml`, `package.json`, lock files, and any workspace references are updated
5. **Generate changelogs** — `CHANGELOG.md` entries are created from changeset descriptions
6. **Clean up** — consumed changeset files are deleted
7. **Git operations** — depending on your [git strategy](/cursus/reference/configuration/#git), changes are committed and optionally pushed or placed on a release branch

## Linked versions

For monorepos where packages should share a version number, configure [linked versions](/cursus/reference/configuration/#linked-versions). When any package in a linked group is bumped, all packages in the group receive the same version.

## Dependency propagation

When a package is bumped, its dependents may need a bump too. The `dependency_bump` setting controls this behaviour:

| Value | Behaviour |
|-------|-----------|
| `auto` (default) | Propagates `major` upstream bumps as `major`; all others as `patch` |
| `patch` / `minor` / `major` | Always bump dependents by this fixed level |
| `match` | Bump dependents by the same level as the dependency |

## Git strategies

The `[git]` configuration controls how prepare interacts with Git:

- **`push`** (default) — commits and pushes directly to the current branch
- **`branch`** — creates a release branch (e.g., `cursus-release/my-package`) with a pull request if GitHub integration is enabled

## Dry run

Preview what prepare would do without making any changes:

```bash
cursus prepare --dry-run
```
