---
title: Changeset Format
description: Specification for Cursus changeset files
---

Changeset files live in the `.cursus/` directory and use a Hugo-style TOML frontmatter format.

## Structure

```
+++
package-name = "minor"
another-package = "patch"
+++

A human-readable description of the change.
```

The frontmatter (between `+++` delimiters) is TOML containing a mapping of package names to change types. The body after the closing `+++` is a free-text description that will appear in the generated changelog.

## Change types

| Type | Semantic version bump | When to use |
|------|----------------------|-------------|
| `major` | `x.0.0` | Breaking changes to the public API |
| `minor` | `0.x.0` | New features that are backwards-compatible |
| `patch` | `0.0.x` | Bug fixes and minor improvements |

## File naming

Changeset files are given random names to avoid merge conflicts. You don't need to worry about the filenames — Cursus manages them.

## Lifecycle

1. **Created** by `cursus change` (or manually)
2. **Committed** alongside your code changes
3. **Consumed** by `cursus prepare`, which reads the changesets, applies version bumps, generates changelog entries, and deletes the files

## Manual creation

While `cursus change` is the recommended way to create changesets, you can also create them manually. Place a file in `.cursus/` following the format above. Any filename works as long as it doesn't conflict with `config.toml`.

## Limits

Changeset files must not exceed **64 KiB**. This limit exists to prevent excessive memory use when processing a large number of changesets. In practice, changeset descriptions are a few sentences, so this limit is unlikely to be reached.
