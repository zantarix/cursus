//! Integration tests for the `publish` command's git lifecycle integration.

mod common;

use common::{
	git_enabled_config, git_tags, run_chronicle, temp_real_git_repo_with_cargo_workspace,
	temp_real_git_repo_with_config,
};

use chronicle::model::config::PackageManager;

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
