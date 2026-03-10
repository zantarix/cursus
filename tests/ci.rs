//! Integration tests for the `ci` subcommand.

mod common;

use std::process::Command;

use chronicle::git::GitConfig;
use chronicle::model::config::PackageManager;
use common::{
	git_tag_exists, git_tags, run_chronicle, temp_git_repo, temp_git_repo_with_project,
	temp_real_git_repo_with_cargo_workspace, temp_real_git_repo_with_config,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Creates a changeset file in the `.chronicle` directory.
fn write_changeset(dir: &std::path::Path, filename: &str, content: &str) {
	let chronicle_dir = dir.join(".chronicle");
	std::fs::create_dir_all(&chronicle_dir).unwrap();
	std::fs::write(chronicle_dir.join(filename), content).unwrap();
}

/// Creates a lightweight git tag in the given directory.
fn git_tag(dir: &std::path::Path, tag: &str) {
	let out = Command::new("git")
		.args(["tag", tag])
		.current_dir(dir)
		.output()
		.unwrap();
	assert!(
		out.status.success(),
		"git tag failed: {}",
		String::from_utf8_lossy(&out.stderr)
	);
}

fn git_enabled_config() -> GitConfig {
	GitConfig {
		enabled: Some(true),
		..Default::default()
	}
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// When pending changesets exist, `ci` should delegate to `release`.
#[test]
fn ci_with_changesets_runs_release() {
	// Use a simple fake-git repo with git disabled to avoid real git ops.
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nA feature\n",
	);

	// --dry-run avoids filesystem changes; --no-git avoids git operations.
	let result = run_chronicle(
		[
			"chronicle",
			"--no-interactive",
			"ci",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	// Verify no version bump happened (dry-run)
	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("version = \"0.1.0\""),
		"Dry-run should not bump the version"
	);
}

/// When no changesets exist and all tags are present, `ci` does nothing.
#[test]
fn ci_when_all_tags_present_nothing_to_do() {
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());

	// No changesets created; tag the current version manually.
	git_tag(dir.path(), "v1.0.0");

	let result = run_chronicle(
		["chronicle", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	// No new tags should have been created.
	let tags = git_tags(dir.path());
	assert_eq!(tags, vec!["v1.0.0"], "No new tags should have been created");
}

/// When there are no changesets and git is disabled, `ci` does nothing.
#[test]
fn ci_git_disabled_no_changesets_nothing_to_do() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	// No changesets. Git is not enabled in config (no [git] section).

	let result = run_chronicle(
		["chronicle", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

/// When no changesets exist but tags are missing, `ci` delegates to `publish` (dry-run).
#[test]
fn ci_tags_missing_triggers_publish_dry_run() {
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());

	// No changesets. Tag for v1.0.0 is absent → post-release, pre-publish state.
	assert!(!git_tag_exists(dir.path(), "v1.0.0"));

	// With --dry-run, publish does not create real tags or push to a registry.
	let result = run_chronicle(
		["chronicle", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	// Dry-run should not create actual tags.
	assert!(
		git_tags(dir.path()).is_empty(),
		"Dry-run publish should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

/// With --no-git, `ci` never checks for missing tags and does nothing when there
/// are no changesets.
#[test]
fn ci_no_git_skips_tag_detection() {
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());

	// No changesets, tag missing — but --no-git should prevent tag detection.
	assert!(!git_tag_exists(dir.path(), "v1.0.0"));

	let result = run_chronicle(
		["chronicle", "--no-interactive", "ci", "--no-git"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	// No tags should have been created.
	assert!(
		git_tags(dir.path()).is_empty(),
		"--no-git should skip publish, got: {:?}",
		git_tags(dir.path())
	);
}

/// `ci` accepts `--no-interactive` (consistent with other subcommands) but does not
/// require it — it is always non-interactive by design.
#[test]
fn ci_is_always_non_interactive() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);

	// No changesets, no git. Should succeed without --no-interactive.
	let result = run_chronicle(["chronicle", "ci", "--dry-run"], dir.path());
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

/// `ci --dry-run` with changesets does not consume the changesets.
#[test]
fn ci_dry_run_does_not_consume_changesets() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nA feature\n",
	);

	let result = run_chronicle(
		[
			"chronicle",
			"--no-interactive",
			"ci",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let changeset_exists = dir.path().join(".chronicle").join("change.md").exists();
	assert!(changeset_exists, "Dry-run should not consume changesets");
}

/// The `ci` subcommand parses correctly via the CLI.
#[test]
fn ci_parses_from_cli() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	// Just verify the CLI parses `ci` as a valid subcommand with its flags.
	let result = run_chronicle(
		[
			"chronicle",
			"--no-interactive",
			"ci",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

/// Multi-package: `ci` uses all packages when determining tag presence.
#[test]
fn ci_multi_package_partial_tags_triggers_publish() {
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);

	// Tag pkg-a but not pkg-b — should still trigger publish.
	git_tag(dir.path(), "pkg-a@1.0.0");
	assert!(!git_tag_exists(dir.path(), "pkg-b@2.0.0"));

	let result = run_chronicle(
		["chronicle", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

/// Multi-package: `ci` does nothing when ALL packages are tagged.
#[test]
fn ci_multi_package_all_tags_present_nothing_to_do() {
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);

	git_tag(dir.path(), "pkg-a@1.0.0");
	git_tag(dir.path(), "pkg-b@2.0.0");

	let result = run_chronicle(
		["chronicle", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	// No additional tags created.
	let tags = git_tags(dir.path());
	assert_eq!(tags.len(), 2, "No new tags should have been created");
}

/// `ci` with a package filter only checks the selected package's tag.
#[test]
fn ci_package_filter_only_checks_selected_packages() {
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);

	// Tag pkg-a only; filter to pkg-a → should see "nothing to do" since pkg-a is tagged.
	git_tag(dir.path(), "pkg-a@1.0.0");
	assert!(!git_tag_exists(dir.path(), "pkg-b@2.0.0"));

	let result = run_chronicle(
		[
			"chronicle",
			"--no-interactive",
			"ci",
			"--dry-run",
			"-p",
			"pkg-a",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	// Only the original tag should exist (nothing triggered).
	let tags = git_tags(dir.path());
	assert_eq!(tags, vec!["pkg-a@1.0.0"], "Only pkg-a@1.0.0 should exist");
}

/// `ci --no-git` with changesets delegates to `release --no-git`.
#[test]
fn ci_no_git_with_changesets_runs_release_no_git() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-pkg\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
	)
	.unwrap();
	std::fs::create_dir_all(dir.path().join("src")).unwrap();
	std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);

	let result = run_chronicle(
		[
			"chronicle",
			"--no-interactive",
			"ci",
			"--dry-run",
			"--no-git",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

/// `ci` returns an error when the config file is missing.
#[test]
fn ci_fails_when_no_config() {
	let dir = temp_git_repo();
	// No .chronicle/config.toml present.
	let result = run_chronicle(["chronicle", "--no-interactive", "ci"], dir.path());
	assert!(result.is_err(), "Expected Err when config is missing");
}

/// `ci` returns an error when a requested package does not exist (with git enabled).
#[test]
fn ci_fails_when_package_filter_names_unknown_package() {
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());

	// No changesets; git enabled; tag missing → would try to publish, but -p nonexistent fails.
	let result = run_chronicle(
		[
			"chronicle",
			"--no-interactive",
			"ci",
			"--dry-run",
			"-p",
			"nonexistent",
		],
		dir.path(),
	);
	assert!(
		result.is_err(),
		"Expected Err for unknown package filter, got: {result:?}"
	);
}
