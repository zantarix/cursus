# ADR-003: `chronicle release` Command

## Status

Proposed

## Context

Chronicle collects changesets describing what has changed across packages in a repository. Each changeset is a `.chronicle/*.md` file with TOML frontmatter mapping package names to a semver bump level (`major`, `minor`, or `patch`), plus an optional freeform description.

There is currently no mechanism to consume these changesets and apply the accumulated changes to the repository. A release workflow needs to translate pending changesets into concrete version bumps, changelog entries, and cleanup of the consumed changesets.

The release workflow is a three-step process:

1. **Update the filesystem** — bump versions, generate changelogs, consume changesets (this ADR)
2. **Commit to source control** — managed by the user or CI, not by Chronicle
3. **Publish to registries** — handled by a separate `chronicle publish` command (see ADR-004)

Chronicle intentionally does not handle the commit step. Users run different CI systems, may want different commit strategies (single commit vs. per-package), and may require GPG signing or other policies that Chronicle should not assume.

## Decision

Implement a `chronicle release` subcommand that performs the following steps:

### 1. Gather pending changesets

Read all `.chronicle/*.md` files (excluding `config.toml`), parsing each with the existing `parse_changeset()` function.

If no changesets are found, exit early with a message: "No pending changesets. Nothing to release."

### 2. Aggregate changes per package

For each package mentioned across all changesets, determine the highest change type: `major` > `minor` > `patch`. A package that appears as `minor` in one changeset and `patch` in another receives a `minor` bump.

### 3. Resolve current versions

Read the current version for each affected package from its source of truth:

- **Cargo** — `[package].version` in `Cargo.toml`
- **npm** — `"version"` in `package.json`

Packages not mentioned in any changeset are left untouched.

### 4. Compute next versions

Apply standard semver bumping rules to each affected package's current version:

- `major` — increment major, reset minor and patch to 0
- `minor` — increment minor, reset patch to 0
- `patch` — increment patch

Pre-release and build metadata are stripped on bump (standard semver behaviour).

### 5. Update version files and lock files

Write the new version back to the package's manifest file (`Cargo.toml` or `package.json`). Only the version field is modified; all other content is preserved.

After updating the version, Chronicle automatically updates the lock file:

- **Cargo**: Runs `cargo generate-lockfile` in the workspace root to regenerate `Cargo.lock`
- **npm**: Auto-detects the lock file type and runs the appropriate command:
  - `package-lock.json` → `npm install --package-lock-only`
  - `pnpm-lock.yaml` → `pnpm install --lockfile-only`
  - `yarn.lock` → `yarn install --mode update-lockfile`
  - No lock file → no-op

Users can override the npm lock file update command by setting `lock_command` in `.chronicle/config.toml`:

```toml
[npm]
enabled = true
lock_command = "pnpm install --lockfile-only"
```

### 6. Generate changelog

Append entries to a `CHANGELOG.md` file. New entries are prepended (most recent version at the top).

Format:

```markdown
## 1.2.0

### Minor Changes

- Added foo bar feature
- Improved baz handling

### Patch Changes

- Fixed quux edge case
```

Sections are ordered: Major Changes, Minor Changes, Patch Changes. Each entry comes from the changeset's `message` field. Changesets without a message are omitted from the changelog.

Changelog location:

- Single-package repos: `CHANGELOG.md` at the repository root
- Monorepos: `CHANGELOG.md` adjacent to each package's manifest file

### 7. Consume changesets

Delete all processed `.chronicle/*.md` files. The `.chronicle/` directory and `config.toml` are preserved.

### 8. Print summary

Output a summary of what was released:

```
chronicle-cli: 0.1.0 -> 0.2.0 (minor)
@mscharley/chronicle: 0.1.0 -> 0.2.0 (minor)
```

### Dry-run support

A `--dry-run` flag prints the summary and changelog entries without writing any changes to disk. This is useful for CI preview steps and manual inspection.

### Interactive vs. non-interactive

The release command does not require a TUI. It is a batch operation suitable for both local and CI use. The `--no-interactive` flag is accepted but has no effect on behaviour.

## Consequences

- The filesystem is the only thing modified by this command. Users retain full control over source control and publishing.
- Changesets are consumed (deleted) on release, so the command is not idempotent. Running it twice without new changesets results in a no-op.
- Lock files are automatically updated after version bumps, reducing manual steps and ensuring consistency between manifest and lock files.
- Inter-package dependency version updates in monorepos (e.g., updating package A's dependency on package B after B is bumped) are deferred to a future enhancement.
