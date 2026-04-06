//! Integration tests for the `publish` command's git lifecycle integration.

mod common;

use common::{
	git_enabled_config, git_tags, run_cursus, temp_real_git_repo_with_cargo_workspace,
	temp_real_git_repo_with_config,
};

use cursus::model::config::{GitConfig, PackageManager};
use cursus::test_logging::{init_test_logger, take_logs};

/// Helper: write a config TOML directly to `.cursus/config.toml`.
fn write_config(dir: &std::path::Path, toml: &str) {
	let config_dir = dir.join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), toml).unwrap();
}

// --- Flag parsing ---

#[tokio::test]
async fn publish_no_git_flag_parses_without_error() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();

	// --no-git just skips git operations; --dry-run avoids hitting a registry.
	// The command should succeed (no changesets → exit success is fine too).
	let result = run_cursus(
		[
			"cursus",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

// --- Dry-run does not create actual tags ---

#[tokio::test]
async fn publish_git_enabled_dry_run_does_not_create_tags() {
	let dir =
		temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config()).await;
	// Dry-run should report what would happen but not touch the git repository.
	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
	assert!(
		git_tags(dir.path()).is_empty(),
		"Dry-run should not create any git tags"
	);
}

#[tokio::test]
async fn publish_no_git_dry_run_does_not_create_tags() {
	let dir =
		temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config()).await;
	let result = run_cursus(
		[
			"cursus",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
	assert!(
		git_tags(dir.path()).is_empty(),
		"--no-git should not create any git tags"
	);
}

// --- --no-git skips GitHub Releases token check ---

#[tokio::test]
async fn publish_no_git_skips_github_token_check() {
	// With github enabled but no token, the command should fail.
	// With --no-git, GitHub Releases are skipped entirely, so no token is needed.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
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
	let result = run_cursus(
		[
			"cursus",
			"publish",
			"--no-interactive",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok with --no-git, got: {result:?}");
}

// --- Log-content checks for dry-run tag decisions ---

/// Multi-package dry-run logs tags in `pkg@version` format (not `v{version}`).
///
/// This guards against `replace > with <` on `projects.len() > 1` (line ~109),
/// which would treat a multi-package repo as single-package and use the wrong
/// tag format.
#[tokio::test]
async fn publish_multi_package_dry_run_logs_prefixed_tag_format() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	)
	.await;
	std::fs::write(dir.path().join("pkg-a/CHANGELOG.md"), "# Changelog\n").unwrap();
	std::fs::write(dir.path().join("pkg-b/CHANGELOG.md"), "# Changelog\n").unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
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
#[tokio::test]
async fn publish_git_enabled_dry_run_logs_would_create_tag_and_summary_tag_note() {
	init_test_logger();
	let _ = take_logs();
	let dir =
		temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config()).await;
	std::fs::write(dir.path().join("my-app/CHANGELOG.md"), "# Changelog\n").unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
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
#[tokio::test]
async fn publish_git_disabled_dry_run_no_would_create_tag_in_logs_or_summary() {
	init_test_logger();
	let _ = take_logs();
	// Use write_config to set up a Cargo-only config (no git section → git disabled).
	let dir =
		temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config()).await;
	write_config(dir.path(), "[cargo]\nenabled = true\n");

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
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
#[tokio::test]
async fn publish_github_disabled_dry_run_no_would_create_github_release() {
	init_test_logger();
	let _ = take_logs();
	// git enabled but github not configured → github.enabled = false
	let dir =
		temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config()).await;

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("Would create GitHub Release")),
		"Should NOT log GitHub Release messages when GitHub is disabled, got: {logs:?}"
	);
}

/// Git disabled + CHANGELOG.md present (package is prepared): dry-run must NOT log "would be tagged".
///
/// Guards `&&`→`||` on the tag_note condition: with git disabled but published packages,
/// a `||` mutation would produce a non-empty tag_note even though git is off.
#[tokio::test]
async fn publish_git_disabled_with_changelog_dry_run_no_tag_note_in_summary() {
	init_test_logger();
	let _ = take_logs();
	// Set up workspace with git-disabled config.
	let dir =
		temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config()).await;
	write_config(dir.path(), "[cargo]\nenabled = true\n");
	// Write CHANGELOG.md so the package is "prepared" and would appear in published list.
	std::fs::write(dir.path().join("my-app/CHANGELOG.md"), "# Changelog\n").unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		!logs.iter().any(|(_, m)| m.contains("would be tagged")),
		"Summary should NOT include 'would be tagged' when git is disabled, got: {logs:?}"
	);
}

// ── publish_private_packages (ADR-043) ────────────────────────────────────────

/// A private npm package listed in `publish_private_packages` shows "Would create tag"
/// in dry-run (using the configured tag format) and does NOT show "Would publish ... to"
/// (no registry publish).
#[tokio::test]
async fn publish_private_package_in_list_dry_run_logs_would_create_tag() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_config(
		PackageManager::Npm,
		GitConfig::enabled_config().with_publish_private_packages(vec!["my-action".to_string()]),
	)
	.await;
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "my-action", "version": "1.2.0", "private": true}"#,
	)
	.unwrap();
	std::fs::write(dir.path().join("CHANGELOG.md"), "# Changelog\n").unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	// Tag is logged by maybe_create_tags using the configured format.
	// Single-package with TagFormat::Auto → "v{version}".
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Would create tag") && m.contains("v1.2.0")),
		"Expected 'Would create tag v1.2.0' in logs: {logs:?}"
	);
	// Summary distinguishes private packages from registry-published ones.
	assert!(
		logs.iter().any(|(_, m)| m.contains("private (tag only)")),
		"Expected 'private (tag only)' in summary: {logs:?}"
	);
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("Would publish my-action")),
		"Private package should NOT show 'Would publish' to a registry: {logs:?}"
	);
}

/// A private package NOT listed in `publish_private_packages` is silently skipped —
/// no per-package mention in logs (preserves ADR-007 behavior).
#[tokio::test]
async fn publish_private_package_not_in_list_silently_skipped() {
	init_test_logger();
	let _ = take_logs();
	let dir =
		temp_real_git_repo_with_config(PackageManager::Npm, GitConfig::enabled_config()).await;
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "my-action", "version": "1.2.0", "private": true}"#,
	)
	.unwrap();
	std::fs::write(dir.path().join("CHANGELOG.md"), "# Changelog\n").unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("my-action") && !m.contains("Summary")),
		"Private package not in list should produce no per-package log lines: {logs:?}"
	);
}

/// A private package in the list but missing `CHANGELOG.md` is warned about and
/// skipped — it was never prepared, so there are no release notes to publish.
#[tokio::test]
async fn publish_private_package_in_list_no_changelog_skipped() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_config(
		PackageManager::Npm,
		GitConfig::enabled_config().with_publish_private_packages(vec!["my-action".to_string()]),
	)
	.await;
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "my-action", "version": "1.2.0", "private": true}"#,
	)
	.unwrap();
	// No CHANGELOG.md — package was never prepared.

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Warn && m.contains("no CHANGELOG.md found")),
		"Expected CHANGELOG.md warning for unprepared private package: {logs:?}"
	);
	assert!(
		!logs.iter().any(|(_, m)| m.contains("Would create tag")),
		"Should NOT log 'Would create tag' for a package without CHANGELOG.md: {logs:?}"
	);
}

/// A public package named in `publish_private_packages` follows the normal registry
/// publish path — the list only affects packages that are actually private.
#[tokio::test]
async fn publish_public_package_in_private_list_follows_normal_path() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_config(
		PackageManager::Npm,
		GitConfig::enabled_config().with_publish_private_packages(vec!["my-pkg".to_string()]),
	)
	.await;
	// Public package (no "private": true)
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "my-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();
	std::fs::write(dir.path().join("CHANGELOG.md"), "# Changelog\n").unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	// Public package should show normal "Would publish ... to" message
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Would publish") && m.contains("my-pkg")),
		"Public package in list should still show 'Would publish' (registry path): {logs:?}"
	);
	assert!(
		!logs.iter().any(|(_, m)| m.contains("private (tag only)")),
		"Public package should NOT show private (tag only) in summary: {logs:?}"
	);
}

/// The summary line correctly shows private-tagged packages separately from
/// registry-published packages.
#[tokio::test]
async fn publish_private_package_in_list_dry_run_summary_shows_private_note() {
	init_test_logger();
	let _ = take_logs();
	// Workspace: one public, one private-listed.
	// Use temp_real_git_repo_with_config for the git setup, then write npm workspace manually.
	let dir = temp_real_git_repo_with_config(
		PackageManager::Npm,
		GitConfig::enabled_config().with_publish_private_packages(vec!["my-action".to_string()]),
	)
	.await;
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "private": true, "workspaces": ["packages/*"]}"#,
	)
	.unwrap();
	std::fs::create_dir_all(dir.path().join("packages/my-pkg")).unwrap();
	std::fs::write(
		dir.path().join("packages/my-pkg/package.json"),
		r#"{"name": "my-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();
	std::fs::write(
		dir.path().join("packages/my-pkg/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();
	std::fs::create_dir_all(dir.path().join("packages/my-action")).unwrap();
	std::fs::write(
		dir.path().join("packages/my-action/package.json"),
		r#"{"name": "my-action", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();
	std::fs::write(
		dir.path().join("packages/my-action/CHANGELOG.md"),
		"# Changelog\n",
	)
	.unwrap();

	let result = run_cursus(
		["cursus", "publish", "--no-interactive", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	// Summary: "1 would be published, 1 private (tag only), 0 would be skipped"
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("1 would be published") && m.contains("1 private (tag only)")),
		"Expected summary with both registry and private counts: {logs:?}"
	);
}
