# ADR-026: Per-Package Change Level Selection in TUI Wizard

## Status

Accepted

## Context

Cursus's `cursus change` command records changeset files in `.cursus/` with Hugo-style TOML frontmatter mapping package names to semver bump levels ([ADR-002](002-changeset-recording.md)). The file format already supports per-package change levels:

```
+++
package-name = "minor"
another-package = "patch"
+++

Description message here
```

However, the TUI wizard for `change` does not expose this capability. The current wizard consists of two screens ([ADR-020](020-tui-screen-submodule-structure.md)):

1. **SelectProjects**: A checkbox list where the user selects which packages are affected.
2. **SelectChangeType**: A single major/minor/patch selector applied uniformly to all selected packages.

After the TUI completes, the user's `$EDITOR` is launched to compose the changeset description. This external editor handoff works but introduces a jarring context switch -- the user leaves the TUI, enters a terminal editor, writes a message, saves, and exits. For short descriptions (the common case), this is heavyweight. The project now has a `ratatui-textarea` component (introduced in [ADR-019](019-improved-init-workflow.md)) that can provide inline text editing within the TUI.

The two-screen design also means a developer who needs to record a minor bump for one package and a patch bump for another must run `cursus change` twice, producing two separate changeset files. This is friction that discourages accurate semver classification -- users tend to pick a single level for all packages rather than running the command multiple times.

## Decision

We will redesign the change TUI wizard into a two-screen flow that combines package selection with per-package change level assignment on the first screen, and adds an inline message editor on the second screen.

### Screen 1: Package selection with per-package change levels

The first screen displays a list of packages. Each row contains:

- A **checkbox** on the left indicating whether the package is included in the changeset.
- The **package name**.
- A **change level indicator** (Major / Minor / Patch) on the right-hand side, visible only when the package is checked.

When a package is unchecked, the change level indicator for that row is hidden. When the user checks a package, the indicator appears with the current default level.

**Default state**: All packages that have been detected as changed (via the existing changed-file detection logic) are pre-selected. Pre-selected packages default to **patch** level. Packages not detected as changed are unchecked by default but can be manually selected.

**Keyboard controls**:

- **Up / Down** arrow keys (or `k`/`j`) move the cursor between packages in the list.
- **Space** toggles the checkbox for the focused package. Checking a package reveals its change level indicator at the default level; unchecking hides it.
- **Left / Right** arrow keys shift the change level of the **currently focused** package, wrapping cyclically: patch -> major -> minor -> patch (Right) and patch -> minor -> major -> patch (Left). This means any level is reachable from any other level with a single keypress. This only has an effect when the focused package is checked. Vim-style `h`/`l` keys are aliases for Left/Right.
- **`,` and `.`** (comma and period) provide bulk adjustment. They shift the **first checked package's** change level backward (`,`) or forward (`.`) using the same wrapping cycle as Left/Right, then **force all other checked packages to that same level**. This makes `,`/`.` a "set all to X" operation anchored on the first checked package's current state, providing a fast path for the common case where all packages share the same bump level.
- **`a`**, **`c`**, and **`u`** provide group toggling: `a` toggles all packages, `c` toggles the "Changed" group, `u` toggles the "Unchanged" group. If all packages in the targeted group are already checked, the key unchecks them; otherwise it checks them all.
- **Mouse clicks** on a project row toggle its checkbox. Clicking a level indicator (Major / Minor / Patch) on a selected project sets its level directly.
- **Enter** confirms the selection and proceeds to Screen 2. At least one package must be checked; if none are checked, Enter displays an inline error.
- **Escape** (or `q`) cancels the wizard entirely.

### Single-package screen

When the repository contains only a single package, the multi-package selection screen is unnecessary. The wizard will instead enter a **dedicated single-package screen** showing only the package name and a Major / Minor / Patch selector. The package is implicitly selected (no checkbox needed). Left/Right arrow keys adjust the level with the same wrapping behaviour. Enter proceeds to Screen 2 (message input).

This is a separate `Screen` variant in the code, not a conditional branch within the multi-package screen. The wizard forms a V-shaped flow: two different entry points (single-package screen or multi-package screen) depending on the project structure, converging on the shared message input screen before completion.

### Screen 2: Message input

The second screen presents a multi-line `ratatui-textarea` widget for composing the changeset description inline. This replaces the default behaviour of launching an external editor after the TUI exits.

**Keyboard controls**:

- Standard text editing keys within the textarea (typing, backspace, arrow keys for cursor movement, etc.) as provided by the `ratatui-textarea` component.
- **Enter** confirms the message and completes the wizard.
- **Alt+Enter** (or **Shift+Enter**) inserts a newline within the textarea, allowing multi-line descriptions.
- **Ctrl+E** terminates the TUI and drops to the user's external editor (`$VISUAL`, `$EDITOR`, falling back to `nano`, `vim`, `vi`) as the final step, in the same manner as the current post-TUI editor launch. The TUI does not resume after the editor exits -- the editor session is the final interaction, and the changeset is written from the editor's output. This avoids the complexity of suspending and restoring the TUI mid-flow.
- **Escape** returns to the previous screen (multi-package or single-package), preserving the package selection and change level state.

**Empty message**: An empty message is permitted. Changeset descriptions are optional per [ADR-002](002-changeset-recording.md).

### Screen file structure

Following [ADR-020](020-tui-screen-submodule-structure.md), the redesigned wizard will have three screen files replacing the current two:

- One file for the multi-package selection screen (replacing both `select_projects.rs` and `select_change_type.rs`).
- One file for the single-package selection screen (new).
- One file for the message input screen (new).

The `Screen` enum in `mod.rs` will carry three variants: one for the multi-package screen, one for the single-package screen, and one for the message input screen. The entry point in `mod.rs` inspects the project count and initialises the appropriate first-screen variant.

### ChangeResult update

The `ChangeResult` struct will change from carrying a single `change_type: ChangeType` to carrying per-package change levels. Each selected project is paired with its individually assigned change level. The result will also indicate whether the user completed the message inline (carrying the message text) or requested an external editor via Ctrl+E (signalling the caller to launch the editor, as the current workflow does). This aligns the TUI output with the file format's existing per-package semantics.

### Non-interactive mode

The non-interactive path (`--change-type/-t`, `--message/-m`, `--project/-p`) is unchanged. It continues to apply a single change type to all specified projects. Per-package levels in non-interactive mode are a separate concern and would require new CLI syntax.

## Consequences

### Positive

- Users can assign distinct semver levels to different packages in a single `cursus change` invocation, eliminating the need to run the command multiple times for mixed-level changesets.
- The bulk adjustment keys (`,` / `.`) preserve the efficiency of the old workflow: users who want the same level for all packages can set it with a single keypress, since the operation shifts the first checked package and forces all others to match.
- The file format requires no changes -- per-package levels are already supported. This decision closes the gap between the format's capability and the TUI's expressiveness.
- The inline message editor removes the context switch to an external editor for short descriptions, making the common case faster and keeping the user within the TUI.
- The Ctrl+E escape hatch terminates the TUI and drops to an external editor, preserving full editing capability for users who need it without the complexity of suspending and restoring the TUI.
- The single-package shortcut provides a cleaner experience for single-package repositories, removing visual clutter that adds no value.

### Negative

- The keyboard control scheme on Screen 1 is more complex than the current design. Users must learn that Left/Right operates on the focused package while `,`/`.` sets all checked packages to the same level. This distinction, along with the wrapping behaviour, may not be immediately discoverable without a help line or legend on the screen.
- The unified selection screen carries more visual information density than either of the two screens it replaces. Users with many packages may find the combined checkbox-plus-level-indicator layout harder to scan than the current sequential flow.
- The `ChangeResult` struct change is a breaking internal API change. All consumers of `ChangeResult` (the changeset file writer, tests, and any code that reads the result) must be updated to handle per-package levels and the two completion modes (inline message vs editor handoff).
- Non-interactive mode remains limited to a single change type for all packages, creating a capability gap between interactive and non-interactive usage.
- Enter submitting the message while Alt+Enter / Shift+Enter inserts a newline is a common pattern but may surprise users accustomed to Enter always inserting a newline in text areas. A help line indicating the key bindings is important.

### Neutral

- The existing changed-file detection logic for pre-selecting packages is unaffected. Only the presentation and level-assignment UX changes.
- The `--auto` flag ([ADR-025](025-auto-changeset-from-conventional-commit.md)) is unaffected, as it bypasses the TUI entirely and derives change levels from commit parsing.
- The changeset file format is unchanged. Files produced by the new TUI are identical in structure to files produced by the old TUI or the non-interactive path.
- The single-package screen is a presentation optimisation; it produces the same `ChangeResult` as the multi-package screen with one package checked.
- The wizard has three `Screen` variants but only two are active in any given run, forming a V-shaped flow. The number of user-visible steps remains two regardless of project count.
- The wrapping cycle on change levels means patch, minor, and major are all equidistant from each other (one keypress in either direction), removing any directional bias in the UI.

## Alternatives Considered

### Keep two screens and add per-package override on a third screen

Retain the current flow where Screen 1 selects packages and Screen 2 selects a global change level, then add a third screen to override individual packages. This was rejected because it adds complexity without reducing navigation steps. The two-screen flow already feels like one screen too many for what is fundamentally a single decision ("which packages changed and how"). Adding a third screen would make it worse.

### Per-package CLI flags for non-interactive mode

Extend the non-interactive CLI to support syntax like `-p my-lib:minor -p my-app:patch` for per-package levels. This was considered but excluded from this ADR's scope. The non-interactive path works well for CI scripts that typically apply a uniform change type. Per-package CLI syntax would add parsing complexity and is better evaluated as a separate decision if demand arises. The TUI is where users most naturally make per-package distinctions.

### Inline editing with a table/grid layout

Present packages in a table with columns for the checkbox, package name, and three radio-button-style cells (Major / Minor / Patch) where the user navigates a 2D grid. This was rejected because ratatui's widget model is better suited to list-based navigation, and 2D grid navigation introduces complexity in cursor management (horizontal vs vertical movement semantics become ambiguous). The chosen design keeps vertical navigation for package focus and horizontal navigation for level adjustment, which maps cleanly to arrow key semantics.

### Default to minor instead of patch

Pre-select "minor" as the default level for checked packages, on the theory that most intentional changes are feature additions. This was rejected because patch is the safest default -- it produces the smallest version bump. Users who want minor or major can bulk-adjust with one or two `.` presses. Defaulting to a higher bump level risks accidental major/minor bumps when users confirm without reviewing levels.

### Launch external editor only (no inline textarea)

Keep the current approach of launching `$EDITOR` after the TUI exits for message composition. This was rejected because the external editor handoff is disproportionately heavyweight for short changeset descriptions, which are the common case. The `ratatui-textarea` component is already available in the project ([ADR-019](019-improved-init-workflow.md)) and provides a sufficient editing experience for typical messages. The Ctrl+E escape hatch ensures the external editor remains accessible for complex editing needs.
