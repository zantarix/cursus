//! Integration tests for git lifecycle automation in the `prepare` command.

mod common;

use std::process::{Command, Stdio};

use chronicle::git::{GitConfig, Strategy, TagFormat};
use chronicle::model::config::PackageManager;
use common::{
	add_local_remote, git_current_branch, git_enabled_config, git_local_branch_exists, git_log,
	git_push_to_remote, git_tags, temp_git_repo_with_project,
	temp_real_git_repo_with_cargo_workspace, temp_real_git_repo_with_config, write_changeset,
};

/// Stages all files and creates a commit with the given message.
fn git_commit_all(dir: &std::path::Path, message: &str) {
	let output = Command::new("git")
		.args(["add", "."])
		.current_dir(dir)
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"git add failed:\n{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let output = Command::new("git")
		.args(["commit", "-m", message])
		.current_dir(dir)
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"git commit failed:\n{}",
		String::from_utf8_lossy(&output.stderr)
	);
}

/// Writes a single-package Cargo setup into the given directory and commits it.
fn setup_single_cargo_package(dir: &std::path::Path, name: &str, version: &str) {
	std::fs::write(
		dir.join("Cargo.toml"),
		format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
	)
	.unwrap();
	std::fs::create_dir_all(dir.join("src")).unwrap();
	std::fs::write(dir.join("src/lib.rs"), "").unwrap();
	git_commit_all(dir, "chore: add package");
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn prepare_git_disabled_by_default() {
	// Without [git] enabled, a release should succeed without touching git state.
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nA feature\n",
	);

	// Uses a fake .git dir, not a real repo — verifies nothing panics when enabled=false.
	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
}

#[test]
fn prepare_git_creates_commit() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nA feature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	let log = git_log(dir.path());
	assert!(
		log.iter().any(|msg| msg.contains("chore(release):")),
		"Expected a release commit, got log: {log:?}"
	);
	assert!(
		log[0].contains("my-pkg@0.2.0"),
		"Latest commit should mention my-pkg@0.2.0, got: {}",
		log[0]
	);
}

#[test]
fn prepare_git_does_not_create_tags() {
	// Tags are now created during publish, not release.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nA fix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	// Release no longer creates tags — publish does.
	assert!(
		git_tags(dir.path()).is_empty(),
		"Release should not create tags (tags are created on publish), got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn prepare_git_tag_format_config_no_tags_at_release() {
	// Tag format only affects publish step now; release just commits.
	let config = GitConfig {
		enabled: Some(true),
		tag_format: TagFormat::Prefixed,
		..Default::default()
	};
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, config);
	setup_single_cargo_package(dir.path(), "solo", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nsolo = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());

	assert!(
		git_tags(dir.path()).is_empty(),
		"Release should not create tags regardless of tag_format, got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn prepare_git_multi_package_creates_single_commit() {
	// When multiple packages are released simultaneously, a single commit is created.
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);
	write_changeset(
		dir.path(),
		"change.md",
		"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nFix and feature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	let log = git_log(dir.path());
	assert!(
		log[0].contains("pkg-a") && log[0].contains("pkg-b"),
		"Release commit should mention both packages, got: {}",
		log[0]
	);
	// No tags at release time
	assert!(
		git_tags(dir.path()).is_empty(),
		"Release should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn prepare_no_git_flag_skips_git() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "prepare", "--no-git"],
		dir.path(),
	);
	assert!(result.is_ok());

	let log = git_log(dir.path());
	assert!(
		!log.iter().any(|msg| msg.contains("chore(release):")),
		"--no-git should skip git operations, got log: {log:?}"
	);
	assert!(
		git_tags(dir.path()).is_empty(),
		"--no-git should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn prepare_git_stages_only_chronicle_files() {
	// Chronicle uses `git add -- <files>` for selective staging, so tracked
	// but unmodified files are never included in the release commit.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");

	// Commit an unrelated tracked file so the tree stays clean for the pre-flight check.
	std::fs::write(dir.path().join("unrelated.txt"), "tracked but unmodified").unwrap();
	git_commit_all(dir.path(), "chore: track unrelated file");

	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());

	// The release commit should not contain the unrelated file (it was not modified)
	let output = Command::new("git")
		.args(["show", "--name-only", "--format=", "HEAD"])
		.current_dir(dir.path())
		.output()
		.unwrap();
	let changed_files = String::from_utf8(output.stdout).unwrap();
	assert!(
		!changed_files.contains("unrelated.txt"),
		"Unrelated file should not be in the release commit, got: {changed_files}"
	);
}

#[test]
fn prepare_git_filesystem_changes_persist_after_lifecycle() {
	// Version bumps (filesystem) happen before git ops; a successful git lifecycle
	// should leave the bumped version in place.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());

	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("1.0.1"),
		"Version should be bumped to 1.0.1, got: {cargo_toml}"
	);
}

#[test]
fn prepare_dry_run_with_git_enabled_does_not_create_commit_or_tags() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "prepare", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok());

	let log = git_log(dir.path());
	assert!(
		!log.iter().any(|msg| msg.contains("chore(release):")),
		"Dry run should not create a commit, got log: {log:?}"
	);
	assert!(
		git_tags(dir.path()).is_empty(),
		"Dry run should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn prepare_git_extra_files_are_staged() {
	// An extra file produced by a custom lock_command should be staged in the release
	// commit. We use an npm project with a lock_command that writes custom.lock so the
	// file is created WITHIN chronicle's execution (after the dirty-tree check).
	let git_config = GitConfig {
		enabled: Some(true),
		extra_files: vec!["custom.lock".to_string()],
		..Default::default()
	};
	let dir = temp_real_git_repo_with_config(PackageManager::Npm, git_config);

	// Write config with a lock_command that produces custom.lock during the release.
	std::fs::write(
		dir.path().join(".chronicle").join("config.toml"),
		"[npm]\nenabled = true\nlock_command = \"echo updated > custom.lock\"\n\
		 [git]\nenabled = true\nextra_files = [\"custom.lock\"]\n",
	)
	.unwrap();
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name":"my-pkg","version":"1.0.0"}"#,
	)
	.unwrap();
	// Track custom.lock from the start (initially empty / placeholder)
	std::fs::write(dir.path().join("custom.lock"), "initial").unwrap();
	git_commit_all(dir.path(), "chore: set up npm project");

	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	// Tree is clean; lock_command will write custom.lock during chronicle's execution.
	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	// Verify custom.lock was included in the release commit
	let output = Command::new("git")
		.args(["show", "--name-only", "--format=", "HEAD"])
		.current_dir(dir.path())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.output()
		.unwrap();
	let changed_files = String::from_utf8(output.stdout).unwrap();
	assert!(
		changed_files.contains("custom.lock"),
		"custom.lock should be in the release commit, got: {changed_files}"
	);
}

/// Config helper for branch strategy tests.
fn branch_strategy_config() -> GitConfig {
	GitConfig {
		enabled: Some(true),
		strategy: Some(Strategy::Branch),
		..Default::default()
	}
}

#[test]
fn prepare_dirty_tree_fails_when_git_enabled() {
	// A dirty working tree should abort the release before making any changes.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	// Make the tree dirty with an untracked file
	std::fs::write(dir.path().join("dirty.txt"), "untracked change").unwrap();

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_err());
	assert!(
		result.unwrap_err().to_string().contains("dirty"),
		"Expected 'dirty' in error message"
	);
}

#[test]
fn prepare_dirty_tree_ignored_when_no_git() {
	// --no-git bypasses the dirty tree check.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	// Dirty tree
	std::fs::write(dir.path().join("dirty.txt"), "untracked change").unwrap();

	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "prepare", "--no-git"],
		dir.path(),
	);
	assert!(
		result.is_ok(),
		"release --no-git should succeed even with dirty tree: {result:?}"
	);
}

#[test]
fn prepare_push_strategy_commits_and_pushes() {
	// Push strategy: commit is pushed directly to origin.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let _remote = add_local_remote(dir.path());
	// Push initial state to remote so origin/<branch> exists
	git_push_to_remote(dir.path());

	let initial_branch = git_current_branch(dir.path());

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	// Verify the release commit was pushed to origin
	let output = Command::new("git")
		.args(["log", &format!("origin/{initial_branch}"), "--format=%s"])
		.current_dir(dir.path())
		.output()
		.expect("Failed to run git log");
	let log = String::from_utf8(output.stdout).expect("log not UTF-8");
	assert!(
		log.lines().any(|l| l.contains("chore(release):")),
		"Expected release commit on origin/{initial_branch}, got: {log}"
	);
}

#[test]
fn prepare_push_strategy_dry_run_does_not_push() {
	// Dry-run must not push (no remote → would fail if push were attempted).
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	// No remote — push would fail; this verifies dry-run doesn't push.

	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "prepare", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "dry-run should succeed: {result:?}");

	// No release commit
	let log = git_log(dir.path());
	assert!(
		!log.iter().any(|m| m.contains("chore(release):")),
		"Dry-run should not create a commit, got log: {log:?}"
	);
}

#[test]
fn prepare_branch_strategy_creates_branch_and_returns() {
	// Branch strategy: release commit lands on a new branch; current branch is restored.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, branch_strategy_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let initial_branch = git_current_branch(dir.path());
	let expected_release_branch = format!("chronicle-release/{initial_branch}");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	// Current branch is back to original
	assert_eq!(
		git_current_branch(dir.path()),
		initial_branch,
		"Should have returned to original branch"
	);

	// Release branch exists locally
	assert!(
		git_local_branch_exists(dir.path(), &expected_release_branch),
		"Release branch '{expected_release_branch}' should exist locally"
	);

	// Release commit is on the release branch
	let output = Command::new("git")
		.args(["log", &expected_release_branch, "--format=%s"])
		.current_dir(dir.path())
		.output()
		.expect("Failed to run git log");
	let log = String::from_utf8(output.stdout).expect("log not UTF-8");
	assert!(
		log.lines().any(|l| l.contains("chore(release):")),
		"Release branch should contain the release commit, got: {log}"
	);
}

#[test]
fn prepare_branch_strategy_dry_run_does_not_checkout() {
	// Dry-run branch strategy must not switch branches.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, branch_strategy_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let initial_branch = git_current_branch(dir.path());

	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "prepare", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok(), "dry-run should succeed: {result:?}");

	// Still on original branch
	assert_eq!(
		git_current_branch(dir.path()),
		initial_branch,
		"Dry-run should not change the current branch"
	);
	// No release commit
	assert!(
		!git_log(dir.path())
			.iter()
			.any(|m| m.contains("chore(release):")),
		"Dry-run should not create a commit"
	);
}

#[test]
fn prepare_branch_flag_overrides_prefix() {
	// --branch overrides the computed release branch name.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, branch_strategy_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let initial_branch = git_current_branch(dir.path());

	let result = common::run_chronicle(
		[
			"chronicle",
			"--no-interactive",
			"prepare",
			"--branch",
			"custom-release-branch",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "release failed: {result:?}");

	// Back on original branch
	assert_eq!(git_current_branch(dir.path()), initial_branch);

	// Custom branch exists, not the default-named one
	assert!(
		git_local_branch_exists(dir.path(), "custom-release-branch"),
		"Custom branch should exist"
	);
	assert!(
		!git_local_branch_exists(dir.path(), &format!("chronicle-release/{initial_branch}")),
		"Default release branch should not exist when --branch is used"
	);
}

#[test]
fn prepare_git_config_old_run_until_field_fails_to_load() {
	// Old configs with run_until must produce a clear parse error.
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let config_dir = dir.path().join(".chronicle");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[git]\nenabled = true\nrun_until = \"push\"\n",
	)
	.unwrap();
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-pkg\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let result = common::run_chronicle(["chronicle", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_err(), "Expected error for old run_until field");
}
