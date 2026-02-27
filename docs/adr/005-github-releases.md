# ADR-005: GitHub Releases Integration

## Status

Proposed

## Context

Chronicle's `chronicle publish` command (ADR-004) publishes packages to their respective registries (crates.io, npm, etc.), and `chronicle release` (ADR-003) generates changelog entries for each version. However, the tool does not currently create GitHub Releases, which many projects use to communicate new versions to users, attach release artifacts, and provide a central location for release notes.

Users want the option to automatically create GitHub Releases as part of their publish workflow, with release notes populated from the changelog content that Chronicle has already generated.

GitHub Releases are a Git hosting feature, not a package registry. They are complementary to publishing packages to registries but serve a different audience and purpose. Unlike registries, GitHub Releases are user-facing documentation and announcement mechanisms.

## Decision

Implement opt-in GitHub Releases integration as part of the `chronicle publish` workflow.

### Trigger and scope

GitHub Releases are created during `chronicle publish`, **after** each package is successfully published to its registry. If a package fails to publish, no GitHub Release is created for it.

One GitHub Release is created per package, not one per `chronicle publish` invocation. This means in a monorepo with multiple packages being published, multiple GitHub Releases may be created in a single publish run.

### Configuration

GitHub Releases are configured via a new `[github]` section in `.chronicle/config.toml`:

```toml
[github]
enabled = false        # default: opt-out, user must explicitly enable
owner = "mscharley"    # optional: GitHub repository owner (user or org)
repo = "chronicle"     # optional: GitHub repository name
```

The `[github]` section is optional. If omitted, GitHub Releases are disabled.

**Repository detection:**

The `owner` and `repo` fields are optional. When omitted, Chronicle attempts to detect them automatically by parsing the `origin` git remote URL. This supports both HTTPS and SSH remote formats:

- `https://github.com/mscharley/chronicle.git` → owner: `mscharley`, repo: `chronicle`
- `git@github.com:mscharley/chronicle.git` → owner: `mscharley`, repo: `chronicle`

Explicitly configured `owner` and `repo` values override auto-detection. This is useful when the remote URL does not match the GitHub repository (e.g., using a fork as the remote but wanting releases on the upstream repository).

If auto-detection fails and the fields are not configured, Chronicle reports an error and exits with a non-zero status code when `enabled = true`.

### Tag format

GitHub Releases are identified by Git tags. Chronicle expects tags in one of two formats:

- **Multi-package repos**: `pkg-name@version` (e.g., `chronicle-cli@0.2.0`, `@mscharley/chronicle@0.2.0`)
- **Single-package repos**: `v{version}` (e.g., `v0.2.0`)

The tag format is determined by the `[git].tag_format` configuration (see ADR-006). Chronicle uses this setting to identify which git tag corresponds to each GitHub Release.

**Tag creation:**

Chronicle creates git tags automatically when `[git].tag` is enabled (see ADR-006). When git hooks are disabled, tags must be created manually before running `chronicle publish`. If a required tag does not exist, the GitHub Release creation will fail.

### Authentication

GitHub API access requires authentication. Chronicle uses the `GITHUB_TOKEN` environment variable, following the same pattern as registry authentication in ADR-004.

Chronicle does not manage, store, or prompt for GitHub credentials. It expects the environment to be pre-configured:

- **Local development**: User sets `GITHUB_TOKEN` to a personal access token with `repo` scope
- **CI**: CI system provides `GITHUB_TOKEN` (GitHub Actions provides this automatically as `secrets.GITHUB_TOKEN`)

If `GITHUB_TOKEN` is missing or invalid, Chronicle reports the error and exits with a non-zero status code.

### Release body content

The GitHub Release body is populated from the changelog entry generated for that version. Chronicle reads the corresponding section from the package's `CHANGELOG.md` file and uses it as the release description.

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

### Not a PackageManagerAdapter

GitHub is **not** a package manager. It does not enumerate projects, read versions, or write versions. It is a publish-time hook that runs after packages have been successfully published to their actual registries.

Chronicle models GitHub Releases as a **post-publish action**, not as a package manager adapter. The `[github]` configuration section is separate from `[npm]` and `[cargo]`, and the GitHub integration does not implement the `PackageManagerAdapter` trait.

This keeps the concerns separated: package managers are about version management and enumeration, while GitHub Releases are about communication and documentation.

### Error handling

If a GitHub Release fails to be created (due to authentication, network, or API errors), Chronicle reports the failure but does **not** roll back the package publish. The package has already been successfully published to its registry, and a GitHub Release failure should not invalidate that.

Chronicle reports GitHub Release failures clearly and exits with a non-zero status code, allowing CI pipelines to detect and alert on partial failures.

### Dry-run support

When `chronicle publish --dry-run` is invoked, GitHub Releases are **not** created. The dry-run output includes a note about which GitHub Releases would have been created, but no API calls are made.

### Summary output

After publishing, Chronicle's summary output includes GitHub Release creation:

```text
Published chronicle-cli@0.2.0 to crates.io
Created GitHub Release for chronicle-cli@0.2.0
Published @mscharley/chronicle@0.2.0 to npm
Created GitHub Release for @mscharley/chronicle@0.2.0
```

Or on partial failure:

```text
Published chronicle-cli@0.2.0 to crates.io
Failed to create GitHub Release for chronicle-cli@0.2.0: missing GITHUB_TOKEN
```

## Consequences

### Positive

- GitHub Releases are opt-in and disabled by default, preventing surprise API calls or credential requirements for users who don't want this feature.
- Auto-detection of repository owner and name from the git remote reduces configuration burden. Most users can enable GitHub Releases with just `enabled = true`, without manually specifying repository details.
- Authentication is delegated to the environment, following the same pattern as registry publishing. Chronicle does not store or manage GitHub tokens.
- GitHub Releases are modelled as a post-publish action, not as a package manager. This keeps the abstraction boundaries clean and prevents GitHub from being conflated with actual package registries.
- The release body is sourced from the existing changelog, avoiding duplication of release notes content. Users write the release description once (in changesets), and it flows through to both `CHANGELOG.md` and GitHub Releases.
- GitHub Release creation failures do not block or roll back package publishing. If a package is published but its GitHub Release fails, the user can manually create the release or re-run the command after fixing the authentication issue.

### Negative

- Chronicle becomes responsible for creating GitHub Releases, coupling it to the GitHub API. This API is stable, well-documented, and versioned, but it is still an external dependency.
- When git hooks are disabled (the default without `[github].enabled`), Chronicle depends on Git tags already existing in the repository, and the user or CI is responsible for creating and pushing tags. When git hooks are enabled (see ADR-006), Chronicle creates tags automatically as part of `chronicle release`.

### Neutral

- Future enhancements could support attaching release artifacts (binaries, archives) to GitHub Releases, but this is out of scope for the initial implementation.
