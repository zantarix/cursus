# Changelog

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

