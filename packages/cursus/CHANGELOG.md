# Changelog

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

