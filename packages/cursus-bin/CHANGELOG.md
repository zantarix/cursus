# Changelog

## 0.1.0 - 2026-04-19

### Bug Fixes

- update rust crate clap to v4.6.1 [f278f15] via #83
- update rust crate tokio to v1.52.1 [59bb964] via #85
- update rust crate tokio to v1.51.0 [da46fa8] via #66
- update rust crate tokio to v1.51.1 [c76faf5] via #73
- fix: workspace-inherited Cargo versions now included in release commit [fcfcab9]

  When crates use `version.workspace = true`, the workspace root `Cargo.toml` was modified by `write_version` but never staged for git — causing the version bump to be absent from release PRs. `PackageManagerAdapter::write_version` now returns the list of modified paths (mirroring `update_dependency_version`), and callers extend `modified_files` accordingly.
- update rust crate octocrab to v0.49.7 [3beed54] via #62

