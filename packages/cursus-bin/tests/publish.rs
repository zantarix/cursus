//! Integration tests for the `publish` command.

mod common;

use common::{run_cursus, run_cursus_subprocess, temp_git_repo};
use cursus::test_logging::{init_test_logger, take_logs};

#[tokio::test]
async fn publish_with_no_config_fails() {
	let dir = temp_git_repo();

	let result = common::run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;

	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No configuration found")
	);
}

#[tokio::test]
async fn publish_dry_run_with_unknown_package_fails() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "test-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = common::run_cursus(
		[
			"cursus",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"nonexistent",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("Unknown package: nonexistent")
	);
}

#[tokio::test]
async fn publish_dry_run_basic() {
	init_test_logger();
	let _ = take_logs();

	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "test-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	// CHANGELOG.md must exist for the package to be considered prepared.
	std::fs::write(dir.path().join("CHANGELOG.md"), "# Changelog\n").unwrap();

	let result = common::run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info && m.contains("Would publish test-pkg@")),
		"Expected 'Would publish test-pkg@...' log, got: {logs:?}"
	);
}

#[tokio::test]
async fn publish_with_package_filter() {
	init_test_logger();
	let _ = take_logs();

	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
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

	// CHANGELOG.md for pkg-a so it is considered prepared.
	std::fs::write(
		dir.path().join("packages/pkg-a/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/pkg-b")).unwrap();
	std::fs::write(
		dir.path().join("packages/pkg-b/package.json"),
		r#"{"name": "pkg-b", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = common::run_cursus(
		[
			"cursus",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"pkg-a",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info && m.contains("Would publish pkg-a@")),
		"Expected 'Would publish pkg-a@...' log, got: {logs:?}"
	);
	assert!(
		!logs.iter().any(|(_, m)| m.contains("Would publish pkg-b@")),
		"Expected pkg-b to be excluded by the package filter, got: {logs:?}"
	);
}

#[tokio::test]
async fn publish_dry_run_with_workspace_dependencies() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
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
		r#"{"name": "@cursus-test/lib", "version": "1.0.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "@cursus-test/app", "version": "1.0.0", "dependencies": {"@cursus-test/lib": "1.0.0"}}"#,
	)
	.unwrap();

	let result = common::run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[tokio::test]
async fn publish_dry_run_with_workspace_dependencies_filtered() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
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
		r#"{"name": "@cursus-test/utils", "version": "1.0.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/lib")).unwrap();
	std::fs::write(
		dir.path().join("packages/lib/package.json"),
		r#"{"name": "@cursus-test/lib", "version": "1.0.0", "dependencies": {"@cursus-test/utils": "1.0.0"}}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "@cursus-test/app", "version": "1.0.0", "dependencies": {"@cursus-test/lib": "1.0.0"}}"#,
	)
	.unwrap();

	// Publish only app and lib (not utils) — graph should still order lib before app
	let result = common::run_cursus(
		[
			"cursus",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"@cursus-test/lib",
			"--package",
			"@cursus-test/app",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[tokio::test]
async fn publish_cargo_dry_run() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
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

	let result = common::run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[tokio::test]
async fn publish_dry_run_npm_private_package_excluded() {
	init_test_logger();
	let _ = take_logs();

	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	let result = common::run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;

	// Should succeed with no output (package silently excluded)
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);

	let logs = take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("Would publish private-pkg")),
		"Private package should not appear in 'Would publish' output, got: {logs:?}"
	);
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info && m.contains("0 would be published")),
		"Expected summary to show 0 would be published, got: {logs:?}"
	);
}

#[tokio::test]
async fn publish_dry_run_npm_mixed_workspace() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
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

	let result = common::run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;

	// Should succeed and only list public packages
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[tokio::test]
async fn publish_dry_run_cargo_publish_false_excluded() {
	init_test_logger();
	let _ = take_logs();

	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
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

	let result = common::run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;

	// Should succeed with no output (crate silently excluded)
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);

	let logs = take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("Would publish private-crate")),
		"Crate with publish=false should not appear in 'Would publish' output, got: {logs:?}"
	);
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info && m.contains("0 would be published")),
		"Expected summary to show 0 would be published, got: {logs:?}"
	);
}

#[tokio::test]
async fn publish_dry_run_explicitly_naming_private_package() {
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	let result = common::run_cursus(
		[
			"cursus",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"private-pkg",
		],
		dir.path(),
	)
	.await;

	// Should succeed (not error) and silently skip the private package
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[tokio::test]
async fn publish_dry_run_cyclic_npm_workspace() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
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
	std::fs::write(
		dir.path().join("packages/types/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/utils")).unwrap();
	std::fs::write(
		dir.path().join("packages/utils/package.json"),
		r#"{"name": "@test/utils", "version": "1.0.0", "dependencies": {"@test/types": "1.0.0"}}"#,
	)
	.unwrap();
	std::fs::write(
		dir.path().join("packages/utils/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "@test/app", "version": "1.0.0", "dependencies": {"@test/types": "1.0.0", "@test/utils": "1.0.0"}}"#,
	)
	.unwrap();
	std::fs::write(
		dir.path().join("packages/app/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	let logs = take_logs();

	// Should warn about circular dependencies
	let warn_msg = logs
		.iter()
		.find(|(level, m)| {
			*level == log::Level::Warn && m.contains("circular dependencies detected between")
		})
		.map(|(_, m)| m.as_str())
		.expect("Expected cycle warning log");
	assert!(
		warn_msg.contains("@test/types") && warn_msg.contains("@test/utils"),
		"Expected cycle members in warning, got: {warn_msg}"
	);

	// @test/app (dependent) must appear after @test/types and @test/utils in the log ordering
	let info_msgs: Vec<&str> = logs
		.iter()
		.filter(|(level, _)| *level == log::Level::Info)
		.map(|(_, m)| m.as_str())
		.collect();
	let pos_types = info_msgs
		.iter()
		.position(|m| m.contains("@test/types"))
		.expect("@test/types not in info logs");
	let pos_utils = info_msgs
		.iter()
		.position(|m| m.contains("@test/utils"))
		.expect("@test/utils not in info logs");
	let pos_app = info_msgs
		.iter()
		.position(|m| m.contains("@test/app"))
		.expect("@test/app not in info logs");
	assert!(
		pos_types < pos_app && pos_utils < pos_app,
		"Expected @test/types and @test/utils before @test/app in logs"
	);
}

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
		run_cursus_subprocess(&["publish", "--no-interactive", "--dry-run"], dir.path());

	// Should succeed
	assert!(success, "Expected success, stderr: {stderr}");

	// Should NOT emit any cycle warning when disable_dependency_cycle_warnings = true
	assert!(
		!stderr.contains("circular dependencies detected between"),
		"Expected no cycle warning in stderr, got: {stderr}"
	);
}

#[tokio::test]
async fn publish_dry_run_summary_single_public_package() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "my-pkg", "version": "1.2.3"}"#,
	)
	.unwrap();
	std::fs::write(dir.path().join("CHANGELOG.md"), "# Changelog\n").unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Would publish my-pkg@1.2.3")),
		"Expected per-package line in logs, got: {logs:?}"
	);
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Summary: 1 would be published, 0 would be skipped")),
		"Expected summary log, got: {logs:?}"
	);
}

#[tokio::test]
async fn publish_dry_run_summary_multiple_public_packages() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
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
	std::fs::write(
		dir.path().join("packages/alpha/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/beta")).unwrap();
	std::fs::write(
		dir.path().join("packages/beta/package.json"),
		r#"{"name": "beta", "version": "3.0.0"}"#,
	)
	.unwrap();
	std::fs::write(
		dir.path().join("packages/beta/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Summary: 2 would be published, 0 would be skipped")),
		"Expected summary log, got: {logs:?}"
	);
}

#[tokio::test]
async fn publish_dry_run_summary_mixed_public_private_packages() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
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
	std::fs::write(
		dir.path().join("packages/public-pkg/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/private-pkg")).unwrap();
	std::fs::write(
		dir.path().join("packages/private-pkg/package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	let logs = take_logs();
	// Only the public package is counted; private is silently excluded
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Summary: 1 would be published, 0 would be skipped")),
		"Expected summary log, got: {logs:?}"
	);
}

#[tokio::test]
async fn publish_dry_run_summary_all_private_packages() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "private-root", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Summary: 0 would be published, 0 would be skipped")),
		"Expected summary log, got: {logs:?}"
	);
}

/// A public package without `CHANGELOG.md` is warned about and skipped; publish succeeds.
#[tokio::test]
async fn publish_skips_package_without_changelog() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "my-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	// No CHANGELOG.md — package has never been prepared.

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected success, got: {result:?}");
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);

	let logs = take_logs();
	let warn_log = logs
		.iter()
		.find(|(level, m)| *level == log::Level::Warn && m.contains("no CHANGELOG.md found"))
		.map(|(_, m)| m.as_str())
		.expect("Expected warning about missing CHANGELOG.md");
	assert!(
		warn_log.contains("cursus prepare"),
		"Warning should mention 'cursus prepare', got: {warn_log}"
	);
}

/// In a two-package workspace, only the package with `CHANGELOG.md` is published;
/// the other is warned about and skipped with a correct summary.
#[tokio::test]
async fn publish_mixed_changelog_packages() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo();

	std::fs::create_dir(dir.path().join(".cursus")).unwrap();
	std::fs::write(
		dir.path().join(".cursus/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "private": true, "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/prepared")).unwrap();
	std::fs::write(
		dir.path().join("packages/prepared/package.json"),
		r#"{"name": "prepared-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();
	std::fs::write(
		dir.path().join("packages/prepared/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/unprepared")).unwrap();
	std::fs::write(
		dir.path().join("packages/unprepared/package.json"),
		r#"{"name": "unprepared-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();
	// No CHANGELOG.md for unprepared-pkg.

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	let logs = take_logs();

	// Only the prepared package should appear in "Would publish".
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Would publish prepared-pkg")),
		"Expected 'Would publish prepared-pkg' in logs, got: {logs:?}"
	);
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("Would publish unprepared-pkg")),
		"unprepared-pkg should not appear in 'Would publish', got: {logs:?}"
	);

	// Warning for unprepared-pkg.
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Warn && m.contains("no CHANGELOG.md found")),
		"Expected warning for unprepared-pkg, got: {logs:?}"
	);

	// Summary should reflect 1 unprepared skipped.
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("1 skipped (not yet prepared)")),
		"Expected summary to mention '1 skipped (not yet prepared)', got: {logs:?}"
	);
}
