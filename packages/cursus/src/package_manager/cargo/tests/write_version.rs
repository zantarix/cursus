use super::*;

fn project_info(dir: &std::path::Path, name: &str, path: &str) -> ProjectInfo {
	ProjectInfo::for_test(name, AbsolutePath::new(dir.join(path)).unwrap())
}

#[test]
fn write_version_file_not_found() {
	let dir = temp_dir();
	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-crate", "");
	let version: semver::Version = "1.0.0".parse().unwrap();
	let result = adapter.write_version(&info, &version, false);
	assert!(result.is_err());
}

#[test]
fn write_version_invalid_toml() {
	let dir = temp_dir();
	write_cargo_toml(dir.path(), "not valid toml [[[");
	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-crate", "");
	let version: semver::Version = "1.0.0".parse().unwrap();
	let result = adapter.write_version(&info, &version, false);
	assert!(result.is_err());
}

#[test]
fn write_version_missing_package_section() {
	let dir = temp_dir();
	write_cargo_toml(dir.path(), "[dependencies]\n");
	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-crate", "");
	let version: semver::Version = "1.0.0".parse().unwrap();
	let result = adapter.write_version(&info, &version, false);
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No [package] table")
	);
}

#[test]
fn write_version_updates_cargo_toml() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.0.0"
edition = "2024"
"#,
	);
	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-crate", "");
	let new_version: semver::Version = "2.0.0".parse().unwrap();
	adapter.write_version(&info, &new_version, false).unwrap();

	let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(contents.contains("version = \"2.0.0\""));
	// Preserve other fields
	assert!(contents.contains("edition = \"2024\""));
}

#[test]
fn write_version_dry_run_does_not_write_file() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.0.0"
edition = "2024"
"#,
	);
	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-crate", "");
	let new_version: semver::Version = "2.0.0".parse().unwrap();
	adapter.write_version(&info, &new_version, true).unwrap();

	let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		contents.contains("version = \"1.0.0\""),
		"dry-run should not modify the file, got: {contents}"
	);
	assert!(
		!contents.contains("version = \"2.0.0\""),
		"dry-run should not write new version, got: {contents}"
	);
}

#[test]
fn write_version_roundtrip() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "0.1.0"
"#,
	);
	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-crate", "");

	let new_v: semver::Version = "0.2.0".parse().unwrap();
	adapter.write_version(&info, &new_v, false).unwrap();

	// Re-enumerate to verify the write
	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].version.to_string(), "0.2.0");
}

#[test]
fn write_version_workspace_inherited_updates_root() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
members = ["crates/*"]

[workspace.package]
version = "1.0.0"
"#,
	);

	let member = dir.path().join("crates/member");
	std::fs::create_dir_all(&member).unwrap();
	write_cargo_toml(
		&member,
		r#"
[package]
name = "member"
version.workspace = true
"#,
	);

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "member", "crates/member");

	let new_v: semver::Version = "2.0.0".parse().unwrap();
	adapter.write_version(&info, &new_v, false).unwrap();

	// Member should still have version.workspace = true
	let member_toml = std::fs::read_to_string(member.join("Cargo.toml")).unwrap();
	assert!(
		member_toml.contains("version.workspace = true"),
		"Member Cargo.toml should preserve workspace inheritance, got:\n{member_toml}"
	);

	// Workspace root should have the updated version
	let root_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		root_toml.contains("\"2.0.0\""),
		"Workspace root should have updated version, got:\n{root_toml}"
	);

	// Re-enumerate to verify the resolved version
	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].version.to_string(), "2.0.0");
}

#[test]
fn write_version_workspace_inherited_dry_run_does_not_write() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
members = ["crates/*"]

[workspace.package]
version = "1.0.0"
"#,
	);

	let member = dir.path().join("crates/member");
	std::fs::create_dir_all(&member).unwrap();
	write_cargo_toml(
		&member,
		r#"
[package]
name = "member"
version.workspace = true
"#,
	);

	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "member", "crates/member");

	let new_v: semver::Version = "2.0.0".parse().unwrap();
	adapter.write_version(&info, &new_v, true).unwrap();

	// Workspace root should still have the original version
	let root_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		root_toml.contains("\"1.0.0\""),
		"Dry-run should not update workspace root, got:\n{root_toml}"
	);
	assert!(
		!root_toml.contains("\"2.0.0\""),
		"Dry-run should not write new version, got:\n{root_toml}"
	);
}
