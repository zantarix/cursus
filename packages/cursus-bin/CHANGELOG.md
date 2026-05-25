# Changelog

## 0.9.0 - 2026-05-25

### Features

- version sync to 0.9.0 (linked versions)

## 0.8.0 - 2026-05-24

### Features

- version sync to 0.8.0 (linked versions)

## 0.7.0 - 2026-05-23

### Features

- Adds GitLab verified release commits. When running on GitLab CI ≥18.10 with a token, the prepare commit is routed through the GitLab commits API and appears as **Verified** in the GitLab UI — no signing key custody required. Reuses the existing `[git].signed_commits` config knob (`"auto"`, `"force"`, `"off"`); `"auto"` engages whenever `GITLAB_CI=true` and a token is present. See the [GitLab CI guide](https://zantarix.github.io/cursus/guides/ci-integration/gitlab/#verified-commits) for setup. [9f13b24] via #150
- Harden GitLab forge support. Self-managed instances served over plain HTTP are now reachable end-to-end: the API client and the asset URLs surfaced in release notes both honour the scheme from `CI_API_V4_URL` or `[gitlab].host`. The release-asset host is also pinned to the same endpoint the API client used, so a stale or mirrored git remote can no longer cause asset links to point at the wrong instance. GitLab API errors run through credential redaction before being logged, matching the protection already in place for the signed-commit decorator. [9f13b24] via #150
- Cursus now rejects configurations with more than one forge section enabled at load time. Setting both `[github].enabled = true` and `[gitlab].enabled = true` in `.cursus/config.toml` produces a hard error that names the offending flags and explains the fix. Configs with a single enabled forge — or no enabled forge — continue to work as before. [9f13b24] via #150
- Add GitLab support to `cursus init`. The wizard now prompts you to pick GitHub, GitLab, or Neither as your forge, with a dedicated GitLab editor screen that auto-detects `group/project` from your git origin and surfaces a self-managed host field for non-gitlab.com instances. The generated `.cursus/config.toml` writes the chosen forge as `enabled = true` and emits the other forge as a commented-out template, so switching forges later is a hand-edit away. The config also reorders active sections to the top of the file so your live configuration is visible without scrolling as the schema grows. [9f13b24] via #150
- Add first-class GitLab support: [gitlab] config section, ReqwestGitLabClient implementing CodeForgeClient via the Kitware gitlab crate, GitLab CI token detection at the binary boundary, and a forge-neutral crate::forge module layout (relocates crate::github to crate::forge::github) [9f13b24] via #150

### Bug Fixes

- update rust crate octocrab to v0.51.0 [9f13b24] via #150
- pin rust crate gitlab to =0.1811.0 [49bc29e] via #154

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

- version sync to 0.6.0 (linked versions)

## 0.5.3 - 2026-05-02

### Bug Fixes

- Redeploy after a bad publish [78feef7]

## 0.5.2 - 2026-05-02

### Bug Fixes

- Fixes npm install on Windows where `./node_modules/.bin/cursus` would print "native binary is not installed" after a successful install. Also adds `cargo binstall cursus-bin` support for fast prebuilt-binary installs from the Rust ecosystem, with glibc Linux mapped to the musl artifact automatically. [2eae47f] via #121

## 0.5.1 - 2026-05-01

### Bug Fixes

- update rust crate octocrab to v0.49.9 [2cc681d] via #113

## 0.5.0 - 2026-04-29

### Features

- Adds verified release commits when running on GitHub Actions with a GitHub App token. The prepare commit is now routed through the GitHub Git Data API, which causes GitHub to sign it with the web-flow GPG key and display the green Verified badge. Enabled automatically via \`signed_commits = "auto"\` (the default); can be disabled with \`signed_commits = "off"\`. [6f62e1a]

## 0.4.0 - 2026-04-27

### Features

- version sync to 0.4.0 (linked versions)

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

- update rust crate octocrab to v0.49.8 [c458adb] via #95

## 0.2.1 - 2026-04-19

### Bug Fixes

- Fix a packaging issue with the OSX artifacts [e6b796a]

## 0.2.0 - 2026-04-19

### Features

- Output from the configured build command and npm lock command is now streamed live to the terminal as the command runs, rather than buffered until completion. Long-running builds no longer appear to hang with no output. [ac67ec6]

## 0.1.1 - 2026-04-19

### Bug Fixes

- Logs the filename of the created changeset after running `cursus change`. [9ce35b8]
- Fixes npm package binary download failing due to incorrect release tag format. [ad7ef84]

## 0.1.0 - 2026-04-19

### Bug Fixes

- update rust crate clap to v4.6.1 [f278f15] via #83
- update rust crate tokio to v1.52.1 [59bb964] via #85
- update rust crate tokio to v1.51.0 [da46fa8] via #66
- update rust crate tokio to v1.51.1 [c76faf5] via #73
- fix: workspace-inherited Cargo versions now included in release commit [fcfcab9]

  When crates use `version.workspace = true`, the workspace root `Cargo.toml` was modified by `write_version` but never staged for git — causing the version bump to be absent from release PRs. `PackageManagerAdapter::write_version` now returns the list of modified paths (mirroring `update_dependency_version`), and callers extend `modified_files` accordingly.
- update rust crate octocrab to v0.49.7 [3beed54] via #62

