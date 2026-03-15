use super::*;
use semver::Version;

fn project_info(dir: &std::path::Path, name: &str, path: &str) -> ProjectInfo {
	ProjectInfo::for_test(name, AbsolutePath::new(dir.join(path)).unwrap())
}

fn make_project_with_deps(dir: &std::path::Path, deps_json: &str) -> ProjectInfo {
	let content =
		format!(r#"{{"name": "pkg-a", "version": "0.1.0", "dependencies": {deps_json}}}"#);
	write_package_json(dir, &content);
	project_info(dir, "pkg-a", "")
}

#[test]
fn update_dep_version_missing_manifest_returns_empty() {
	let dir = temp_dir();
	// No package.json written
	let info = project_info(dir.path(), "pkg-a", "");
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "2.0.0".parse().unwrap();
	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();
	assert!(modified.is_empty());
}

#[test]
fn update_dep_version_invalid_json_returns_error() {
	let dir = temp_dir();
	write_package_json(dir.path(), "not valid json {{{{");
	let info = project_info(dir.path(), "pkg-a", "");
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "2.0.0".parse().unwrap();
	let result = adapter.update_dependency_version(&info, "pkg-b", &new_version, false);
	assert!(result.is_err());
}

#[test]
fn update_dep_version_preserves_caret() {
	let dir = temp_dir();
	let info = make_project_with_deps(dir.path(), r#"{"pkg-b": "^1.0.0"}"#);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "2.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(content.contains("\"pkg-b\": \"^2.0.0\""), "got: {content}");
}

#[test]
fn update_dep_version_preserves_tilde() {
	let dir = temp_dir();
	let info = make_project_with_deps(dir.path(), r#"{"pkg-b": "~1.2.0"}"#);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "1.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(content.contains("\"pkg-b\": \"~1.3.0\""), "got: {content}");
}

#[test]
fn update_dep_version_exact_no_prefix() {
	let dir = temp_dir();
	let info = make_project_with_deps(dir.path(), r#"{"pkg-b": "1.0.0"}"#);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "2.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let content = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(content.contains("\"pkg-b\": \"2.0.0\""), "got: {content}");
}

#[test]
fn update_dep_version_workspace_protocol_prints_warning() {
	let dir = temp_dir();
	let info = make_project_with_deps(dir.path(), r#"{"pkg-b": "workspace:*"}"#);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "2.0.0".parse().unwrap();

	// Should not error, should return empty (skipped)
	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert!(modified.is_empty(), "workspace: deps should be skipped");
}

#[test]
fn update_dep_version_not_found_returns_empty() {
	let dir = temp_dir();
	let info = make_project_with_deps(dir.path(), r#"{"other-dep": "1.0.0"}"#);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "2.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "nonexistent", &new_version, false)
		.unwrap();

	assert!(modified.is_empty());
}

#[test]
fn update_dep_version_in_dev_dependencies() {
	let dir = temp_dir();
	let content =
		r#"{"name": "pkg-a", "version": "0.1.0", "devDependencies": {"pkg-b": "^1.0.0"}}"#;
	write_package_json(dir.path(), content);
	let info = project_info(dir.path(), "pkg-a", "");
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "2.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let updated = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(updated.contains("\"pkg-b\": \"^2.0.0\""), "got: {updated}");
}

#[test]
fn semver_range_prefix_caret() {
	assert_eq!(crate::package_manager::semver_range_prefix("^1.0.0"), "^");
}

#[test]
fn semver_range_prefix_tilde() {
	assert_eq!(crate::package_manager::semver_range_prefix("~1.2.0"), "~");
}

#[test]
fn semver_range_prefix_empty() {
	assert_eq!(crate::package_manager::semver_range_prefix("1.0.0"), "");
}

#[test]
fn update_dep_version_in_peer_dependencies() {
	let dir = temp_dir();
	let content =
		r#"{"name": "pkg-a", "version": "0.1.0", "peerDependencies": {"pkg-b": "^1.0.0"}}"#;
	write_package_json(dir.path(), content);
	let info = project_info(dir.path(), "pkg-a", "");
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "2.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let updated = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(updated.contains("\"pkg-b\": \"^2.0.0\""), "got: {updated}");
}

#[test]
fn update_dep_version_in_optional_dependencies() {
	let dir = temp_dir();
	let content =
		r#"{"name": "pkg-a", "version": "0.1.0", "optionalDependencies": {"pkg-b": "~1.0.0"}}"#;
	write_package_json(dir.path(), content);
	let info = project_info(dir.path(), "pkg-a", "");
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let new_version: Version = "2.0.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	let updated = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(updated.contains("\"pkg-b\": \"~2.0.0\""), "got: {updated}");
}

#[test]
fn update_dep_version_in_root_workspace_project() {
	// The root package.json in an npm workspace can depend on workspace members.
	// project.path is the absolute dir path for the root, so update_dependency_version
	// should write to dir/package.json.
	let dir = temp_dir();
	let content = r#"{
  "name": "my-monorepo",
  "version": "1.0.0",
  "private": true,
  "workspaces": ["packages/*"],
  "dependencies": {
    "pkg-b": "^0.2.0"
  }
}"#;
	write_package_json(dir.path(), content);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	// Root project path is the absolute dir path
	let info = project_info(dir.path(), "my-monorepo", "");
	let new_version: Version = "0.3.0".parse().unwrap();

	let modified = adapter
		.update_dependency_version(&info, "pkg-b", &new_version, false)
		.unwrap();

	assert_eq!(modified.len(), 1);
	assert_eq!(modified[0], dir.path().join("package.json"));
	let updated = std::fs::read_to_string(&modified[0]).unwrap();
	assert!(updated.contains("\"pkg-b\": \"^0.3.0\""), "got: {updated}");
}
