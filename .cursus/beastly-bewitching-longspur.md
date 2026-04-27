+++
cursus = "patch"
+++

Fixes `cursus change --no-interactive` to select only git-changed projects by default, falling back to all projects when none are detected. Explicit `--project` flags are unaffected.
