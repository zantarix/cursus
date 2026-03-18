# TUI wizard strings

# Button labels
button-yes = Yes
button-no = No
button-major = Major
button-minor = Minor
button-patch = Patch
button-push = Push
button-branch = Branch

# Individual help instructions (atomic, for composition and reuse)
help-switch-buttons = ←/→ or click to switch
help-confirm-click = Enter or click to confirm
help-cancel-esc = Esc to cancel
help-move-focus = ↑/↓/Tab: move focus
help-toggle-click = Space/Click: toggle
help-confirm-required = Enter: confirm (≥1 required)
help-confirm = Enter: confirm
help-cancel = Esc: cancel
help-submit = Enter: submit
help-newline = Alt+Enter (or Shift+Enter): newline
help-open-editor = Ctrl+E: open editor
help-back = Esc: back
help-navigate = ↑/↓/j/k: navigate
help-toggle = Space: toggle
help-change-level = ←/→: change level
help-set-all-levels = ,/.: set all levels
help-filter-all = a: all
help-filter-changed = c: changed
help-filter-unchanged = u: unchanged

# Shared button-screen help line
button-screen-help = { help-switch-buttons }, { help-confirm-click }, { help-cancel-esc }

# Init wizard screens
confirm-overwrite-question = Config already exists. Overwrite?

enable-git-question = Enable git automation? (commits, tags, push/branch on prepare and publish)

enable-github-question = Enable GitHub Releases? (creates a release on GitHub after publish)

git-strategy-question = Git strategy? Push: commit to current branch. Branch: create release branch (for PRs).

open-editor-question = Open the config file in your editor after saving?

# Select package managers screen
select-pms-question = Which package managers does this project use?
select-pms-help = { help-move-focus } | { help-toggle-click } | { help-confirm-required } | { help-cancel }
select-pms-title = Package Managers
select-pms-tab-short = Managers
select-pms-tab-long = Package Managers
cargo-label = Cargo
npm-label = NPM

# Manifest path screen
manifest-path-question = { $manifest } not found at repo root. Enter subdirectory path (or leave empty):
manifest-path-help = { help-confirm } | { help-cancel }

# Edit GitHub screen
edit-github-question = GitHub repository (owner/repo, e.g. acme/my-app, or leave empty):
edit-github-invalid-question = Invalid format. Enter owner/repo (e.g. acme/my-app), or leave empty:
edit-github-help = { help-confirm } | { help-cancel }

# Tab labels
tab-git = Git
tab-github = GitHub

# Change wizard screens
single-package-question = What type of change is this?

select-projects-question = Which projects does this change apply to?
select-projects-error = Select at least one project to continue.
select-projects-help = { help-navigate } | { help-toggle } | { help-change-level } | { help-set-all-levels } | { help-filter-all } | { help-filter-changed } | { help-filter-unchanged } | { help-confirm } | { help-cancel }

enter-message-question = Describe this change:
enter-message-help = { help-submit } | { help-newline } | { help-open-editor } | { help-back }
enter-message-title = Message
