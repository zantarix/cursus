# Changelog

## 0.9.1 - 2026-05-27

### Bug Fixes

- update rust crate serde-saphyr to 0.0.27 [7b24e20] via #162

## 0.9.0 - 2026-05-25

### Features

- version sync to 0.9.0 (linked versions)

## 0.8.0 - 2026-05-24

### Features

- Adds GitLab merge request references to generated changelog entries. When a changeset's commit came from a GitLab merge request, the changelog now links it using GitLab syntax (`!123+`, including cross-project `group/proj!123+` references) instead of leaving it unlinked. GitHub pull request references are detected and rendered as before. [dcb035c] via #158

### Bug Fixes

- `prepare` now fails immediately with a clear error when run on a detached HEAD under the branch strategy, instead of creating a `cursus-release/detached` branch and failing later. Check out a branch or use the push strategy. [dcb035c] via #158
- Fixes GitLab releases failing in CI when the runner token cannot push tags. Release tags are now created through the forge API (GitLab Tags API / GitHub Git Data API) when verified commits are enabled, so the git remote no longer needs code-push permission. Tags remain annotated but unsigned. [dcb035c] via #158

## 0.7.0 - 2026-05-23

### Features

- Adds GitLab verified release commits. When running on GitLab CI ≥18.10 with a token, the prepare commit is routed through the GitLab commits API and appears as **Verified** in the GitLab UI — no signing key custody required. Reuses the existing `[git].signed_commits` config knob (`"auto"`, `"force"`, `"off"`); `"auto"` engages whenever `GITLAB_CI=true` and a token is present. See the [GitLab CI guide](https://zantarix.github.io/cursus/guides/ci-integration/gitlab/#verified-commits) for setup. [9f13b24] via #150
- Harden GitLab forge support. Self-managed instances served over plain HTTP are now reachable end-to-end: the API client and the asset URLs surfaced in release notes both honour the scheme from `CI_API_V4_URL` or `[gitlab].host`. The release-asset host is also pinned to the same endpoint the API client used, so a stale or mirrored git remote can no longer cause asset links to point at the wrong instance. GitLab API errors run through credential redaction before being logged, matching the protection already in place for the signed-commit decorator. [9f13b24] via #150
- Cursus now rejects configurations with more than one forge section enabled at load time. Setting both `[github].enabled = true` and `[gitlab].enabled = true` in `.cursus/config.toml` produces a hard error that names the offending flags and explains the fix. Configs with a single enabled forge — or no enabled forge — continue to work as before. [9f13b24] via #150
- Add GitLab support to `cursus init`. The wizard now prompts you to pick GitHub, GitLab, or Neither as your forge, with a dedicated GitLab editor screen that auto-detects `group/project` from your git origin and surfaces a self-managed host field for non-gitlab.com instances. The generated `.cursus/config.toml` writes the chosen forge as `enabled = true` and emits the other forge as a commented-out template, so switching forges later is a hand-edit away. The config also reorders active sections to the top of the file so your live configuration is visible without scrolling as the schema grows. [9f13b24] via #150
- Add first-class GitLab support: [gitlab] config section, ReqwestGitLabClient implementing CodeForgeClient via the Kitware gitlab crate, GitLab CI token detection at the binary boundary, and a forge-neutral crate::forge module layout (relocates crate::github to crate::forge::github) [9f13b24] via #150

### Bug Fixes

- update rust crate octocrab to v0.51.0 [9f13b24] via #150

## 0.6.3 - 2026-05-12

### Bug Fixes

- update rust crate tokio to v1.52.3 [5d22372] via #133

## 0.6.2 - 2026-05-07

### Bug Fixes

- Revert octocrab to 0.49 to revert upstream issues [fc2bbdf]

## 0.6.1 - 2026-05-07

### Bug Fixes

- Updates octocrab to 0.50.0, tokio to 1.52.2, and serde-saphyr to 0.0.26. [d9b875c]

## 0.6.0 - 2026-05-02

### Features

- Fixes a gap where re-running `cursus publish` after a partial failure would not create missing git tags or GitHub Releases for packages already published to a registry. All three publish stages (registry, git tag, GitHub Release) are now idempotent: re-running safely completes any stage that did not finish in a prior run. If a draft GitHub Release already exists for a tag, cursus reports a clear error and exits non-zero instead of silently failing. [e832dde] via #125

## 0.5.3 - 2026-05-02

### Bug Fixes

- Redeploy after a bad publish [78feef7]

## 0.5.2 - 2026-05-02

### Bug Fixes

- version sync to 0.5.2 (linked versions)

## 0.5.1 - 2026-05-01

### Bug Fixes

- Fixes token leakage where GitHub access tokens, registry credentials, and other URL-embedded secrets could appear in error messages produced by failed git operations or package publishes. [131dc96] via #112
- Rejects changeset files larger than 64 KiB and config.toml larger than 256 KiB to prevent out-of-memory conditions when processing maliciously oversized inputs. [131dc96] via #112
- Fixes security vulnerabilities in the npm postinstall download script: redirect targets are now validated against an allowlist of known GitHub domains, response sizes are bounded to prevent memory exhaustion, and GitHub API rate-limit errors include actionable retry guidance. [131dc96] via #112
- Rejects package names and git ref names that start with '-' or contain ASCII control characters, preventing argv-smuggling attacks where a malicious workspace member name could be interpreted as a flag by the git binary. [131dc96] via #112

## 0.5.0 - 2026-04-29

### Features

- Adds verified release commits when running on GitHub Actions with a GitHub App token. The prepare commit is now routed through the GitHub Git Data API, which causes GitHub to sign it with the web-flow GPG key and display the green Verified badge. Enabled automatically via \`signed_commits = "auto"\` (the default); can be disabled with \`signed_commits = "off"\`. [6f62e1a]

## 0.4.0 - 2026-04-27

### Features

- Fixes `cursus change` incorrectly attributing file changes inside an ignored sub-project to its releasable parent. Adds `match_files_to_projects_in_scope`, `Config::load_all_projects`, and `Config::load_projects_partitioned` to the public API. [3ca0e57]

### Bug Fixes

- Fixes `cursus change --no-interactive` to select only git-changed projects by default, falling back to all projects when none are detected. Explicit `--project` flags are unaffected. [e1620a2]
- Fixes `cursus change --change-type <type>` (without `--project`) incorrectly selecting all projects when git-changed projects are available. It now selects only changed projects, consistent with the interactive TUI pre-selection. [e1620a2]

## 0.3.2 - 2026-04-27

### Bug Fixes

- Adds `Project::is_releasable_under(&Config)`, `Project::is_prepared_for_release(&dyn Filesystem)`, and `package_manager::matching::match_files_to_projects` as public API. [3082f8a]

## 0.3.1 - 2026-04-27

### Bug Fixes

- Fixes attestation verification failing on Linux during `npm install @zantarix/cursus`. [698d77a]

## 0.3.0 - 2026-04-27

### Features

- Verifies the Sigstore build-provenance attestation of the downloaded binary during `npm install @zantarix/cursus`, confirming it was produced by the official release workflow before writing it to disk. [c453746]

## 0.2.3 - 2026-04-27

### Bug Fixes

- Fixes Windows release binaries, which were failing to build due to a linker incompatibility in the cross-compilation toolchain. Windows binaries are now built natively using the MSVC toolchain with a statically linked CRT, producing self-contained executables with no runtime DLL dependencies. [6de8fe6]

## 0.2.2 - 2026-04-25

### Bug Fixes

- update rust crate serde-saphyr to 0.0.24 [fd397c3] via #96
- update rust crate fluent-templates to 0.14.0 [5c7fa20] via #99
- update rust crate serde-saphyr to 0.0.25 [f899a27] via #102

## 0.2.1 - 2026-04-19

### Bug Fixes

- version sync to 0.2.1 (linked versions)

## 0.2.0 - 2026-04-19

### Features

- Output from the configured build command and npm lock command is now streamed live to the terminal as the command runs, rather than buffered until completion. Long-running builds no longer appear to hang with no output. [ac67ec6]

## 0.1.1 - 2026-04-19

### Bug Fixes

- Logs the filename of the created changeset after running `cursus change`. [9ce35b8]
- Fixes npm package binary download failing due to incorrect release tag format. [ad7ef84]
- Expose crates.io trusted publishing as a viable option for publishing crates [9a13b99] via #87

## 0.1.0 - 2026-04-19

### Features

- Initial release [f923074]

### Bug Fixes

- update rust crate tokio to v1.52.1 [59bb964] via #85
- update rust crate serde-saphyr to 0.0.23 [bd769c8] via #63
- update rust crate tokio to v1.51.0 [da46fa8] via #66
- update rust crate tokio to v1.51.1 [c76faf5] via #73
- fix: workspace-inherited Cargo versions now included in release commit [fcfcab9]

  When crates use `version.workspace = true`, the workspace root `Cargo.toml` was modified by `write_version` but never staged for git — causing the version bump to be absent from release PRs. `PackageManagerAdapter::write_version` now returns the list of modified paths (mirroring `update_dependency_version`), and callers extend `modified_files` accordingly.
- update rust crate petname to v3 [8cc0d12] via #80
- update rust crate ratatui-textarea to 0.9.0 [323087c] via #74

