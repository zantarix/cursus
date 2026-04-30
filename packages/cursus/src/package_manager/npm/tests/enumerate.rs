use super::*;

fn write_pnpm_workspace(dir: &std::path::Path, content: &str) {
	std::fs::write(dir.join("pnpm-workspace.yaml"), content).unwrap();
}

#[tokio::test]
async fn enumerate_returns_empty_when_no_package_json() {
	let dir = temp_dir();
	let projects = enumerate(dir.path()).await.unwrap();
	assert!(projects.is_empty());
}

#[tokio::test]
async fn enumerate_single_package() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name, "my-app");
	assert_eq!(projects[0].path.as_path(), dir.path());
}

#[tokio::test]
async fn enumerate_single_package_without_name_fails() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"version": "0.1.0"}"#);

	let result = enumerate(dir.path()).await;
	assert!(result.is_err());
	let err_msg = result.unwrap_err().to_string();
	assert!(err_msg.contains("Missing name in"));
	assert!(err_msg.contains("package.json"));
}

#[tokio::test]
async fn enumerate_workspaces_array() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "monorepo", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
	);

	// Create workspace packages
	let pkg_a = dir.path().join("packages/pkg-a");
	let pkg_b = dir.path().join("packages/pkg-b");
	std::fs::create_dir_all(&pkg_a).unwrap();
	std::fs::create_dir_all(&pkg_b).unwrap();
	write_package_json(&pkg_a, r#"{"name": "@scope/pkg-a", "version": "0.1.0"}"#);
	write_package_json(&pkg_b, r#"{"name": "@scope/pkg-b", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 3);
	// Root project first (shorter absolute path sorts first)
	assert_eq!(projects[0].name, "monorepo");
	assert_eq!(projects[0].path.as_path(), dir.path());
	assert_eq!(projects[1].name, "@scope/pkg-a");
	assert_eq!(
		projects[1].path.as_path(),
		dir.path().join("packages/pkg-a")
	);
	assert_eq!(projects[2].name, "@scope/pkg-b");
	assert_eq!(
		projects[2].path.as_path(),
		dir.path().join("packages/pkg-b")
	);
}

#[tokio::test]
async fn enumerate_workspaces_object() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "root", "version": "0.1.0", "workspaces": {"packages": ["packages/*"]}}"#,
	);

	let pkg = dir.path().join("packages/my-pkg");
	std::fs::create_dir_all(&pkg).unwrap();
	write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "root");
	assert_eq!(projects[0].path.as_path(), dir.path());
	assert_eq!(projects[1].name, "my-pkg");
}

#[tokio::test]
async fn enumerate_workspace_without_root_name_fails() {
	let dir = temp_dir();
	// Root package without a name field
	write_package_json(
		dir.path(),
		r#"{"version": "0.1.0", "workspaces": ["packages/*"]}"#,
	);

	let pkg = dir.path().join("packages/my-pkg");
	std::fs::create_dir_all(&pkg).unwrap();
	write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

	let result = enumerate(dir.path()).await;
	assert!(result.is_err());
	let err_msg = result.unwrap_err().to_string();
	assert!(err_msg.contains("Missing name in"));
	assert!(err_msg.contains("package.json"));
}

#[tokio::test]
async fn enumerate_multiple_workspace_patterns() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "monorepo", "version": "0.1.0", "workspaces": ["packages/*", "apps/*"]}"#,
	);

	let pkg = dir.path().join("packages/lib");
	let app = dir.path().join("apps/web");
	std::fs::create_dir_all(&pkg).unwrap();
	std::fs::create_dir_all(&app).unwrap();
	write_package_json(&pkg, r#"{"name": "lib", "version": "0.1.0"}"#);
	write_package_json(&app, r#"{"name": "web", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 3);
	// Root first (shorter absolute path), then sorted by path
	assert_eq!(projects[0].name, "monorepo");
	assert_eq!(projects[0].path.as_path(), dir.path());
	assert_eq!(projects[1].name, "web");
	assert_eq!(projects[1].path.as_path(), dir.path().join("apps/web"));
	assert_eq!(projects[2].name, "lib");
	assert_eq!(projects[2].path.as_path(), dir.path().join("packages/lib"));
}

#[tokio::test]
async fn enumerate_skips_directories_without_package_json() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
	);

	let pkg = dir.path().join("packages/valid");
	let no_pkg = dir.path().join("packages/no-package-json");
	std::fs::create_dir_all(&pkg).unwrap();
	std::fs::create_dir_all(&no_pkg).unwrap();
	write_package_json(&pkg, r#"{"name": "valid", "version": "0.1.0"}"#);
	// no_pkg has no package.json

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "root");
	assert_eq!(projects[1].name, "valid");
}

#[tokio::test]
async fn enumerate_skips_files_matching_glob() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
	);

	std::fs::create_dir_all(dir.path().join("packages")).unwrap();
	// Create a file instead of directory
	std::fs::write(dir.path().join("packages/not-a-dir"), "").unwrap();

	let projects = enumerate(dir.path()).await.unwrap();

	// Only root project, no workspace packages
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name, "root");
}

#[tokio::test]
async fn enumerate_handles_nested_workspaces() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*/subpackages/*"]}"#,
	);

	let nested = dir.path().join("packages/group/subpackages/nested-pkg");
	std::fs::create_dir_all(&nested).unwrap();
	write_package_json(&nested, r#"{"name": "nested-pkg", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "root");
	assert_eq!(projects[1].name, "nested-pkg");
	assert_eq!(
		projects[1].path.as_path(),
		dir.path().join("packages/group/subpackages/nested-pkg")
	);
}

#[tokio::test]
async fn enumerate_fails_on_invalid_package_json() {
	let dir = temp_dir();
	write_package_json(dir.path(), "not valid json");

	let result = enumerate(dir.path()).await;

	assert!(result.is_err());
}

#[tokio::test]
async fn enumerate_fails_on_invalid_workspace_package_json() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"version": "0.1.0", "workspaces": ["packages/*"]}"#,
	);

	let pkg = dir.path().join("packages/bad");
	std::fs::create_dir_all(&pkg).unwrap();
	write_package_json(&pkg, "invalid json");

	let result = enumerate(dir.path()).await;

	assert!(result.is_err());
}

#[tokio::test]
async fn new_creates_adapter() {
	let dir = temp_dir();
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	// Should work without panicking
	let _ = adapter.enumerate_projects().await;
}

#[tokio::test]
async fn workspaces_patterns_array() {
	let ws = Workspaces::Array(vec!["a/*".to_string(), "b/*".to_string()]);
	assert_eq!(ws.patterns(), &["a/*", "b/*"]);
}

#[tokio::test]
async fn workspaces_patterns_object() {
	let ws = Workspaces::Object {
		packages: vec!["pkg/*".to_string()],
	};
	assert_eq!(ws.patterns(), &["pkg/*"]);
}

#[tokio::test]
async fn enumerate_pnpm_workspace() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "pnpm-monorepo", "version": "0.1.0"}"#,
	);
	write_pnpm_workspace(dir.path(), "packages:\n  - 'packages/*'\n");

	let pkg = dir.path().join("packages/my-pkg");
	std::fs::create_dir_all(&pkg).unwrap();
	write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "pnpm-monorepo");
	assert_eq!(projects[0].path.as_path(), dir.path());
	assert_eq!(projects[1].name, "my-pkg");
	assert_eq!(
		projects[1].path.as_path(),
		dir.path().join("packages/my-pkg")
	);
}

#[tokio::test]
async fn enumerate_pnpm_workspace_multiple_patterns() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "root", "version": "0.1.0"}"#);
	write_pnpm_workspace(dir.path(), "packages:\n  - 'packages/*'\n  - 'apps/*'\n");

	let pkg = dir.path().join("packages/lib");
	let app = dir.path().join("apps/web");
	std::fs::create_dir_all(&pkg).unwrap();
	std::fs::create_dir_all(&app).unwrap();
	write_package_json(&pkg, r#"{"name": "lib", "version": "0.1.0"}"#);
	write_package_json(&app, r#"{"name": "web", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 3);
	assert_eq!(projects[0].name, "root");
	assert_eq!(projects[1].name, "web");
	assert_eq!(projects[2].name, "lib");
}

#[tokio::test]
async fn enumerate_pnpm_workspace_takes_precedence_over_package_json() {
	let dir = temp_dir();
	// package.json has workspaces pointing to different location
	write_package_json(
		dir.path(),
		r#"{"name": "root", "version": "0.1.0", "workspaces": ["other/*"]}"#,
	);
	// pnpm-workspace.yaml should take precedence
	write_pnpm_workspace(dir.path(), "packages:\n  - 'packages/*'\n");

	let pkg = dir.path().join("packages/from-pnpm");
	let other = dir.path().join("other/from-npm");
	std::fs::create_dir_all(&pkg).unwrap();
	std::fs::create_dir_all(&other).unwrap();
	write_package_json(&pkg, r#"{"name": "from-pnpm", "version": "0.1.0"}"#);
	write_package_json(&other, r#"{"name": "from-npm", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "root");
	// Should find from-pnpm (pnpm-workspace.yaml), not from-npm (package.json workspaces)
	assert_eq!(projects[1].name, "from-pnpm");
}

#[tokio::test]
async fn enumerate_pnpm_workspace_empty_packages_falls_back_to_package_json() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
	);
	// pnpm-workspace.yaml with empty packages
	write_pnpm_workspace(dir.path(), "packages: []\n");

	let pkg = dir.path().join("packages/my-pkg");
	std::fs::create_dir_all(&pkg).unwrap();
	write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "root");
	assert_eq!(projects[1].name, "my-pkg");
}

#[tokio::test]
async fn enumerate_pnpm_workspace_invalid_yaml_returns_error() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
	);
	// Invalid YAML
	write_pnpm_workspace(dir.path(), "not: valid: yaml: [[");

	let result = enumerate(dir.path()).await;

	assert!(result.is_err());
}

#[tokio::test]
async fn enumerate_pnpm_workspace_without_packages_field() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
	);
	// pnpm-workspace.yaml without packages field
	write_pnpm_workspace(dir.path(), "other_field: true\n");

	let pkg = dir.path().join("packages/my-pkg");
	std::fs::create_dir_all(&pkg).unwrap();
	write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

	let projects = enumerate(dir.path()).await.unwrap();

	assert_eq!(projects.len(), 2);
	assert_eq!(projects[0].name, "root");
	assert_eq!(projects[1].name, "my-pkg");
}

#[tokio::test]
async fn enumerate_single_package_in_subfolder() {
	let dir = temp_dir();
	let subfolder = dir.path().join("frontend");
	std::fs::create_dir_all(&subfolder).unwrap();
	write_package_json(&subfolder, r#"{"name": "my-app", "version": "0.1.0"}"#);

	let projects = enumerate_with_path(dir.path(), "frontend").await.unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name, "my-app");
	assert_eq!(projects[0].path.as_path(), dir.path().join("frontend"));
}

#[tokio::test]
async fn enumerate_workspace_in_subfolder() {
	let dir = temp_dir();
	let subfolder = dir.path().join("frontend");
	std::fs::create_dir_all(&subfolder).unwrap();
	write_package_json(
		&subfolder,
		r#"{"name": "monorepo", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
	);

	let pkg_a = subfolder.join("packages/pkg-a");
	let pkg_b = subfolder.join("packages/pkg-b");
	std::fs::create_dir_all(&pkg_a).unwrap();
	std::fs::create_dir_all(&pkg_b).unwrap();
	write_package_json(&pkg_a, r#"{"name": "@scope/pkg-a", "version": "0.1.0"}"#);
	write_package_json(&pkg_b, r#"{"name": "@scope/pkg-b", "version": "0.1.0"}"#);

	let projects = enumerate_with_path(dir.path(), "frontend").await.unwrap();

	assert_eq!(projects.len(), 3);
	assert_eq!(projects[0].name, "monorepo");
	assert_eq!(projects[0].path.as_path(), dir.path().join("frontend"));
	assert_eq!(projects[1].name, "@scope/pkg-a");
	assert_eq!(
		projects[1].path.as_path(),
		dir.path().join("frontend/packages/pkg-a")
	);
	assert_eq!(projects[2].name, "@scope/pkg-b");
	assert_eq!(
		projects[2].path.as_path(),
		dir.path().join("frontend/packages/pkg-b")
	);
}

#[tokio::test]
async fn enumerate_errors_when_subfolder_missing() {
	let dir = temp_dir();
	let result = enumerate_with_path(dir.path(), "nonexistent").await;
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("does not exist or escapes repository root"),
		"got: {msg}"
	);
}

#[tokio::test]
async fn enumerate_includes_version() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.2.3"}"#);
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].version.to_string(), "1.2.3");
}

#[tokio::test]
async fn enumerate_missing_version_fails() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app"}"#);
	let result = enumerate(dir.path()).await;
	assert!(result.is_err());
}

#[tokio::test]
async fn enumerate_invalid_semver_fails() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "my-app", "version": "not-a-version"}"#,
	);
	let result = enumerate(dir.path()).await;
	assert!(result.is_err());
}

#[tokio::test]
async fn enumerate_includes_publishable_status() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert!(
		projects[0].publishable,
		"Package without private field should be publishable"
	);
}

#[tokio::test]
async fn enumerate_publishable_false_for_private_true() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "my-app", "version": "1.0.0", "private": true}"#,
	);
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert!(
		!projects[0].publishable,
		"Package with private: true should not be publishable"
	);
}

#[tokio::test]
async fn enumerate_publishable_true_for_private_false() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "my-app", "version": "1.0.0", "private": false}"#,
	);
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert!(
		projects[0].publishable,
		"Package with private: false should be publishable"
	);
}

#[tokio::test]
async fn enumerate_includes_dependency_names() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{
			"name": "my-app",
			"version": "1.0.0",
			"dependencies": {
				"react": "^18.0.0",
				"lodash": "^4.17.21"
			},
			"devDependencies": {
				"jest": "^29.0.0"
			},
			"peerDependencies": {
				"typescript": "^5.0.0"
			}
		}"#,
	);
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].dependency_names.len(), 4);
	assert!(projects[0].dependency_names.contains(&"react".to_string()));
	assert!(projects[0].dependency_names.contains(&"lodash".to_string()));
	assert!(projects[0].dependency_names.contains(&"jest".to_string()));
	assert!(
		projects[0]
			.dependency_names
			.contains(&"typescript".to_string())
	);
}

#[tokio::test]
async fn enumerate_parses_publishconfig_provenance_true() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "my-app", "version": "1.0.0", "publishConfig": {"provenance": true}}"#,
	);
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].publishconfig_provenance, Some(true));
}

#[tokio::test]
async fn enumerate_parses_publishconfig_provenance_false() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "my-app", "version": "1.0.0", "publishConfig": {"provenance": false}}"#,
	);
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].publishconfig_provenance, Some(false));
}

#[tokio::test]
async fn enumerate_parses_no_publishconfig_as_none() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].publishconfig_provenance, None);
}

#[tokio::test]
async fn enumerate_parses_publishconfig_without_provenance_as_none() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "my-app", "version": "1.0.0", "publishConfig": {"registry": "https://registry.npmjs.org"}}"#,
	);
	let projects = enumerate(dir.path()).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].publishconfig_provenance, None);
}

#[tokio::test]
async fn enumerate_rejects_leading_dash_package_name() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "--exec=evil", "version": "1.0.0"}"#);
	let err = enumerate(dir.path()).await.unwrap_err();
	let msg = format!("{err:#}");
	assert!(
		msg.contains("Invalid package name"),
		"Expected 'Invalid package name', got: {msg}"
	);
	assert!(
		msg.contains("must not start with '-'"),
		"Expected validation detail, got: {msg}"
	);
}

#[tokio::test]
async fn enumerate_rejects_package_name_with_control_char() {
	// JSON \t escape produces a tab (0x09); our validator must reject ASCII control characters.
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		"{\"name\": \"my\\tpkg\", \"version\": \"1.0.0\"}",
	);
	let err = enumerate(dir.path()).await.unwrap_err();
	assert!(
		format!("{err:#}").contains("Invalid package name"),
		"Expected validation error"
	);
}

#[tokio::test]
async fn enumerate_workspace_rejects_member_leading_dash_name() {
	let dir = temp_dir();
	write_package_json(
		dir.path(),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	);
	let member_dir = dir.path().join("packages/evil");
	std::fs::create_dir_all(&member_dir).unwrap();
	write_package_json(&member_dir, r#"{"name": "--evil", "version": "0.1.0"}"#);
	let err = enumerate(dir.path()).await.unwrap_err();
	assert!(
		format!("{err:#}").contains("Invalid package name"),
		"Expected validation error for workspace member"
	);
}
