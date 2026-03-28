use super::*;

#[test]
fn enumerate_returns_empty_when_no_cargo_toml() {
	let dir = temp_dir();
	let projects = enumerate(dir.path()).unwrap();
	assert!(projects.is_empty());
}

#[test]
fn enumerate_single_crate() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "0.1.0"
"#,
	);

	let projects = enumerate(dir.path()).unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name, "my-crate");
	assert_eq!(projects[0].path.as_path(), dir.path());
}

#[test]
fn enumerate_virtual_manifest_no_members() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
"#,
	);

	let projects = enumerate(dir.path()).unwrap();
	assert!(projects.is_empty());
}

#[test]
fn enumerate_workspace_members() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
members = ["crates/*"]
"#,
	);

	let crate_a = dir.path().join("crates/crate-a");
	let crate_b = dir.path().join("crates/crate-b");
	std::fs::create_dir_all(&crate_a).unwrap();
	std::fs::create_dir_all(&crate_b).unwrap();
	write_cargo_toml(
		&crate_a,
		r#"
[package]
name = "crate-a"
version = "0.1.0"
"#,
	);
	write_cargo_toml(
		&crate_b,
		r#"
[package]
name = "crate-b"
version = "0.1.0"
"#,
	);

	let projects = enumerate(dir.path()).unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "crate-a");
	assert_eq!(
		projects[0].path.as_path(),
		dir.path().join("crates/crate-a")
	);
	assert_eq!(projects[1].name, "crate-b");
	assert_eq!(
		projects[1].path.as_path(),
		dir.path().join("crates/crate-b")
	);
}

#[test]
fn enumerate_workspace_with_root_package() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "root-crate"
version = "0.1.0"

[workspace]
members = ["crates/*"]
"#,
	);

	let member = dir.path().join("crates/member");
	std::fs::create_dir_all(&member).unwrap();
	write_cargo_toml(
		&member,
		r#"
[package]
name = "member-crate"
version = "0.1.0"
"#,
	);

	let projects = enumerate(dir.path()).unwrap();

	assert_eq!(projects.len(), 2);
	// Root comes first (shorter absolute path sorts first)
	assert_eq!(projects[0].name, "root-crate");
	assert_eq!(projects[0].path.as_path(), dir.path());
	assert_eq!(projects[1].name, "member-crate");
	assert_eq!(projects[1].path.as_path(), dir.path().join("crates/member"));
}

#[test]
fn enumerate_multiple_member_patterns() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
members = ["crates/*", "tools/*"]
"#,
	);

	let crate_dir = dir.path().join("crates/lib");
	let tool_dir = dir.path().join("tools/cli");
	std::fs::create_dir_all(&crate_dir).unwrap();
	std::fs::create_dir_all(&tool_dir).unwrap();
	write_cargo_toml(
		&crate_dir,
		r#"
[package]
name = "lib"
version = "0.1.0"
"#,
	);
	write_cargo_toml(
		&tool_dir,
		r#"
[package]
name = "cli"
version = "0.1.0"
"#,
	);

	let projects = enumerate(dir.path()).unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "lib");
	assert_eq!(projects[0].path.as_path(), dir.path().join("crates/lib"));
	assert_eq!(projects[1].name, "cli");
	assert_eq!(projects[1].path.as_path(), dir.path().join("tools/cli"));
}

#[test]
fn enumerate_skips_directories_without_cargo_toml() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
members = ["crates/*"]
"#,
	);

	let valid = dir.path().join("crates/valid");
	let no_cargo = dir.path().join("crates/no-cargo-toml");
	std::fs::create_dir_all(&valid).unwrap();
	std::fs::create_dir_all(&no_cargo).unwrap();
	write_cargo_toml(
		&valid,
		r#"
[package]
name = "valid"
version = "0.1.0"
"#,
	);

	let projects = enumerate(dir.path()).unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name, "valid");
}

#[test]
fn enumerate_skips_virtual_manifest_members() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
members = ["crates/*"]
"#,
	);

	let real_crate = dir.path().join("crates/real");
	let virtual_manifest = dir.path().join("crates/virtual");
	std::fs::create_dir_all(&real_crate).unwrap();
	std::fs::create_dir_all(&virtual_manifest).unwrap();
	write_cargo_toml(
		&real_crate,
		r#"
[package]
name = "real"
version = "0.1.0"
"#,
	);
	// Virtual manifest has [workspace] but no [package]
	write_cargo_toml(
		&virtual_manifest,
		r#"
[workspace]
members = []
"#,
	);

	let projects = enumerate(dir.path()).unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name, "real");
}

#[test]
fn enumerate_fails_on_invalid_cargo_toml() {
	let dir = temp_dir();
	write_cargo_toml(dir.path(), "not valid toml [[[");

	let result = enumerate(dir.path());

	assert!(result.is_err());
}

#[test]
fn enumerate_fails_on_invalid_member_cargo_toml() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
members = ["crates/*"]
"#,
	);

	let bad = dir.path().join("crates/bad");
	std::fs::create_dir_all(&bad).unwrap();
	write_cargo_toml(&bad, "invalid toml");

	let result = enumerate(dir.path());

	assert!(result.is_err());
}

#[test]
fn new_creates_adapter() {
	let dir = temp_dir();
	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	let _ = adapter.enumerate_projects();
}

#[test]
fn enumerate_single_crate_in_subfolder() {
	let dir = temp_dir();
	let subfolder = dir.path().join("backend");
	std::fs::create_dir_all(&subfolder).unwrap();
	write_cargo_toml(
		&subfolder,
		r#"
[package]
name = "my-crate"
version = "0.1.0"
"#,
	);

	let projects = enumerate_with_path(dir.path(), "backend").unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name, "my-crate");
	assert_eq!(projects[0].path.as_path(), dir.path().join("backend"));
}

#[test]
fn enumerate_workspace_in_subfolder() {
	let dir = temp_dir();
	let subfolder = dir.path().join("backend");
	std::fs::create_dir_all(&subfolder).unwrap();
	write_cargo_toml(
		&subfolder,
		r#"
[workspace]
members = ["crates/*"]
"#,
	);

	let crate_a = subfolder.join("crates/crate-a");
	let crate_b = subfolder.join("crates/crate-b");
	std::fs::create_dir_all(&crate_a).unwrap();
	std::fs::create_dir_all(&crate_b).unwrap();
	write_cargo_toml(
		&crate_a,
		r#"
[package]
name = "crate-a"
version = "0.1.0"
"#,
	);
	write_cargo_toml(
		&crate_b,
		r#"
[package]
name = "crate-b"
version = "0.1.0"
"#,
	);

	let projects = enumerate_with_path(dir.path(), "backend").unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "crate-a");
	assert_eq!(
		projects[0].path.as_path(),
		dir.path().join("backend/crates/crate-a")
	);
	assert_eq!(projects[1].name, "crate-b");
	assert_eq!(
		projects[1].path.as_path(),
		dir.path().join("backend/crates/crate-b")
	);
}

#[test]
fn enumerate_errors_when_subfolder_missing() {
	let dir = temp_dir();
	let result = enumerate_with_path(dir.path(), "nonexistent");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("does not exist or escapes repository root"),
		"got: {msg}"
	);
}

#[test]
fn enumerate_includes_version() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.2.3"
"#,
	);
	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].version.to_string(), "1.2.3");
}

#[test]
fn enumerate_missing_version_fails() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
"#,
	);
	let result = enumerate(dir.path());
	assert!(result.is_err());
}

#[test]
fn enumerate_invalid_semver_fails() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "not-a-version"
"#,
	);
	let result = enumerate(dir.path());
	assert!(result.is_err());
}

#[test]
fn enumerate_includes_publishable_status() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.0.0"
"#,
	);
	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert!(
		projects[0].publishable,
		"Crate without publish field should be publishable"
	);
}

#[test]
fn enumerate_publishable_false_for_publish_false() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.0.0"
publish = false
"#,
	);
	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert!(
		!projects[0].publishable,
		"Crate with publish = false should not be publishable"
	);
}

#[test]
fn enumerate_publishable_false_for_empty_array() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.0.0"
publish = []
"#,
	);
	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert!(
		!projects[0].publishable,
		"Crate with publish = [] should not be publishable"
	);
}

#[test]
fn enumerate_publishable_true_for_publish_true() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.0.0"
publish = true
"#,
	);
	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert!(
		projects[0].publishable,
		"Crate with publish = true should be publishable"
	);
}

#[test]
fn enumerate_publishable_true_for_registry_array() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.0.0"
publish = ["crates-io"]
"#,
	);
	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert!(
		projects[0].publishable,
		"Crate with publish = [\"crates-io\"] should be publishable"
	);
}

#[test]
fn enumerate_includes_dependency_names() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.0.0"

[dependencies]
serde = "1.0"
tokio = "1.0"

[dev-dependencies]
tempfile = "3.0"
"#,
	);
	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].dependency_names.len(), 3);
	assert!(projects[0].dependency_names.contains(&"serde".to_string()));
	assert!(projects[0].dependency_names.contains(&"tokio".to_string()));
	assert!(
		projects[0]
			.dependency_names
			.contains(&"tempfile".to_string())
	);
}

#[test]
fn enumerate_workspace_member_inherits_version() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
members = ["crates/*"]

[workspace.package]
version = "3.2.1"
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

	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].version.to_string(), "3.2.1");
	assert!(
		projects[0].workspace_version,
		"should flag workspace inheritance"
	);
}

#[test]
fn enumerate_workspace_true_without_workspace_version_fails() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[workspace]
members = ["crates/*"]
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

	let result = enumerate(dir.path());
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("version.workspace = true"),
		"Error should mention version.workspace = true, got: {msg}"
	);
}

#[test]
fn enumerate_root_package_inherits_workspace_version() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "root-crate"
version.workspace = true

[workspace]

[workspace.package]
version = "2.0.0"
"#,
	);

	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name, "root-crate");
	assert_eq!(projects[0].version.to_string(), "2.0.0");
	assert!(
		projects[0].workspace_version,
		"should flag workspace inheritance"
	);
}

#[test]
fn enumerate_literal_version_not_flagged_as_workspace() {
	let dir = temp_dir();
	write_cargo_toml(
		dir.path(),
		r#"
[package]
name = "my-crate"
version = "1.0.0"
"#,
	);

	let projects = enumerate(dir.path()).unwrap();
	assert_eq!(projects.len(), 1);
	assert!(
		!projects[0].workspace_version,
		"literal version should not be flagged as workspace"
	);
}
