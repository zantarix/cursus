# Changelog

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

