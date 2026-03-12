# ADR-010: Scoped Release Changeset Consumption

## Status

Accepted

## Context

Chronicle's `release` command supports a `--package` flag that scopes a release to specific packages. This was added to support monorepo workflows where packages are released independently -- for example, releasing `pkg-a` without releasing `pkg-b`.

Changesets are small Markdown files in `.chronicle/` with TOML frontmatter mapping package names to change types. A single changeset can reference multiple packages:

```
+++
pkg-a = "patch"
pkg-b = "minor"
+++

Shared infrastructure change affecting both packages
```

The current implementation has a correctness bug: when `release --package pkg-a` runs, version bumping and changelog generation are correctly scoped to `pkg-a`, but **all** changeset files are unconditionally deleted afterwards -- including changesets that contain entries for packages not part of this release. This silently discards pending changes for unreleased packages.

[ADR-003](003-release-command.md) defined changeset consumption as "delete all processed `.chronicle/*.md` files" but did not anticipate scoped releases. The `--package` flag was added to the implementation without updating the consumption semantics.

## Decision

We will rewrite changesets to remove consumed package entries during scoped releases.

When `release` runs with a `--package` scope, changeset consumption will follow these rules:

1. **Fully consumed changesets are deleted.** If every package referenced in a changeset was included in the release scope, the file is removed from disk. This matches the existing behaviour for unscoped releases.

2. **Partially consumed changesets are rewritten.** If a changeset references packages both inside and outside the release scope, the released package entries are stripped from the TOML frontmatter. The description message is preserved unchanged. The file is rewritten in place.

3. **Unscoped releases are unaffected.** When `release` runs without `--package`, all changesets are fully consumed and deleted, exactly as before. The new logic only activates when a package filter is present.

4. **Rewritten changesets remain valid.** A rewritten changeset must be parseable by the same `parse_changeset()` function. The only change is the removal of key-value pairs from the frontmatter.

5. **The description message is always preserved.** Even after stripping some package entries, the original description is kept. It may reference packages that are no longer in the frontmatter, but this is acceptable -- the message is a human-readable note, not a structured reference. The changelog for the released package already captured the relevant entry.

## Consequences

### Positive

- Scoped releases no longer silently discard changes for unreleased packages. Each package entry in a changeset is consumed exactly once.
- The changeset format is unchanged. No new file formats, manifest files, or tracking mechanisms are introduced.
- Unscoped releases are completely unaffected, preserving backward compatibility.
- The implementation is contained to the release command's consumption step and a small utility function in the changeset module.

### Negative

- Changeset files are now mutable artifacts. A file that originally described changes to `pkg-a` and `pkg-b` may be rewritten to only reference `pkg-b`. The on-disk file no longer matches what the developer originally wrote. Users who inspect `.chronicle/` between releases may find this surprising.
- The description message may reference packages no longer listed in the frontmatter, which could be mildly confusing on inspection. However, changesets are transient artifacts meant to be consumed, and the authoritative record of changes lives in the changelog and git history.

### Neutral

- Dry-run mode remains unaffected. Rewriting only happens during actual (non-dry-run) releases.
- This decision does not affect how changesets are created, only how they are consumed.
- Git lifecycle hooks ([ADR-006](006-git-lifecycle-hooks.md)), if enabled, will commit the rewritten changesets as part of the release commit, so the intermediate state is captured in version control.

## Alternatives Considered

### Delete only fully consumed changesets

Leave a changeset on disk entirely if it references any package outside the release scope. This is the simplest approach but is fundamentally incorrect: the leftover changeset still contains entries for already-released packages, which would cause spurious version bumps on the next release. Fixing this would require additional logic to skip already-bumped packages, adding complexity comparable to the rewrite approach without its clarity.

### Track consumed entries in a separate manifest

Maintain a `.chronicle/released.toml` recording which package entries have been consumed from which changeset files. This preserves changeset files as immutable records but introduces a new file format, a new consistency concern (manifest and changesets can drift out of sync), and additional cleanup logic. The complexity is disproportionate to the problem, especially given that changesets are transient artifacts designed to be consumed and deleted.

## Errata

**2026-03-09**: [ADR-016](016-rename-release-to-prepare.md) renames the `chronicle release` subcommand to `chronicle prepare`. References to `release` as a subcommand name in this ADR now refer to `chronicle prepare`. The scoped changeset consumption behavior is unchanged. See [ADR-016](016-rename-release-to-prepare.md) for details.
