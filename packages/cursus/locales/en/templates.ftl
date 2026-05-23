# Config template comment strings

# [global] section comments
global-disable-dep-cycle-comment = Suppress warnings about circular dependencies between packages (two or more packages in a cycle)
global-ignore-comment = Glob patterns for packages to exclude from enumeration

# [cargo] section comments
cargo-path-comment = Subdirectory for Cargo.toml (relative to git root)

# [npm] section comments
npm-path-comment = Subdirectory for package.json (relative to git root)
npm-lock-command-comment = Custom command to update the lock file
npm-access-comment = Access level for scoped packages: "public" or "restricted" (default: "restricted")

# [git] section comments
git-tag-format-comment = Tag format: "auto", "prefixed", or "simple"
git-release-branch-prefix-comment = Prefix for release branches (branch strategy)
git-extra-files-comment = Additional files to stage before committing
git-prepare-commit-message-comment = Commit message for the prepare step
git-publish-private-packages-comment = Private packages that receive git tags and forge releases (no registry publish)

# [prepare] section comments
prepare-dependency-bump-comment = Bump level for dependents: "auto" (default), "match", "patch", "minor", or "major"

# [linked-versions] section comments
linked-versions-global-comment = Keep all packages at the same version (global linking).
linked-versions-groups-comment = Or define groups of packages that should share a version:

# [github] section comments
github-owner-auto-detect-comment = GitHub owner (auto-detected from remote if omitted)
github-repo-auto-detect-comment = GitHub repo (auto-detected from remote if omitted)
github-build-command-comment = Shell command to build release artifacts
github-pr-title-comment = Custom PR title (default: "Release updates")
github-artifacts-comment = Per-package: display name -> file path for release assets

# [gitlab] section comments
gitlab-group-auto-detect-comment = GitLab group (auto-detected from remote if omitted)
gitlab-project-auto-detect-comment = GitLab project (auto-detected from remote if omitted)
gitlab-host-comment = GitLab instance hostname (defaults to gitlab.com)
gitlab-build-command-comment = Shell command to build release artifacts
gitlab-mr-title-comment = Custom MR title (default: "Release updates")
gitlab-artifacts-comment = Per-package: display name -> file path for release assets
