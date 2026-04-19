+++
cursus = "patch"
cursus-bin = "patch"
+++

fix: workspace-inherited Cargo versions now included in release commit

When crates use `version.workspace = true`, the workspace root `Cargo.toml` was modified by `write_version` but never staged for git — causing the version bump to be absent from release PRs. `PackageManagerAdapter::write_version` now returns the list of modified paths (mirroring `update_dependency_version`), and callers extend `modified_files` accordingly.
