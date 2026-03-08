//! Integration tests for the `publish` command.

mod common;

use common::temp_git_repo;

use crate::common::run_chronicle_subprocess;

#[test]
fn publish_with_no_config_fails() {
	let dir = temp_git_repo();

	let result = common::run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);

	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No configuration found")
	);
}

#[test]
fn publish_dry_run_with_unknown_package_fails() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "test-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = common::run_chronicle(
		[
			"chronicle",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"nonexistent",
		],
		dir.path(),
	);

	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("Unknown package: nonexistent")
	);
}

#[test]
fn publish_dry_run_basic() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "test-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = common::run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_with_package_filter() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/pkg-a")).unwrap();
	std::fs::write(
		dir.path().join("packages/pkg-a/package.json"),
		r#"{"name": "pkg-a", "version": "1.0.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/pkg-b")).unwrap();
	std::fs::write(
		dir.path().join("packages/pkg-b/package.json"),
		r#"{"name": "pkg-b", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = common::run_chronicle(
		[
			"chronicle",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"pkg-a",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_with_workspace_dependencies() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/lib")).unwrap();
	std::fs::write(
		dir.path().join("packages/lib/package.json"),
		r#"{"name": "@chronicle-test/lib", "version": "1.0.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "@chronicle-test/app", "version": "1.0.0", "dependencies": {"@chronicle-test/lib": "1.0.0"}}"#,
	)
	.unwrap();

	let result = common::run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_with_workspace_dependencies_filtered() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	// Three packages: utils <- lib <- app
	std::fs::create_dir_all(dir.path().join("packages/utils")).unwrap();
	std::fs::write(
		dir.path().join("packages/utils/package.json"),
		r#"{"name": "@chronicle-test/utils", "version": "1.0.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/lib")).unwrap();
	std::fs::write(
		dir.path().join("packages/lib/package.json"),
		r#"{"name": "@chronicle-test/lib", "version": "1.0.0", "dependencies": {"@chronicle-test/utils": "1.0.0"}}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "@chronicle-test/app", "version": "1.0.0", "dependencies": {"@chronicle-test/lib": "1.0.0"}}"#,
	)
	.unwrap();

	// Publish only app and lib (not utils) — graph should still order lib before app
	let result = common::run_chronicle(
		[
			"chronicle",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"@chronicle-test/lib",
			"--package",
			"@chronicle-test/app",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_cargo_dry_run() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[cargo]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"test-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
	)
	.unwrap();

	std::fs::create_dir(dir.path().join("src")).unwrap();
	std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();

	let result = common::run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_npm_private_package_excluded() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	let result = common::run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);

	// Should succeed with no output (package silently excluded)
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_npm_mixed_workspace() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/private-pkg")).unwrap();
	std::fs::write(
		dir.path().join("packages/private-pkg/package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/public-pkg")).unwrap();
	std::fs::write(
		dir.path().join("packages/public-pkg/package.json"),
		r#"{"name": "public-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = common::run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);

	// Should succeed and only list public packages
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_cargo_publish_false_excluded() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[cargo]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"private-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\npath = \"src/lib.rs\"\n",
	)
	.unwrap();

	std::fs::create_dir(dir.path().join("src")).unwrap();
	std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();

	let result = common::run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);

	// Should succeed with no output (crate silently excluded)
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_explicitly_naming_private_package() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	let result = common::run_chronicle(
		[
			"chronicle",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"private-pkg",
		],
		dir.path(),
	);

	// Should succeed (not error) and silently skip the private package
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_cyclic_npm_workspace() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/types")).unwrap();
	std::fs::write(
		dir.path().join("packages/types/package.json"),
		r#"{"name": "@test/types", "version": "1.0.0", "dependencies": {"@test/utils": "1.0.0"}}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/utils")).unwrap();
	std::fs::write(
		dir.path().join("packages/utils/package.json"),
		r#"{"name": "@test/utils", "version": "1.0.0", "dependencies": {"@test/types": "1.0.0"}}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "@test/app", "version": "1.0.0", "dependencies": {"@test/types": "1.0.0", "@test/utils": "1.0.0"}}"#,
	)
	.unwrap();

	// Run as subprocess to capture stdout/stderr output
	let (success, stdout, stderr) =
		run_chronicle_subprocess(&["publish", "--no-interactive", "--dry-run"], dir.path());

	// Should succeed despite circular dependencies
	assert!(success, "Expected success, stderr: {stderr}");

	// Should warn about circular dependencies on stderr
	assert!(
		stderr.contains("circular dependencies detected between"),
		"Expected cycle warning in stderr, got: {stderr}"
	);
	assert!(
		stderr.contains("@test/types") && stderr.contains("@test/utils"),
		"Expected cycle members in warning, got: {stderr}"
	);

	// @test/app (dependent) must appear after @test/types and @test/utils (the cycle group)
	let pos_types = stdout
		.find("@test/types")
		.expect("@test/types not in stdout");
	let pos_utils = stdout
		.find("@test/utils")
		.expect("@test/utils not in stdout");
	let pos_app = stdout.find("@test/app").expect("@test/app not in stdout");
	assert!(
		pos_types < pos_app && pos_utils < pos_app,
		"Expected @test/types and @test/utils before @test/app in stdout"
	);
}

#[test]
fn publish_dry_run_cyclic_npm_workspace_warnings_suppressed() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
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
		run_chronicle_subprocess(&["publish", "--no-interactive", "--dry-run"], dir.path());

	// Should succeed
	assert!(success, "Expected success, stderr: {stderr}");

	// Should NOT emit any cycle warning when disable_dependency_cycle_warnings = true
	assert!(
		!stderr.contains("circular dependencies detected between"),
		"Expected no cycle warning in stderr, got: {stderr}"
	);
}

#[test]
fn publish_dry_run_summary_single_public_package() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "my-pkg", "version": "1.2.3"}"#,
	)
	.unwrap();

	let (success, stdout, stderr) =
		run_chronicle_subprocess(&["publish", "--no-interactive", "--dry-run"], dir.path());

	assert!(success, "Expected success, stderr: {stderr}");
	assert!(
		stdout.contains("Would publish my-pkg@1.2.3"),
		"Expected per-package line in stdout, got: {stdout}"
	);
	assert!(
		stdout.contains("Summary: 1 would be published, 0 would be skipped"),
		"Expected summary 'Summary: 1 would be published, 0 would be skipped' in stdout, got: {stdout}"
	);
}

#[test]
fn publish_dry_run_summary_multiple_public_packages() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "private": true, "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/alpha")).unwrap();
	std::fs::write(
		dir.path().join("packages/alpha/package.json"),
		r#"{"name": "alpha", "version": "2.0.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/beta")).unwrap();
	std::fs::write(
		dir.path().join("packages/beta/package.json"),
		r#"{"name": "beta", "version": "3.0.0"}"#,
	)
	.unwrap();

	let (success, stdout, stderr) =
		run_chronicle_subprocess(&["publish", "--no-interactive", "--dry-run"], dir.path());

	assert!(success, "Expected success, stderr: {stderr}");
	assert!(
		stdout.contains("Summary: 2 would be published, 0 would be skipped"),
		"Expected summary 'Summary: 2 would be published, 0 would be skipped' in stdout, got: {stdout}"
	);
}

#[test]
fn publish_dry_run_summary_mixed_public_private_packages() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "private": true, "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/public-pkg")).unwrap();
	std::fs::write(
		dir.path().join("packages/public-pkg/package.json"),
		r#"{"name": "public-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/private-pkg")).unwrap();
	std::fs::write(
		dir.path().join("packages/private-pkg/package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	let (success, stdout, stderr) =
		run_chronicle_subprocess(&["publish", "--no-interactive", "--dry-run"], dir.path());

	assert!(success, "Expected success, stderr: {stderr}");
	// Only the public package is counted; private is silently excluded
	assert!(
		stdout.contains("Summary: 1 would be published, 0 would be skipped"),
		"Expected summary 'Summary: 1 would be published, 0 would be skipped' in stdout, got: {stdout}"
	);
}

#[test]
fn publish_dry_run_summary_all_private_packages() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "private-root", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	let (success, stdout, stderr) =
		run_chronicle_subprocess(&["publish", "--no-interactive", "--dry-run"], dir.path());

	assert!(success, "Expected success, stderr: {stderr}");
	assert!(
		stdout.contains("Summary: 0 would be published, 0 would be skipped"),
		"Expected summary 'Summary: 0 would be published, 0 would be skipped' in stdout, got: {stdout}"
	);
}
