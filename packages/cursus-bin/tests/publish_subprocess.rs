//! Subprocess tests for the `publish` command that require capturing stderr.

mod common;

use common::temp_git_repo;

#[tokio::test]
async fn publish_dry_run_cyclic_npm_workspace_warnings_suppressed() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
		"[npm]\nenabled = true\n\n[global]\ndisable_dependency_cycle_warnings = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/alpha")).unwrap();
	std::fs::write(
		dir.path().join("packages/alpha/package.json"),
		r#"{"name": "@test/alpha", "version": "1.0.0", "dependencies": {"@test/beta": "1.0.0"}}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/beta")).unwrap();
	std::fs::write(
		dir.path().join("packages/beta/package.json"),
		r#"{"name": "@test/beta", "version": "1.0.0", "dependencies": {"@test/alpha": "1.0.0"}}"#,
	)
	.unwrap();

	let (success, _stdout, stderr) =
		common::run_cursus_subprocess(&["publish", "--no-interactive", "--dry-run"], dir.path());

	// Should succeed
	assert!(success, "Expected success, stderr: {stderr}");

	// Should NOT emit any cycle warning when disable_dependency_cycle_warnings = true
	assert!(
		!stderr.contains("circular dependencies detected between"),
		"Expected no cycle warning in stderr, got: {stderr}"
	);
}
