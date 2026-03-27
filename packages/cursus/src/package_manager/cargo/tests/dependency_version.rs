use super::*;

fn make_member_info(dir: &std::path::Path, name: &str, member_path: &str) -> ProjectInfo {
	let project_dir = dir.join(member_path);
	std::fs::create_dir_all(&project_dir).unwrap();
	ProjectInfo::for_test(name, AbsolutePath::new(dir.join(member_path)).unwrap())
}

#[test]
fn update_dep_version_string_format() {
	let dir = temp_dir();
	// Create member with a string-format dependency
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n\n[dependencies]\npkg-b = \"0.2.0\"\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "0.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1, "Expected one file modified");
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(content.contains("pkg-b = \"0.3.0\""), "got: {content}");
}

#[test]
fn update_dep_version_table_format_with_features() {
	let dir = temp_dir();
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n\n[dependencies]\npkg-b = { version = \"0.2.0\", features = [\"foo\"] }\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "1.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(content.contains("version = \"1.0.0\""), "got: {content}");
	assert!(content.contains("features = [\"foo\"]"), "got: {content}");
}

#[test]
fn update_dep_version_table_format_preserves_prefix() {
	let dir = temp_dir();
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n\n[dependencies]\npkg-b = { version = \"^0.2.0\", path = \"../pkg-b\" }\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "1.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(
		content.contains("version = \"^1.0.0\""),
		"Expected prefix to be preserved, got: {content}"
	);
}

#[test]
fn update_dep_version_in_dev_dependencies() {
	let dir = temp_dir();
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\npkg-b = \"0.2.0\"\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "0.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(content.contains("pkg-b = \"0.3.0\""), "got: {content}");
}

#[test]
fn update_dep_version_in_build_dependencies() {
	let dir = temp_dir();
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n\n[build-dependencies]\npkg-b = \"0.2.0\"\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "0.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(content.contains("pkg-b = \"0.3.0\""), "got: {content}");
}

#[test]
fn update_dep_version_path_only_dep_not_modified() {
	// A table dep with only `path = "..."` and no `version` key should not be touched.
	let dir = temp_dir();
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n\n[dependencies]\npkg-b = { path = \"../pkg-b\" }\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "0.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	// Path-only dependency — no version field to update
	assert!(modified.is_empty(), "path-only dep should not be modified");
	let content = std::fs::read_to_string(member_dir.join("Cargo.toml")).unwrap();
	assert!(
		content.contains("path = \"../pkg-b\""),
		"path should be preserved"
	);
	// The pkg-b dependency entry should not have a version field injected
	assert!(
		!content.contains("pkg-b = { path = \"../pkg-b\", version"),
		"no version should be injected into path-only dep"
	);
}

#[test]
fn update_dep_version_no_member_toml_returns_empty() {
	let dir = temp_dir();
	// Create workspace root but no member directory
	write_cargo_toml(dir.path(), "[workspace]\nmembers = []\n");

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = ProjectInfo::for_test(
		"missing-pkg",
		AbsolutePath::new(dir.path().join("missing-dir")).unwrap(),
	);
	let new_version: semver::Version = "1.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert!(modified.is_empty());
}

#[test]
fn update_dep_version_preserves_prefix() {
	let dir = temp_dir();
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n\n[dependencies]\npkg-b = \"^0.2.0\"\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "1.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(content.contains("pkg-b = \"^1.0.0\""), "got: {content}");
}

#[test]
fn update_dep_version_workspace_dep_in_root() {
	let dir = temp_dir();
	// Root Cargo.toml with [workspace.dependencies]
	write_cargo_toml(
		dir.path(),
		"[workspace]\nmembers = [\"pkg-a\"]\n\n[workspace.dependencies]\npkg-b = \"0.2.0\"\n",
	);
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "0.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	assert_eq!(modified[0], dir.path().join("Cargo.toml"));
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(content.contains("pkg-b = \"0.3.0\""), "got: {content}");
}

#[test]
fn update_dep_version_skips_workspace_true_in_member() {
	let dir = temp_dir();
	// Root with workspace.dependencies
	write_cargo_toml(
		dir.path(),
		"[workspace]\nmembers = [\"pkg-a\"]\n\n[workspace.dependencies]\npkg-b = \"0.2.0\"\n",
	);
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n\n[dependencies]\npkg-b = { workspace = true }\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "0.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	// Only the root workspace Cargo.toml should be modified, not the member
	assert_eq!(modified.len(), 1);
	assert_eq!(modified[0], dir.path().join("Cargo.toml"));

	// Member Cargo.toml should be untouched
	let member_content = std::fs::read_to_string(member_dir.join("Cargo.toml")).unwrap();
	assert!(
		member_content.contains("workspace = true"),
		"got: {member_content}"
	);
}

#[test]
fn update_dep_version_not_found_returns_empty() {
	let dir = temp_dir();
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "1.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "nonexistent-dep", &new_version, false)
		.unwrap();

	assert!(modified.is_empty());
}

#[test]
fn update_dep_version_dry_run_does_not_write_file() {
	let dir = temp_dir();
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n\n[dependencies]\npkg-b = \"0.2.0\"\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "0.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, true)
		.unwrap();

	// Should still report the file as modified (would-be), but not write it
	assert_eq!(
		modified.len(),
		1,
		"dry-run should still report the modified path"
	);
	let content = std::fs::read_to_string(member_dir.join("Cargo.toml")).unwrap();
	assert!(
		content.contains("pkg-b = \"0.2.0\""),
		"dry-run should not modify the file, got: {content}"
	);
	assert!(
		!content.contains("pkg-b = \"0.3.0\""),
		"dry-run should not write new version, got: {content}"
	);
}

#[test]
fn update_dep_version_workspace_dep_dry_run_does_not_write_file() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		"[workspace]\nmembers = [\"pkg-a\"]\n\n[workspace.dependencies]\npkg-b = \"0.2.0\"\n",
	);
	let member_dir = dir.path().join("pkg-a");
	std::fs::create_dir_all(&member_dir).unwrap();
	std::fs::write(
		member_dir.join("Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = make_member_info(dir.path(), "pkg-a", "pkg-a");
	let new_version: semver::Version = "0.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, true)
		.unwrap();

	assert_eq!(
		modified.len(),
		1,
		"dry-run should still report the modified path"
	);
	let content = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		content.contains("pkg-b = \"0.2.0\""),
		"dry-run should not modify the workspace Cargo.toml, got: {content}"
	);
}

#[test]
fn semver_range_prefix_extracts_caret() {
	assert_eq!(crate::package_manager::semver_range_prefix("^1.0.0"), "^");
}

#[test]
fn semver_range_prefix_extracts_tilde() {
	assert_eq!(crate::package_manager::semver_range_prefix("~1.0.0"), "~");
}

#[test]
fn semver_range_prefix_empty_for_bare_version() {
	assert_eq!(crate::package_manager::semver_range_prefix("1.0.0"), "");
}

#[test]
fn semver_range_prefix_extracts_gte() {
	assert_eq!(crate::package_manager::semver_range_prefix(">=1.0.0"), ">=");
}
