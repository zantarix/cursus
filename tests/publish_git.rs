//! Integration tests for the `publish` command's git lifecycle integration.

mod common;

use common::{
	git_enabled_config, git_tags, run_chronicle, temp_real_git_repo_with_cargo_workspace,
	temp_real_git_repo_with_config,
};

use chronicle::model::config::PackageManager;
use chronicle::test_logging::{init_test_logger, take_logs};

/// Helper: write a config TOML directly to `.chronicle/config.toml`.
fn write_config(dir: &std::path::Path, toml: &str) {
	let config_dir = dir.join(".chronicle");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), toml).unwrap();
}

// --- Flag parsing ---

#[test]
fn publish_no_git_flag_parses_without_error() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();

	// --no-git just skips git operations; --dry-run avoids hitting a registry.
	// The command should succeed (no changesets → exit success is fine too).
	let result = run_chronicle(
		[
			"chronicle",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

// --- Dry-run does not create actual tags ---

#[test]
fn publish_git_enabled_dry_run_does_not_create_tags() {
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());
	// Dry-run should report what would happen but not touch the git repository.
	let result = run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
	assert!(
		git_tags(dir.path()).is_empty(),
		"Dry-run should not create any git tags"
	);
}

#[test]
fn publish_no_git_dry_run_does_not_create_tags() {
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());
	let result = run_chronicle(
		[
			"chronicle",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
	assert!(
		git_tags(dir.path()).is_empty(),
		"--no-git should not create any git tags"
	);
}

// --- --no-git skips GitHub Releases token check ---

#[test]
fn publish_no_git_skips_github_token_check() {
	// With github enabled but no token, the command should fail.
	// With --no-git, GitHub Releases are skipped entirely, so no token is needed.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[git]\nenabled = true\n[github]\nenabled = true\n",
	);
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();

	// Without --no-git and no token, this would fail due to missing GitHub token.
	// With --no-git, it should succeed (dry-run so no actual publish either).
	let result = run_chronicle(
		[
			"chronicle",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok with --no-git, got: {result:?}");
}

// --- Log-content checks for dry-run tag decisions ---

/// Multi-package dry-run logs tags in `pkg@version` format (not `v{version}`).
///
/// This guards against `replace > with <` on `projects.len() > 1` (line ~109),
/// which would treat a multi-package repo as single-package and use the wrong
/// tag format.
#[test]
fn publish_multi_package_dry_run_logs_prefixed_tag_format() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);

	let result = run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	// Multi-package repos use "pkg@version" format
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Would create tag") && m.contains("pkg-a@1.0.0")),
		"Multi-package dry-run should log 'Would create tag pkg-a@1.0.0', got: {logs:?}"
	);
	// Should not use the single-package "v{version}" format
	assert!(
		!logs.iter().any(|(_, m)| m.contains("Would create tag v")),
		"Multi-package dry-run should not log 'v{{version}}' format, got: {logs:?}"
	);
}

/// Single-package, git enabled, no `--no-git`: dry-run logs "Would create tag".
///
/// Guards against `delete !` on `git_enabled = config.git.enabled() && !args.no_git`
/// (line ~115), which would make git_enabled false when no_git is false.
#[test]
fn publish_git_enabled_dry_run_logs_would_create_tag_and_summary_tag_note() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());

	let result = run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	// "Would create tag" must appear when git is enabled
	assert!(
		logs.iter().any(|(_, m)| m.contains("Would create tag")),
		"Should log 'Would create tag' when git is enabled, got: {logs:?}"
	);
	// Summary must include the tag note when git is enabled and packages were published
	assert!(
		logs.iter().any(|(_, m)| m.contains("would be tagged")),
		"Summary should include 'would be tagged' when git is enabled, got: {logs:?}"
	);
}

/// Git disabled, no `--no-git`: dry-run must NOT log "Would create tag".
///
/// Guards against `replace && with ||` on `git_enabled = config.git.enabled() && !args.no_git`
/// (line ~115), which would make git_enabled true even when git is disabled.
/// Also guards against `replace && with ||` on the tag_note guard (line ~171).
#[test]
fn publish_git_disabled_dry_run_no_would_create_tag_in_logs_or_summary() {
	init_test_logger();
	let _ = take_logs();
	// Use write_config to set up a Cargo-only config (no git section → git disabled).
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());
	write_config(dir.path(), "[cargo]\nenabled = true\n");

	let result = run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		!logs.iter().any(|(_, m)| m.contains("Would create tag")),
		"Should NOT log 'Would create tag' when git is disabled, got: {logs:?}"
	);
	assert!(
		!logs.iter().any(|(_, m)| m.contains("would be tagged")),
		"Summary should NOT include 'would be tagged' when git is disabled, got: {logs:?}"
	);
}

/// GitHub disabled, no `--no-git`: dry-run must NOT log "Would create GitHub Release".
///
/// Guards against `replace && with ||` on `config.github.enabled && !args.no_git`
/// (line ~134), which would log GitHub Release messages even when GitHub is disabled.
#[test]
fn publish_github_disabled_dry_run_no_would_create_github_release() {
	init_test_logger();
	let _ = take_logs();
	// git enabled but github not configured → github.enabled = false
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());

	let result = run_chronicle(
		["chronicle", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("Would create GitHub Release")),
		"Should NOT log GitHub Release messages when GitHub is disabled, got: {logs:?}"
	);
}
