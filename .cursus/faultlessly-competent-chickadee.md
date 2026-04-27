+++
cursus = "patch"
+++

Fixes `cursus change --change-type <type>` (without `--project`) incorrectly selecting all projects when git-changed projects are available. It now selects only changed projects, consistent with the interactive TUI pre-selection.
