# ADR-002: Changeset Recording

## Status

Accepted

## Context

Cursus needs a way for developers to record what has changed in a repository as they work, so that this information can later be consumed to generate changelogs and determine version bumps. Not every commit warrants a release note — CI changes, refactors, and other internal work should not produce changelog entries. The recording mechanism must therefore be an explicit, opt-in action by the developer.

A changeset must capture which packages are affected, the severity of the change (for semver bumping), and an optional human-readable description. Multiple developers working on separate branches must be able to create changesets without conflicts.

This command requires a repository that has been initialised with `cursus init` (see [ADR-001](001-project-initialisation.md)).

## Decision

### The `cursus change` command

The `cursus change` command (also the default when no subcommand is given) records a changeset file. The workflow is:

1. Load configuration and enumerate all projects from enabled package managers
2. Select which projects are affected and the type of change
3. Write a changeset file to `.cursus/`
4. Optionally open the user's editor to add a description

**Interactive mode** (default): A TUI wizard allows the user to select projects and change type, then opens `$EDITOR` (falling back to `nano`, `vim`, `vi`) for the description.

**Non-interactive mode** (`--no-interactive`): Requires `-t <major|minor|patch>` and `-m <message>`. Optionally `-p <project-name>` (repeatable) to select specific projects; defaults to all projects.

### Changeset file format

Changesets use Hugo-style `+++` TOML frontmatter:

```markdown
+++
package-name = "minor"
another-package = "patch"
+++

Description message here
```

- The frontmatter is TOML mapping package names to change types (`"major"`, `"minor"`, or `"patch"`)
- The body after the closing `+++` is an optional freeform description
- A single changeset can reference multiple packages with different change types
- All change types within a single changeset are currently the same (the TUI selects one type applied to all selected projects), but the file format supports per-package types for future flexibility

### File naming and storage

- Changesets are stored as `.cursus/*.md` alongside `config.toml`
- Filenames are randomly generated three-word petnames (e.g., `scrupulously-affirming-thornbill.md`) to avoid naming conflicts when multiple developers create changesets concurrently
- The `.cursus/` directory is created automatically if it doesn't exist
- Changeset files are intended to be committed to source control and accumulate on the main branch until consumed by a release (see [ADR-003](003-release-command.md))

### When changesets are NOT created

Changesets are explicitly opt-in. Changes that don't warrant a changelog entry — CI configuration, internal refactors, documentation updates, dependency bumps — simply don't get a changeset. The absence of a changeset for a commit is the normal, expected case. Only user-facing or noteworthy changes should produce changesets.

## Consequences

- Changesets accumulate as individual files in `.cursus/`, making them merge-friendly — concurrent branches can each add changesets without conflicts.
- The random filename strategy eliminates coordination between developers but means filenames carry no semantic meaning.
- The file format supports per-package change types, providing forward compatibility even though the current TUI applies a single type to all selected packages.
- The `.cursus/` directory serves double duty for both configuration and changeset storage. This keeps the repository footprint minimal (one directory) but means glob patterns for changesets must exclude `config.toml`.
- Interactive and non-interactive modes share the same underlying logic, with the TUI being a presentation layer on top. This ensures CI scripts produce identical changeset files to local development.

## Errata

### 2026-04-27: Default project selection in non-interactive mode

The Decision section above states that in non-interactive mode, omitting `-p <project-name>` defaults to selecting all projects. This default behavior has been changed to align with the TUI's pre-selection logic.

- When `cursus change --no-interactive` is invoked without any `--project` flags, the command now selects only git-changed projects, using the same three-source diff logic as the TUI pre-selection (committed changes since `origin/HEAD`, staged changes, and unstaged changes).
- If no git-changed projects are detected, the command falls back to selecting all projects (preserving the original default for repositories with no detectable changes).
- Explicit `--project` flags continue to take exact precedence and are unaffected by this change.

No new ADR was written for this change. It is a bug fix and behavior alignment between the interactive and non-interactive modes (consistent with the original Decision intent that "interactive and non-interactive modes share the same underlying logic"), rather than a new architectural decision.
