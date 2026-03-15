//! Integration tests for the `ci` subcommand.

mod common;

use std::process::Command;

use common::{
	git_enabled_config, git_tag_exists, git_tags, run_cursus, temp_git_repo,
	temp_git_repo_with_project, temp_real_git_repo_with_cargo_workspace,
	temp_real_git_repo_with_config, write_changeset,
};
use cursus::model::config::PackageManager;
use cursus::test_logging::{init_test_logger, take_logs};

// ── Helpers ──────────────────────────────────────────────────────────────────

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

// ── Tests ─────────────────────────────────────────────────────────────────────

/// When pending changesets exist, `ci` should delegate to `prepare`.
#[test]
fn ci_with_changesets_runs_release() {
	init_test_logger();
	let _ = take_logs();
	// Use a simple fake-git repo with git disabled to avoid real git ops.
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nA feature\n",
	);

	// --dry-run avoids filesystem changes; --no-git avoids git operations.
	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run", "--no-git"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter().any(|(level, m)| *level == log::Level::Info
			&& m.contains("pending changesets found")
			&& m.contains("prepare")),
		"Expected info 'pending changesets found, running prepare' log, got: {logs:?}"
	);

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
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());

	// No changesets created; tag the current version manually.
	git_tag(dir.path(), "v1.0.0");

	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info && m.contains("nothing to do")),
		"Expected info 'ci: nothing to do' log, got: {logs:?}"
	);

	// No new tags should have been created.
	let tags = git_tags(dir.path());
	assert_eq!(tags, vec!["v1.0.0"], "No new tags should have been created");
}

/// When there are no changesets and git is disabled, `ci` does nothing.
#[test]
fn ci_git_disabled_no_changesets_nothing_to_do() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	// No changesets. Git is not enabled in config (no [git] section).

	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info && m.contains("nothing to do")),
		"Expected info 'ci: nothing to do' log, got: {logs:?}"
	);
}

/// When no changesets exist but tags are missing, `ci` delegates to `publish` (dry-run).
#[test]
fn ci_tags_missing_triggers_publish_dry_run() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());

	// Add CHANGELOG.md so the package is considered prepared and tag check applies.
	std::fs::write(dir.path().join("my-app/CHANGELOG.md"), "# Changelog\n").unwrap();

	// No changesets. Tag for v1.0.0 is absent → post-release, pre-publish state.
	assert!(!git_tag_exists(dir.path(), "v1.0.0"));

	// With --dry-run, publish does not create real tags or push to a registry.
	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter().any(|(level, m)| *level == log::Level::Info
			&& m.contains("unpublished tags detected")
			&& m.contains("publish")),
		"Expected info 'no changesets but unpublished tags detected, running publish' log, got: {logs:?}"
	);

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

	let result = run_cursus(["cursus", "--no-interactive", "ci", "--no-git"], dir.path());
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
	let result = run_cursus(["cursus", "ci", "--dry-run"], dir.path());
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

	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run", "--no-git"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let changeset_exists = dir.path().join(".cursus").join("change.md").exists();
	assert!(changeset_exists, "Dry-run should not consume changesets");
}

/// The `ci` subcommand parses correctly via the CLI.
#[test]
fn ci_parses_from_cli() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	// Just verify the CLI parses `ci` as a valid subcommand with its flags.
	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run", "--no-git"],
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

	// Add CHANGELOG.md to both packages so they are considered prepared.
	std::fs::write(dir.path().join("pkg-a/CHANGELOG.md"), "# Changelog\n").unwrap();
	std::fs::write(dir.path().join("pkg-b/CHANGELOG.md"), "# Changelog\n").unwrap();

	// Tag pkg-a but not pkg-b — should still trigger publish.
	git_tag(dir.path(), "pkg-a@1.0.0");
	assert!(!git_tag_exists(dir.path(), "pkg-b@2.0.0"));

	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run"],
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

	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run"],
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

	let result = run_cursus(
		[
			"cursus",
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

/// `ci --no-git` with changesets delegates to `prepare --no-git`.
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

	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run", "--no-git"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

/// `ci` returns an error when the config file is missing.
#[test]
fn ci_fails_when_no_config() {
	let dir = temp_git_repo();
	// No .cursus/config.toml present.
	let result = run_cursus(["cursus", "--no-interactive", "ci"], dir.path());
	assert!(result.is_err(), "Expected Err when config is missing");
}

/// Multi-package: `ci` logs "nothing to do" when all packages have their expected tags.
///
/// This test verifies that `is_multi` is computed as `projects.len() > 1` (not `< 1`).
/// With two packages both tagged in `pkg@version` format, CI should detect all tags present
/// and log "nothing to do" — not trigger a "running publish" dispatch.
#[test]
fn ci_multi_package_all_tags_present_logs_nothing_to_do() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);

	// Both packages tagged in multi-package (pkg@version) format.
	git_tag(dir.path(), "pkg-a@1.0.0");
	git_tag(dir.path(), "pkg-b@2.0.0");

	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info && m.contains("nothing to do")),
		"Expected 'ci: nothing to do' when all multi-package tags are present, got: {logs:?}"
	);
	assert!(
		!logs.iter().any(|(_, m)| m.contains("running publish")),
		"Should not trigger publish when all tags are present, got: {logs:?}"
	);
}

/// Multi-package workspace with no changesets and no `CHANGELOG.md` in any package:
/// `ci` should do nothing (all packages excluded from tag check).
#[test]
fn ci_all_packages_lack_changelog_nothing_to_do() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);

	// No changesets, no CHANGELOG.md in either package — tag check skipped for all.
	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info && m.contains("nothing to do")),
		"Expected 'ci: nothing to do' when no packages have CHANGELOG.md, got: {logs:?}"
	);
	assert!(
		!logs.iter().any(|(_, m)| m.contains("running publish")),
		"Should not trigger publish when no packages have CHANGELOG.md, got: {logs:?}"
	);
}

/// When one package has `CHANGELOG.md` (and its tag is missing) and another does not,
/// `ci` should trigger publish (the prepared package qualifies).
#[test]
fn ci_no_changelog_package_excluded_from_tag_check() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);

	// pkg-a has CHANGELOG.md and its tag is missing → qualifies for publish.
	// pkg-b has no CHANGELOG.md → excluded from tag check.
	std::fs::write(dir.path().join("pkg-a/CHANGELOG.md"), "# Changelog\n").unwrap();

	assert!(!git_tag_exists(dir.path(), "pkg-a@1.0.0"));
	assert!(!git_tag_exists(dir.path(), "pkg-b@2.0.0"));

	let result = run_cursus(
		["cursus", "--no-interactive", "ci", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter().any(|(level, m)| *level == log::Level::Info
			&& m.contains("unpublished tags detected")
			&& m.contains("publish")),
		"Expected publish triggered by pkg-a, got: {logs:?}"
	);
}

/// `ci` returns an error when a requested package does not exist (with git enabled).
#[test]
fn ci_fails_when_package_filter_names_unknown_package() {
	let dir = temp_real_git_repo_with_cargo_workspace(&[("my-app", "1.0.0")], git_enabled_config());

	// No changesets; git enabled; tag missing → would try to publish, but -p nonexistent fails.
	let result = run_cursus(
		[
			"cursus",
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

/// `ci` finds changesets but the `--package` filter doesn't match any changeset package.
/// Release should succeed with "nothing to release" for the filtered package.
#[test]
fn ci_changesets_present_but_package_filter_matches_no_changeset() {
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);

	// Changeset only mentions pkg-a, but we filter for pkg-b.
	write_changeset(
		dir.path(),
		"change.md",
		"+++\npkg-a = \"patch\"\n+++\n\nFix\n",
	);

	// ci detects changesets and dispatches to prepare with -p pkg-b.
	// Release finds nothing to do for pkg-b → succeeds with no changes.
	let result = run_cursus(
		[
			"cursus",
			"--no-interactive",
			"ci",
			"--dry-run",
			"--no-git",
			"-p",
			"pkg-b",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	// No version should have changed.
	let toml = std::fs::read_to_string(dir.path().join("pkg-b/Cargo.toml")).unwrap();
	assert!(
		toml.contains("version = \"2.0.0\""),
		"pkg-b version should not change when it has no changeset"
	);
}
