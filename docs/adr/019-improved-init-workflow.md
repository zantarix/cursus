# ADR-019: Improve the init workflow to cover git and GitHub configuration

## Status

Accepted

## Context

[ADR-001](001-project-initialisation.md) established the `cursus init` command with a two-screen TUI wizard that asks only one meaningful question: which package manager to use. Since then, Cursus has grown to include git lifecycle automation (`[git]` section, [ADR-006](006-git-lifecycle-hooks.md)/[ADR-015](015-ci-managed-release-workflow.md)), GitHub Releases (`[github]` section, [ADR-005](005-github-releases.md)), and a CI-managed workflow (`cursus ci`, [ADR-015](015-ci-managed-release-workflow.md)). These features all require configuration in `.cursus/config.toml`, but `init` provides no guidance for any of them.

As a result, most users must hand-edit the config file immediately after running `init`. This is a poor onboarding experience, especially given that git automation and GitHub Releases are the features most users will want. The existing auto-detection infrastructure (filesystem-based package manager detection, GitHub remote URL parsing via `GitHubRepo::detect_in`) is underutilised during initialisation.

Additionally, Cursus treats multi-package-manager monorepos as a first-class use case -- `[npm]` and `[cargo]` can both be enabled simultaneously. However, the current init wizard forces a binary choice between them. Polyglot monorepos (e.g., a Rust backend with a TypeScript frontend) must hand-edit the config to enable both, which is a gap in the onboarding flow.

The challenge is finding the right balance: the init wizard should cover enough surface that most users get a working config without manual editing, while avoiding an overwhelming questionnaire about every possible option. Advanced options like `git.extra_files`, `github.build_command`, `github.artifacts`, and `npm.lock_command` are rarely needed at setup time and are better left for manual configuration -- but they should be discoverable without reading documentation.

## Decision

We will expand `cursus init` into a purely interactive TUI wizard that guides users through four concerns: package managers, manifest paths, git automation, and GitHub integration. The wizard uses progressive disclosure so that users who want a simple setup answer fewer questions. There will be no non-interactive CLI flags; the existing `--package-manager` flag will be removed. Scripts that need to create a config can write the TOML file directly.

### TUI flow

The wizard proceeds through screens in this order, with conditional branches.

```
(config already exists?)
     |
     yes -> 1. ConfirmOverwrite  "Config exists. Overwrite?" [Yes / No]
     |                                |
     no                          (if No -> cancel)
     |                                |
     +--------------------------------+
     |
2. PackageManagers  "Which package managers?" [Cargo] [NPM]  (multi-select)
     |
     (at least one must be selected)
     |
     (for each selected PM where root manifest is not found at git root)
     |
3. ManifestPath     "Cargo.toml not found. Enter subdirectory path:" (text input)
     |               (one screen per PM with missing manifest; skipped if manifest found)
     |
4. EnableGit        "Enable git automation?" [Yes / No]
     |
     (if No -> write config, go to OpenEditor)
     |
5. GitStrategy      "Git strategy?" [Push / Branch]
     |
     +--- Branch ---> (auto-detect owner/repo) ---> 7. EditGitHub
     |
     Push
     |
6. EnableGitHub     "Enable GitHub Releases?" [Yes / No]
     |
     (if No -> write config, go to OpenEditor)
     |
     (auto-detect owner/repo from origin remote)
     |
7. EditGitHub       "GitHub repository:" owner/repo (editable text input)
     |               [Confirm / Cancel]
     |
     (write config)
     |
8. OpenEditor       "Open config in editor?" [Yes / No]
     |
     (if Yes -> open $VISUAL/$EDITOR with config file path)
     |
     done
```

**Screen details:**

1. **ConfirmOverwrite** -- Shown only when `.cursus/config.toml` already exists. Displays a clear warning that proceeding will overwrite the existing configuration. Yes/No toggle, default No. When no config exists, this screen is skipped entirely and the wizard begins at the PackageManagers screen.

2. **PackageManagers** -- A multi-select screen replacing the current single-select toggle. Each package manager is rendered as a checkbox that can be independently toggled on or off. Auto-detection pre-checks whichever manifest files exist in the repository: `Cargo.toml` for Cargo, `package.json` for npm. When both files are present, both are pre-checked, making polyglot monorepo setup a single Enter press. When neither file is detected, npm is pre-checked as a fallback (matching current behaviour). The user navigates between checkboxes with arrow keys or Tab and toggles with Space. Enter confirms the selection; at least one package manager must be selected or Enter is rejected with a visible hint.

3. **ManifestPath** -- A text input screen shown once per selected package manager whose root manifest file is not found at the git root (e.g., no `Cargo.toml` in the git root when Cargo is selected). The screen asks the user to enter the subdirectory path where the manifest lives, relative to the git root. The entered path is written as `npm.path` or `cargo.path` in the config. If the manifest file still cannot be found at the entered path, the wizard accepts the input anyway and proceeds -- the user may be running init before creating the project structure. This screen is skipped for any package manager whose manifest is found at the git root.

4. **EnableGit** -- Yes/No toggle. Default selection: Yes. Asks whether Cursus should automate git operations (committing, tagging, pushing/branching) during `prepare` and `publish`. Answering No skips the git strategy and GitHub screens.

5. **GitStrategy** -- Push/Branch toggle. Default selection: Push. Explains the difference: Push commits and pushes directly to the current branch; Branch creates a release branch (suitable for PR-based workflows). Selecting Branch implies GitHub integration: the wizard skips the EnableGitHub screen and proceeds directly to EditGitHub with GitHub enabled. This is because the branch strategy exists specifically to support PR-based workflows via GitHub -- a user choosing Branch almost certainly wants GitHub integration. The runtime derivation logic (strategy defaults to Branch when `github.enabled`) only applies when strategy is absent from the config file. Since init will now write an explicit strategy value, the runtime derivation will not override it. If a user later hand-edits the config to disable GitHub while keeping `strategy = "branch"`, Cursus's existing runtime behaviour handles this correctly -- the strategy is explicit and is not re-derived.

6. **EnableGitHub** -- Yes/No toggle. Default selection: No (matching the opt-in principle from [ADR-005](005-github-releases.md)). Only shown when the Push strategy was selected on screen 5. Asks whether Cursus should create GitHub Releases. When Branch was selected, this screen is skipped and GitHub is enabled automatically.

7. **EditGitHub** -- Shown when GitHub is enabled (either explicitly via screen 6, or implicitly via Branch strategy on screen 5). Displays the auto-detected `owner/repo` from the git origin remote (using the existing `GitHubRepo::parse_url` logic) in an editable text field powered by `ratatui-textarea` in single-line mode. The field is pre-populated with the detected `owner/repo` string, or left empty if detection fails (no origin remote, non-GitHub remote). The user can edit the value to correct it or fill it in from scratch. Enter confirms the current value; Esc cancels. If the user confirms an empty value, the config is written without `owner`/`repo` fields (the user must add them manually before running `publish`). If the user confirms the field without editing it (i.e., the value still matches the auto-detected `owner/repo`), the config is written without explicit `owner`/`repo` fields -- identical to confirming an empty field -- and the detected values are rendered as commented-out hints in the generated config. This lets users accept the detection as a runtime default while keeping the hint visible for reference. If the user edits the value to something different from the detected one, the entered value is written as explicit active TOML. The entered value is validated: it must either be empty or match the `owner/repo` format (exactly one `/` separating two non-empty segments that pass `GitHubRepo::new` validation).

8. **OpenEditor** -- Shown to all users after the config file has been written. Asks "Would you like to open the configuration file in your editor now?" Yes/No toggle, default No. If Yes, opens the config file using the editor resolved from `VISUAL` / `EDITOR` (the existing `Env.editor()` mechanism), falling back to the first available editor found on PATH from `nano`, `vim`, `vi`, or `emacs`. If no editor is configured and none of the fallbacks are found, an error is returned suggesting the user set `VISUAL` or `EDITOR`. This gives users a seamless way to review or tweak advanced options immediately after init.

### Text input via ratatui-textarea

The ManifestPath and EditGitHub screens both require inline text input. The `ratatui-textarea` crate (a community fork of `tui-textarea` by rhysd, maintained by orhun) provides a mature single-line input widget for ratatui with cursor movement, insertion, deletion, and Emacs-style keybindings out of the box. It tracks recent ratatui releases and includes a dedicated `single_line` example. Cursus will use `ratatui-textarea` in single-line mode for all text input screens in the init wizard. This avoids reimplementing text editing from scratch and keeps the init-specific code focused on screen flow rather than input mechanics.

### No non-interactive CLI flags

The `init` command will be interactive-only. The existing `--package-manager` flag will be removed. Rationale: `init` is a human-facing onboarding wizard run once per repository. Scripts and CI pipelines that need a config file can write the TOML directly -- the format is stable and well-documented. Removing CLI flags keeps the init command simple and avoids maintaining a parallel non-interactive code path that mirrors every TUI question. If demand for scripted init arises in the future, flags can be added in a subsequent ADR without breaking changes.

Running `cursus init --no-interactive` will produce an error message explaining that init is interactive-only and suggesting that scripts write the config file directly.

### Config output

Init will write only the fields that the user actively chose during the wizard. Fields that the wizard does not ask about (e.g., `tag_format`, `release_branch_prefix`, `extra_files`, `build_command`, `artifacts`) are not written as active values. Instead, the generated config file will include commented-out versions of all available options with their default values and brief inline documentation. This makes the file self-documenting: users who want to customise advanced options can uncomment and edit them without consulting external documentation.

Because TOML comments are not preserved by `toml::to_string`, the config file will be generated using a template approach rather than pure serde serialisation. The template emits the active values as normal TOML, followed by commented-out blocks for each section's remaining options.

**Example: single package manager with git enabled, no GitHub:**

```toml
# [global]
# disable_dependency_cycle_warnings = false  # Suppress circular dependency warnings

[cargo]
enabled = true
# path = "subdir/"              # Subdirectory for Cargo.toml (relative to git root)

# [npm]
# enabled = false
# path = "subdir/"              # Subdirectory for package.json (relative to git root)
# lock_command = "npm install"  # Custom command to update the lock file

[git]
enabled = true
strategy = "push"
# tag_format = "auto"                            # Tag format: "auto", "prefixed", or "simple"
# release_branch_prefix = "cursus-release/"   # Prefix for release branches (branch strategy)
# extra_files = []                               # Additional files to stage before committing

# [github]
# enabled = false
# owner = ""                        # GitHub owner (auto-detected from remote if omitted)
# repo = ""                         # GitHub repo (auto-detected from remote if omitted)
# build_command = ""                # Shell command to build release artifacts
# pull_request_title = ""           # Custom PR title (default: "Release updates")
# [github.artifacts]                # Map of display name -> file path for release assets
```

**Example: polyglot monorepo with git and GitHub:**

```toml
# [global]
# disable_dependency_cycle_warnings = false  # Suppress circular dependency warnings

[cargo]
enabled = true
# path = "subdir/"              # Subdirectory for Cargo.toml (relative to git root)

[npm]
enabled = true
# path = "subdir/"              # Subdirectory for package.json (relative to git root)
# lock_command = "npm install"  # Custom command to update the lock file

[git]
enabled = true
strategy = "branch"
# tag_format = "auto"                            # Tag format: "auto", "prefixed", or "simple"
# release_branch_prefix = "cursus-release/"   # Prefix for release branches (branch strategy)
# extra_files = []                               # Additional files to stage before committing

[github]
enabled = true
owner = "acme"
repo = "my-app"
# build_command = ""                # Shell command to build release artifacts
# pull_request_title = ""           # Custom PR title (default: "Release updates")
# [github.artifacts]                # Map of display name -> file path for release assets
```

### Options that remain out of scope for init screens

The following config fields are not asked about in the wizard. They appear as commented-out options in the generated config file:

- `[global].disable_dependency_cycle_warnings` -- rarely needed, only relevant for monorepos with circular dependencies
- `[npm].lock_command` -- only needed for custom lock file workflows
- `[git].tag_format` -- the default `auto` is correct for the vast majority of projects
- `[git].release_branch_prefix` -- the default `cursus-release/` is almost always correct
- `[git].extra_files` -- only needed for custom lock commands or generated files
- `[github].build_command` -- build artifact configuration is a post-init concern
- `[github].artifacts` -- same as above
- `[github].pull_request_title` -- the default is almost always acceptable

## Consequences

### Positive

- Most users will get a fully working config from `init` without needing to edit the file manually, reducing onboarding friction.
- Progressive disclosure keeps the wizard short for simple use cases (package-manager-only setups see only 3-4 screens).
- Multi-package-manager selection is surfaced directly in the wizard. Polyglot monorepos where both `Cargo.toml` and `package.json` exist will have both pre-checked, making the common case a single Enter press.
- The ManifestPath screen handles the case where a package manager root lives in a subdirectory, eliminating one of the most common reasons to hand-edit the config after init.
- GitHub remote auto-detection pre-populates the EditGitHub field, and inline editing lets users correct it without leaving the wizard. This eliminates a common source of configuration errors (typos in owner/repo) while still handling edge cases like incorrect remotes.
- Selecting Branch strategy automatically enables GitHub, matching the overwhelmingly common intent behind that choice and saving users a redundant question.
- The template config with commented-out options makes every available setting discoverable without leaving the editor. Users can uncomment and customise without consulting documentation.
- The OpenEditor screen provides a seamless transition from wizard to manual customisation for users who want to tweak advanced options immediately.
- Removing the non-interactive CLI flags simplifies the command and avoids maintaining a parallel code path. Scripts can write TOML directly, which is more flexible than any flag-based API.

### Negative

- The TUI state machine becomes more complex, with conditional branches and up to 8 screen variants instead of 2. This increases the testing surface for `handle_key()`.
- The package manager screen changes from a simple left/right toggle to a multi-select with checkbox semantics (Space to toggle, Enter to confirm). This is a different interaction pattern from the other screens in the wizard, which use binary toggles. However, checkbox multi-select is a well-understood UI convention and the screen remains simple with only two items.
- Adding `ratatui-textarea` as a dependency increases the dependency tree. However, the crate is well-maintained, narrowly scoped, and avoids reimplementing text input handling from scratch.
- The template-based config generation is more complex than pure serde serialisation. The template must be kept in sync with the `Config` struct when fields are added or removed. This is a maintenance cost, but the benefit of commented-out documentation justifies it.
- Removing `--package-manager` and non-interactive mode is a breaking change for any existing scripts that call `cursus init`. However, init is typically run once interactively per repository, and scripts can write the TOML file directly as a straightforward migration.
- Branch strategy automatically enabling GitHub is opinionated: a user who genuinely wants Branch without GitHub must select Push in the wizard and then hand-edit the strategy to `branch` afterward. This is an acceptable trade-off because the branch-without-GitHub use case is rare -- the strategy exists primarily for PR-based workflows.

### Neutral

- The ConfirmOverwrite screen replaces the old Confirm screen. The confirm-before-setup prompt is removed for fresh repositories, making init slightly faster for the common case. Existing repositories that already have a config see a clear overwrite warning instead of a generic "Set up Cursus?" prompt.
- The `Screen` enum for the init TUI will grow to 8 variants. The existing testing pattern (pure `handle_key()` function with exhaustive key tests) scales to this without architectural changes. Text input screens delegate to `ratatui-textarea` for keystroke handling, reducing the custom key-handling logic needed.
- The `Env` struct is already threaded through `cmd_init` and carries the editor configuration. The OpenEditor screen reuses the existing `Env.editor()` and `Env.run_interactive()` infrastructure with no new plumbing required.

## Alternatives Considered

### Full-screen form with all options on one page

A single form showing all configuration options at once, with fields for package manager, git, GitHub, strategy, owner, repo, etc. This was rejected because it overwhelms new users and is difficult to implement well in a terminal UI without a mature form framework. Progressive disclosure through sequential screens is simpler to build, test, and use.

### Always ask EnableGitHub regardless of strategy

Showing the EnableGitHub Yes/No screen for both Push and Branch strategies, rather than having Branch imply GitHub. This was rejected because selecting Branch without GitHub is a rare edge case -- the strategy exists specifically for PR-based workflows. Asking the question would almost always result in a Yes answer for Branch users, adding a redundant step. Users who genuinely want Branch without GitHub can select Push in the wizard and hand-edit the strategy afterward, or disable GitHub via the OpenEditor screen.

### Omit git strategy from init

Only asking about `git.enabled` and letting the runtime derivation logic choose the strategy (push when no GitHub, branch when GitHub). This was rejected because the strategy choice has significant workflow implications (direct push vs. PR-based) and users should make this decision consciously during setup. Writing the explicit value also makes the config file self-documenting.

### Keep single-select for package manager

Retaining the current binary Cargo/NPM toggle and requiring users to hand-edit the config to enable both. This was rejected because multi-package-manager monorepos are a first-class use case in Cursus's architecture ([ADR-001](001-project-initialisation.md) explicitly supports it), and the init wizard should reflect that. Auto-detection makes the multi-select zero-friction for both single-PM and multi-PM projects: when only one manifest file exists, only one checkbox is pre-checked, and the experience is essentially the same as a single-select.

### Hand-rolled text input instead of ratatui-textarea

Implementing cursor movement, character insertion/deletion, and key handling manually for the ManifestPath screen. This was rejected because `ratatui-textarea` (a community fork of `tui-textarea` that tracks recent ratatui releases) already provides a well-tested single-line input widget with Emacs-style keybindings, cursor movement, and proper Unicode handling. Reimplementing this would be significant effort with little benefit, and the resulting code would be less featureful and more bug-prone.

### Expose non-interactive CLI flags for scripting

Adding `--package-manager`, `--git`, `--github`, `--git-strategy`, and related flags to support non-interactive init via CLI. This was rejected because init is a human-facing onboarding wizard run once per repository. The TOML config format is stable and simple enough that scripts can write it directly, which is more flexible than any flag-based API could be. Maintaining a parallel non-interactive code path that mirrors every TUI question adds complexity and testing burden with little practical benefit. If demand arises, flags can be added later without breaking changes.

### Pure serde serialisation for config output

Using `toml::to_string_pretty` to generate the config file, relying on serde's `skip_serializing_if` to omit unconfigured fields. This was rejected because it cannot produce commented-out documentation for unused options. The template approach is more work to maintain but produces a significantly more useful config file for users who want to customise beyond the defaults.
