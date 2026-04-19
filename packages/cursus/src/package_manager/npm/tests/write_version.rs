use super::*;

fn project_info(dir: &std::path::Path, name: &str, path: &str) -> ProjectInfo {
	ProjectInfo::for_test(name, AbsolutePath::new(dir.join(path)).unwrap())
}

#[tokio::test]
async fn write_version_file_not_found() {
	let dir = temp_dir();
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-app", "");
	let version: semver::Version = "1.0.0".parse().unwrap();
	let result = adapter.write_version(&info, &version, false).await;
	assert!(result.is_err());
}

#[tokio::test]
async fn write_version_invalid_json() {
	let dir = temp_dir();
	write_package_json(dir.path(), "not valid json");
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-app", "");
	let version: semver::Version = "1.0.0".parse().unwrap();
	let result = adapter.write_version(&info, &version, false).await;
	assert!(result.is_err());
}

#[tokio::test]
async fn write_version_updates_package_json() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-app", "");
	let new_version: semver::Version = "2.0.0".parse().unwrap();
	let paths = adapter
		.write_version(&info, &new_version, false)
		.await
		.unwrap();

	let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		contents.contains("\"2.0.0\""),
		"Should contain new version, got: {contents}"
	);
	assert!(contents.ends_with('\n'), "Should end with newline");
	assert_eq!(paths.len(), 1);
	assert_eq!(paths[0], dir.path().join("package.json"));
}

#[tokio::test]
async fn write_version_dry_run_does_not_write_file() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-app", "");
	let new_version: semver::Version = "2.0.0".parse().unwrap();
	let paths = adapter
		.write_version(&info, &new_version, true)
		.await
		.unwrap();

	let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		contents.contains("\"1.0.0\""),
		"dry-run should not modify the file, got: {contents}"
	);
	assert!(
		!contents.contains("\"2.0.0\""),
		"dry-run should not write new version, got: {contents}"
	);
	// Path is still reported even in dry-run mode
	assert_eq!(paths.len(), 1);
	assert_eq!(paths[0], dir.path().join("package.json"));
}

#[tokio::test]
async fn write_version_roundtrip() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "0.1.0"}"#);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-app", "");

	let new_v: semver::Version = "0.2.0".parse().unwrap();
	adapter.write_version(&info, &new_v, false).await.unwrap();

	// Re-enumerate to verify the write
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].version.to_string(), "0.2.0");
}

#[tokio::test]
async fn write_version_only_updates_package_version_not_dependencies() {
	let dir = temp_dir();
	// "1.0.0" appears as both the package version and as a dependency version.
	let json = "{\n  \"name\": \"my-app\",\n  \"version\": \"1.0.0\",\n  \"dependencies\": {\n    \"some-lib\": \"1.0.0\"\n  }\n}\n";
	write_package_json(dir.path(), json);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-app", "");
	let new_version: semver::Version = "2.0.0".parse().unwrap();
	adapter
		.write_version(&info, &new_version, false)
		.await
		.unwrap();

	let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		contents.contains("\"version\": \"2.0.0\""),
		"Package version should be updated, got: {contents}"
	);
	assert!(
		contents.contains("\"some-lib\": \"1.0.0\""),
		"Dependency version should be unchanged, got: {contents}"
	);
}

#[tokio::test]
async fn write_version_preserves_tab_indent() {
	let dir = temp_dir();
	let tab_json = "{\n\t\"name\": \"my-app\",\n\t\"version\": \"1.0.0\"\n}\n";
	write_package_json(dir.path(), tab_json);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-app", "");
	let new_version: semver::Version = "2.0.0".parse().unwrap();
	adapter
		.write_version(&info, &new_version, false)
		.await
		.unwrap();

	let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		contents.contains("\"2.0.0\""),
		"Should contain new version, got: {contents}"
	);
	assert!(
		contents.contains("\t\"version\""),
		"Should preserve tab indentation, got: {contents}"
	);
	assert!(
		!contents.contains("  \"version\""),
		"Should not have space indentation, got: {contents}"
	);
}

#[tokio::test]
async fn write_version_preserves_four_space_indent() {
	let dir = temp_dir();
	let four_space_json = "{\n    \"name\": \"my-app\",\n    \"version\": \"1.0.0\"\n}\n";
	write_package_json(dir.path(), four_space_json);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-app", "");
	let new_version: semver::Version = "2.0.0".parse().unwrap();
	adapter
		.write_version(&info, &new_version, false)
		.await
		.unwrap();

	let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		contents.contains("\"2.0.0\""),
		"Should contain new version, got: {contents}"
	);
	assert!(
		contents.contains("    \"version\""),
		"Should preserve 4-space indentation, got: {contents}"
	);
}

#[tokio::test]
async fn write_version_preserves_key_order() {
	let dir = temp_dir();
	// Keys are in non-alphabetical order: name, version, description.
	// Alphabetical order would be: description, name, version.
	let json =
		"{\n  \"name\": \"my-app\",\n  \"version\": \"1.0.0\",\n  \"description\": \"A test\"\n}\n";
	write_package_json(dir.path(), json);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	let info = project_info(dir.path(), "my-app", "");
	let new_version: semver::Version = "2.0.0".parse().unwrap();
	adapter
		.write_version(&info, &new_version, false)
		.await
		.unwrap();

	let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	let name_pos = contents.find("\"name\"").unwrap();
	let version_pos = contents.find("\"version\"").unwrap();
	let desc_pos = contents.find("\"description\"").unwrap();
	assert!(
		name_pos < version_pos && version_pos < desc_pos,
		"Key order not preserved: {contents}"
	);
	assert!(contents.contains("\"2.0.0\""));
}
