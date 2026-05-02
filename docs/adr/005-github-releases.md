# ADR-005: GitHub Releases Integration

## Status

Accepted (2026-03-09)

## Context

Cursus's `cursus publish` command ([ADR-004](004-publish-command.md)) publishes packages to their respective registries (crates.io, npm, etc.), and `cursus release` ([ADR-003](003-release-command.md)) generates changelog entries for each version. However, the tool does not currently create GitHub Releases, which many projects use to communicate new versions to users, attach release artifacts, and provide a central location for release notes.

Users want the option to automatically create GitHub Releases as part of their publish workflow, with release notes populated from the changelog content that Cursus has already generated.

GitHub Releases are a Git hosting feature, not a package registry. They are complementary to publishing packages to registries but serve a different audience and purpose. Unlike registries, GitHub Releases are user-facing documentation and announcement mechanisms.

## Decision

Implement opt-in GitHub Releases integration as part of the `cursus publish` workflow.

### Trigger and scope

GitHub Releases are created during `cursus publish`, **after** each package is successfully published to its registry. If a package fails to publish, no GitHub Release is created for it.

One GitHub Release is created per package, not one per `cursus publish` invocation. This means in a monorepo with multiple packages being published, multiple GitHub Releases may be created in a single publish run.

### Configuration

GitHub Releases are configured via a new `[github]` section in `.cursus/config.toml`:

```toml
[github]
enabled = false        # default: opt-out, user must explicitly enable
owner = "mscharley"    # optional: GitHub repository owner (user or org)
repo = "cursus"     # optional: GitHub repository name
build_command = ""     # optional: shell command to run before creating the release

[github.artifacts]     # optional: map of release filename to local file path
# "cursus-x86_64-linux" = "target/x86_64-unknown-linux-musl/release/cursus"
```

The `[github]` section is optional. If omitted, GitHub Releases are disabled.

**Repository detection:**

The `owner` and `repo` fields are optional. When omitted, Cursus attempts to detect them automatically by parsing the `origin` git remote URL. This supports both HTTPS and SSH remote formats:

- `https://github.com/mscharley/cursus.git` → owner: `mscharley`, repo: `cursus`
- `git@github.com:mscharley/cursus.git` → owner: `mscharley`, repo: `cursus`

Explicitly configured `owner` and `repo` values override auto-detection. This is useful when the remote URL does not match the GitHub repository (e.g., using a fork as the remote but wanting releases on the upstream repository).

If auto-detection fails and the fields are not configured, Cursus reports an error and exits with a non-zero status code when `enabled = true`.

### Tag format

GitHub Releases are identified by Git tags. Cursus expects tags in one of two formats:

- **Multi-package repos**: `pkg-name@version` (e.g., `cursus-cli@0.2.0`, `@mscharley/cursus@0.2.0`)
- **Single-package repos**: `v{version}` (e.g., `v0.2.0`)

The tag format is determined by the `[git].tag_format` configuration (see [ADR-006](006-git-lifecycle-hooks.md)). Cursus uses this setting to identify which git tag corresponds to each GitHub Release.

**Tag creation:**

Cursus creates git tags automatically when `[git].tag` is enabled (see [ADR-006](006-git-lifecycle-hooks.md)). When git hooks are disabled, tags must be created manually before running `cursus publish`. If a required tag does not exist, the GitHub Release creation will fail.

### Authentication

GitHub API access requires authentication. Cursus uses the `GITHUB_TOKEN` environment variable, following the same pattern as registry authentication in [ADR-004](004-publish-command.md).

Cursus does not manage, store, or prompt for GitHub credentials. It expects the environment to be pre-configured:

- **Local development**: User sets `GITHUB_TOKEN` to a personal access token with `repo` scope
- **CI**: CI system provides `GITHUB_TOKEN` (GitHub Actions provides this automatically as `secrets.GITHUB_TOKEN`)

If `GITHUB_TOKEN` is missing or invalid, Cursus reports the error and exits with a non-zero status code.

### Release body content

The GitHub Release body is populated from the changelog entry generated for that version. Cursus reads the corresponding section from the package's `CHANGELOG.md` file and uses it as the release description.

For example, if `CHANGELOG.md` contains:

```markdown
## 1.2.0

### Features

- Added foo feature
- Improved bar handling

### Bug Fixes

- Fixed baz edge case
```

The GitHub Release for version 1.2.0 will have the body:

```markdown
### Features

- Added foo feature
- Improved bar handling

### Bug Fixes

- Fixed baz edge case
```

If the changelog entry cannot be found or is empty, the GitHub Release is created with an empty body.

### Build command and artifact attachment

GitHub Releases can optionally have file artifacts attached (binaries, archives, checksums, etc.). Cursus supports this via two configuration fields: `build_command` and `artifacts`.

**`build_command`** is an optional shell command that Cursus executes before creating the GitHub Release. It runs after the version bump has already been applied by `cursus release` (and after registry publishing), but before the GitHub Release is created. This allows users to produce build artifacts that reference the correct version.

The command is executed via the system shell (`sh -c` on Unix) with the working directory set to the repository root. If the command exits with a non-zero status, Cursus reports the failure and skips GitHub Release creation for that package, but does not roll back the registry publish.

The build command runs once per `cursus publish` invocation, not once per package. It is intended for repository-level build steps (e.g., cross-compiling binaries, creating tarballs). If a user needs per-package build logic, they should handle that within the build command itself (e.g., a Makefile or script that builds all necessary targets).

**`artifacts`** is a TOML map where each key is the filename that will appear on the GitHub Release and each value is the path to the file on disk, relative to the repository root. This gives users explicit control over the download names that consumers see, decoupling them from the build system's directory structure.

Example configuration for a project that cross-compiles static binaries:

```toml
[github]
enabled = true
build_command = "cargo make release"

[github.artifacts]
"cursus-x86_64-linux" = "target/x86_64-unknown-linux-musl/release/cursus"
"cursus-aarch64-linux" = "target/aarch64-unknown-linux-musl/release/cursus"
"cursus-aarch64-darwin" = "target/aarch64-apple-darwin/release/cursus"
```

Example with a build script that produces a tarball and checksum:

```toml
[github]
enabled = true
build_command = "./scripts/build-release.sh"

[github.artifacts]
"myapp-0.2.0.tar.gz" = "dist/myapp-0.2.0.tar.gz"
"SHA256SUMS.txt" = "dist/SHA256SUMS.txt"
```

When `artifacts` is omitted or empty (the default), no files are attached to the GitHub Release. When `build_command` is empty (the default), no build step is executed and only pre-existing files referenced by `artifacts` entries are attached.

If a path listed in `artifacts` does not exist at upload time, Cursus reports an error for that artifact but continues uploading the remaining artifacts. The GitHub Release is still created.

In a monorepo with multiple packages, artifacts are attached to every GitHub Release created during the publish run. If different packages need different artifacts, the user should manage this through their build command to ensure the correct files are present.

### Not a PackageManagerAdapter

GitHub is **not** a package manager. It does not enumerate projects, read versions, or write versions. It is a publish-time hook that runs after packages have been successfully published to their actual registries.

Cursus models GitHub Releases as a **post-publish action**, not as a package manager adapter. The `[github]` configuration section is separate from `[npm]` and `[cargo]`, and the GitHub integration does not implement the `PackageManagerAdapter` trait.

This keeps the concerns separated: package managers are about version management and enumeration, while GitHub Releases are about communication and documentation.

### Error handling

If a GitHub Release fails to be created (due to authentication, network, or API errors), Cursus reports the failure but does **not** roll back the package publish. The package has already been successfully published to its registry, and a GitHub Release failure should not invalidate that.

Cursus reports GitHub Release failures clearly and exits with a non-zero status code, allowing CI pipelines to detect and alert on partial failures.

### Dry-run support

When `cursus publish --dry-run` is invoked, GitHub Releases are **not** created. The `build_command` is **not** executed, and no artifacts are uploaded. The dry-run output includes a note about which GitHub Releases would have been created and which artifacts are configured, but no API calls or subprocess invocations are made. This is consistent with the dry-run safety guarantee established in [ADR-008](008-dry-run-local-only-guarantee.md).

### Summary output

After publishing, Cursus's summary output includes GitHub Release creation:

```text
Published cursus-cli@0.2.0 to crates.io
Running build command: cargo make release
Created GitHub Release for cursus-cli@0.2.0
  Attached: cursus-x86_64-linux
  Attached: cursus-aarch64-linux
Published @mscharley/cursus@0.2.0 to npm
Created GitHub Release for @mscharley/cursus@0.2.0
  Attached: cursus-x86_64-linux
  Attached: cursus-aarch64-linux
```

Or on partial failure:

```text
Published cursus-cli@0.2.0 to crates.io
Failed to create GitHub Release for cursus-cli@0.2.0: missing GITHUB_TOKEN
```

## Consequences

### Positive

- GitHub Releases are opt-in and disabled by default, preventing surprise API calls or credential requirements for users who don't want this feature.
- Auto-detection of repository owner and name from the git remote reduces configuration burden. Most users can enable GitHub Releases with just `enabled = true`, without manually specifying repository details.
- Authentication is delegated to the environment, following the same pattern as registry publishing. Cursus does not store or manage GitHub tokens.
- GitHub Releases are modelled as a post-publish action, not as a package manager. This keeps the abstraction boundaries clean and prevents GitHub from being conflated with actual package registries.
- The release body is sourced from the existing changelog, avoiding duplication of release notes content. Users write the release description once (in changesets), and it flows through to both `CHANGELOG.md` and GitHub Releases.
- GitHub Release creation failures do not block or roll back package publishing. If a package is published but its GitHub Release fails, the user can manually create the release or re-run the command after fixing the authentication issue.
- The `build_command` option allows users to produce versioned artifacts as part of the publish workflow without requiring a separate CI step or manual coordination. The build runs after the version bump, so artifacts can reference the correct version.
- Artifact attachment uses an explicit map of release filename to disk path, giving users full control over the download names that appear on the GitHub Release. This decouples user-facing artifact names from the build system's directory layout.

### Negative

- Cursus becomes responsible for creating GitHub Releases, coupling it to the GitHub API. This API is stable, well-documented, and versioned, but it is still an external dependency.
- When git hooks are disabled (the default without `[github].enabled`), Cursus depends on Git tags already existing in the repository, and the user or CI is responsible for creating and pushing tags. When git hooks are enabled (see [ADR-006](006-git-lifecycle-hooks.md)), Cursus creates tags automatically as part of `cursus release`.
- The `build_command` is executed as a subprocess via the system shell, which introduces a dependency on the build environment having the correct toolchain installed. Build failures are reported but cannot be retried without re-running the entire publish workflow.
- In monorepos, artifacts are attached to every GitHub Release in the publish run. There is no per-package artifact configuration. Users with per-package artifact needs must manage this through their build scripts.
- Each artifact requires an explicit map entry. Users who produce many artifacts (e.g., one per platform per package) must list each one individually, which can be verbose compared to glob-based approaches.

### Neutral

- The build command runs once per publish invocation, not once per package. This is a deliberate simplification; users who need per-package build logic must handle it within their build command.

## Errata

**2026-03-01**: [ADR-011](011-command-execution-strategy.md) (Command Execution Strategy) establishes a project-wide standard for how all user-configurable command fields are executed. The `build_command` execution semantics described in this ADR -- "executed via the system shell (`sh -c` on Unix) with the working directory set to the repository root" -- are consistent with [ADR-011](011-command-execution-strategy.md)'s conventions and remain correct. [ADR-011](011-command-execution-strategy.md) is now the authoritative reference for command execution details including shell choice (`/bin/sh`), working directory, dry-run interaction, and error handling conventions. See [ADR-011](011-command-execution-strategy.md) for the full standard that applies to `build_command` and all other configurable command fields.

**2026-03-09**: [ADR-015](015-ci-managed-release-workflow.md) moves tag creation from the release step to the publish step. The "Tag creation" section of this ADR states "When `[git].enabled = true` and `run_until` is `"tag"` or `"push"`, Cursus creates git tags during `cursus release`." This is no longer accurate: tags are always created during `cursus publish` (when `[git].enabled = true`), never during `cursus release`. The `run_until` field has been replaced by `strategy`. Additionally, the statement "When git hooks are disabled, tags must already exist before running `cursus publish`" is no longer accurate: when `[git].enabled = true`, `publish` creates tags itself. See [ADR-015](015-ci-managed-release-workflow.md) for the revised tag creation workflow.

**2026-03-09**: [ADR-016](016-rename-release-to-prepare.md) renames the `cursus release` subcommand to `cursus prepare`. References to `cursus release` in this ADR now refer to `cursus prepare`. The behavior is unchanged. See [ADR-016](016-rename-release-to-prepare.md) for details.

**2026-04-06**: [ADR-044](044-per-package-github-release-artifacts.md) restructures the `[github.artifacts]` configuration from a flat `BTreeMap<String, String>` to per-package sections (`[github.artifacts.<package-name>]`). The artifact configuration format and upload semantics described in this ADR are superseded by the per-package model. The `build_command` behavior is unchanged. See [ADR-044](044-per-package-github-release-artifacts.md) for the new artifact configuration format.

**2026-05-02**: [ADR-055](055-end-to-end-idempotent-publish-recovery.md) amends the "Error handling" section. GitHub Release creation is no longer unconditional: a pre-check via a new `CodeForgeClient::find_release_by_tag` method runs first. Existing published releases are treated as success and skipped; existing draft releases produce an actionable error and require manual intervention before re-running. The "no roll-back of registry publish on GitHub failure" guarantee is unchanged. See [ADR-055](055-end-to-end-idempotent-publish-recovery.md) for the recovery decision tree.
