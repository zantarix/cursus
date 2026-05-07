//! Integration tests for git lifecycle automation in the `prepare` command.

mod common;

use std::process::{Command, Stdio};

use common::{
	add_local_remote, git_current_branch, git_enabled_config, git_local_branch_exists, git_log,
	git_push_to_remote, git_tags, temp_git_repo_with_project,
	temp_real_git_repo_with_cargo_workspace, temp_real_git_repo_with_config, write_changeset,
};
use cursus::filesystem::LocalFilesystem;
use cursus::model::config::PackageManager;
use cursus::model::config::{GitConfig, Strategy, TagFormat};

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

#[tokio::test]
async fn prepare_git_disabled_by_default() {
	// Without [git] enabled, a release should succeed without touching git state.
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	write_changeset(
		dir.path(),
		"change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nA feature\n",
	);

	// Uses a fake .git dir, not a real repo — verifies nothing panics when enabled=false.
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());
}

#[tokio::test]
async fn prepare_git_creates_commit() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nA feature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "release failed: {result:?}");

	let log = git_log(dir.path());
	assert!(
		log.iter().any(|msg| msg.contains("ci(release):")),
		"Expected a release commit, got log: {log:?}"
	);
	assert!(
		log[0].contains("ci(release): version packages"),
		"Latest commit should be the release commit, got: {}",
		log[0]
	);
}

#[tokio::test]
async fn prepare_git_custom_commit_message() {
	let config = git_enabled_config()
		.with_prepare_commit_message("chore(ci): bump package versions".to_string());
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, config).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nA fix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "release failed: {result:?}");

	let log = git_log(dir.path());
	assert!(
		log[0].contains("chore(ci): bump package versions"),
		"Commit should use the configured message, got: {}",
		log[0]
	);
}

#[tokio::test]
async fn prepare_git_does_not_create_tags() {
	// Tags are now created during publish, not release.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nA fix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "release failed: {result:?}");

	// Release no longer creates tags — publish does.
	assert!(
		git_tags(dir.path()).is_empty(),
		"Release should not create tags (tags are created on publish), got: {:?}",
		git_tags(dir.path())
	);
}

#[tokio::test]
async fn prepare_git_tag_format_config_no_tags_at_release() {
	// Tag format only affects publish step now; release just commits.
	let config = GitConfig::enabled_config().with_tag_format(TagFormat::Prefixed);
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, config).await;
	setup_single_cargo_package(dir.path(), "solo", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nsolo = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());

	assert!(
		git_tags(dir.path()).is_empty(),
		"Release should not create tags regardless of tag_format, got: {:?}",
		git_tags(dir.path())
	);
}

#[tokio::test]
async fn prepare_git_multi_package_creates_single_commit() {
	// When multiple packages are released simultaneously, a single commit is created.
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	)
	.await;
	write_changeset(
		dir.path(),
		"change.md",
		"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nFix and feature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "release failed: {result:?}");

	let log = git_log(dir.path());
	assert!(
		log[0].contains("ci(release): version packages"),
		"Release commit should use the standard message, got: {}",
		log[0]
	);
	// No tags at release time
	assert!(
		git_tags(dir.path()).is_empty(),
		"Release should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

#[tokio::test]
async fn prepare_no_git_flag_skips_git() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_cursus(
		["cursus", "--no-interactive", "prepare", "--no-git"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok());

	let log = git_log(dir.path());
	assert!(
		!log.iter().any(|msg| msg.contains("ci(release):")),
		"--no-git should skip git operations, got log: {log:?}"
	);
	assert!(
		git_tags(dir.path()).is_empty(),
		"--no-git should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

#[tokio::test]
async fn prepare_git_stages_only_cursus_files() {
	// Cursus uses `git add -- <files>` for selective staging, so tracked
	// but unmodified files are never included in the release commit.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
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

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
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

#[tokio::test]
async fn prepare_git_filesystem_changes_persist_after_lifecycle() {
	// Version bumps (filesystem) happen before git ops; a successful git lifecycle
	// should leave the bumped version in place.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());

	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("1.0.1"),
		"Version should be bumped to 1.0.1, got: {cargo_toml}"
	);
}

#[tokio::test]
async fn prepare_dry_run_with_git_enabled_does_not_create_commit_or_tags() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_cursus(
		["cursus", "--no-interactive", "prepare", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok());

	let log = git_log(dir.path());
	assert!(
		!log.iter().any(|msg| msg.contains("ci(release):")),
		"Dry run should not create a commit, got log: {log:?}"
	);
	assert!(
		git_tags(dir.path()).is_empty(),
		"Dry run should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

#[tokio::test]
async fn prepare_git_extra_files_are_staged() {
	// An extra file produced by a custom lock_command should be staged in the release
	// commit. We use an npm project with a lock_command that writes custom.lock so the
	// file is created WITHIN cursus's execution (after the dirty-tree check).
	let git_config = GitConfig::enabled_config().with_extra_files(vec!["custom.lock".to_string()]);
	let dir = temp_real_git_repo_with_config(PackageManager::Npm, git_config).await;

	// Write config with a lock_command that produces custom.lock during the release.
	std::fs::write(
		dir.path().join(".cursus").join("config.toml"),
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

	// Tree is clean; lock_command will write custom.lock during cursus's execution.
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
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
	GitConfig::enabled_config().with_strategy(Strategy::Branch)
}

#[tokio::test]
async fn prepare_dirty_tree_fails_when_git_enabled() {
	// A dirty working tree should abort the release before making any changes.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	// Make the tree dirty with an untracked file
	std::fs::write(dir.path().join("dirty.txt"), "untracked change").unwrap();

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_err());
	assert!(
		result.unwrap_err().to_string().contains("dirty"),
		"Expected 'dirty' in error message"
	);
}

#[tokio::test]
async fn prepare_dirty_tree_ignored_when_no_git() {
	// --no-git bypasses the dirty tree check.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	// Dirty tree
	std::fs::write(dir.path().join("dirty.txt"), "untracked change").unwrap();

	let result = common::run_cursus(
		["cursus", "--no-interactive", "prepare", "--no-git"],
		dir.path(),
	)
	.await;
	assert!(
		result.is_ok(),
		"release --no-git should succeed even with dirty tree: {result:?}"
	);
}

#[tokio::test]
async fn prepare_push_strategy_commits_and_pushes() {
	// Push strategy: commit is pushed directly to origin.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
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

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "release failed: {result:?}");

	// Verify the release commit was pushed to origin
	let output = Command::new("git")
		.args(["log", &format!("origin/{initial_branch}"), "--format=%s"])
		.current_dir(dir.path())
		.output()
		.expect("Failed to run git log");
	let log = String::from_utf8(output.stdout).expect("log not UTF-8");
	assert!(
		log.lines().any(|l| l.contains("ci(release):")),
		"Expected release commit on origin/{initial_branch}, got: {log}"
	);
}

#[tokio::test]
async fn prepare_push_strategy_dry_run_does_not_push() {
	// Dry-run must not push (no remote → would fail if push were attempted).
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	// No remote — push would fail; this verifies dry-run doesn't push.

	let result = common::run_cursus(
		["cursus", "--no-interactive", "prepare", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "dry-run should succeed: {result:?}");

	// No release commit
	let log = git_log(dir.path());
	assert!(
		!log.iter().any(|m| m.contains("ci(release):")),
		"Dry-run should not create a commit, got log: {log:?}"
	);
}

#[tokio::test]
async fn prepare_branch_strategy_creates_branch_and_returns() {
	// Branch strategy: release commit lands on a new branch; current branch is restored.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, branch_strategy_config()).await;
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
	let expected_release_branch = format!("cursus-release/{initial_branch}");

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
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
		log.lines().any(|l| l.contains("ci(release):")),
		"Release branch should contain the release commit, got: {log}"
	);
}

#[tokio::test]
async fn prepare_branch_strategy_dry_run_does_not_checkout() {
	// Dry-run branch strategy must not switch branches.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, branch_strategy_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let initial_branch = git_current_branch(dir.path());

	let result = common::run_cursus(
		["cursus", "--no-interactive", "prepare", "--dry-run"],
		dir.path(),
	)
	.await;
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
			.any(|m| m.contains("ci(release):")),
		"Dry-run should not create a commit"
	);
}

#[tokio::test]
async fn prepare_branch_flag_overrides_prefix() {
	// --branch overrides the computed release branch name.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, branch_strategy_config()).await;
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

	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--branch",
			"custom-release-branch",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "release failed: {result:?}");

	// Back on original branch
	assert_eq!(git_current_branch(dir.path()), initial_branch);

	// Custom branch exists, not the default-named one
	assert!(
		git_local_branch_exists(dir.path(), "custom-release-branch"),
		"Custom branch should exist"
	);
	assert!(
		!git_local_branch_exists(dir.path(), &format!("cursus-release/{initial_branch}")),
		"Default release branch should not exist when --branch is used"
	);
}

/// `prepare --branch` with the branch strategy must NOT warn "no effect".
///
/// This guards against a mutation that inverts the strategy equality check (`== Push` →
/// `!= Push`), which would incorrectly warn whenever `--branch` is used with branch strategy.
#[tokio::test]
async fn prepare_branch_arg_with_branch_strategy_no_warning() {
	use cursus::test_logging::{init_test_logger, take_logs};
	init_test_logger();
	let _ = take_logs();

	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, branch_strategy_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--branch",
			"custom-release-branch",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "release failed: {result:?}");

	let logs = take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("no effect") && m.contains("branch")),
		"Should not warn when --branch is used with branch strategy, got: {logs:?}"
	);
}

#[tokio::test]
async fn prepare_git_config_old_run_until_field_fails_to_load() {
	// Old configs with run_until must produce a clear parse error.
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let config_dir = dir.path().join(".cursus");
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

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_err(), "Expected error for old run_until field");
}

#[tokio::test]
async fn prepare_branch_strategy_rerun_is_idempotent() {
	// Running prepare twice with branch strategy should succeed both times.
	// The second run resets the existing release branch and force-pushes it.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, branch_strategy_config()).await;
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
	let expected_release_branch = format!("cursus-release/{initial_branch}");

	// First run
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "first prepare failed: {result:?}");

	// Back on original branch after first run
	assert_eq!(
		git_current_branch(dir.path()),
		initial_branch,
		"Should have returned to original branch after first run"
	);

	// Second run — should succeed even though the release branch already exists
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "second prepare failed: {result:?}");

	// Still on original branch after second run
	assert_eq!(
		git_current_branch(dir.path()),
		initial_branch,
		"Should be on original branch after second run"
	);

	// Release branch still exists with the release commit
	assert!(
		git_local_branch_exists(dir.path(), &expected_release_branch),
		"Release branch '{expected_release_branch}' should still exist after second run"
	);

	let output = Command::new("git")
		.args(["log", &expected_release_branch, "--format=%s"])
		.current_dir(dir.path())
		.output()
		.expect("Failed to run git log");
	let log = String::from_utf8(output.stdout).expect("log not UTF-8");
	assert!(
		log.lines().any(|l| l.contains("ci(release):")),
		"Release branch should contain the release commit after second run, got: {log}"
	);

	// Verify the version was correctly bumped to 0.2.0 on the release branch
	let cargo_toml = Command::new("git")
		.args(["show", &format!("{expected_release_branch}:Cargo.toml")])
		.current_dir(dir.path())
		.output()
		.expect("Failed to read Cargo.toml from release branch");
	let cargo_toml_content = String::from_utf8(cargo_toml.stdout).expect("Not UTF-8");
	assert!(
		cargo_toml_content.contains("version = \"0.2.0\""),
		"Release branch Cargo.toml should have bumped version to 0.2.0, got: {cargo_toml_content}"
	);
}

const PR_JSON: &str = r#"{"url": "https://api.github.com/repos/acme/app/pulls/7", "id": 1, "number": 7, "html_url": "https://github.com/acme/app/pull/7", "head": {"ref": "cursus-release/main", "sha": "abc123"}, "base": {"ref": "main", "sha": "def456"}}"#;
const PR_LIST_JSON: &str = r#"[{"url": "https://api.github.com/repos/acme/app/pulls/7", "id": 1, "number": 7, "html_url": "https://github.com/acme/app/pull/7", "head": {"ref": "cursus-release/main", "sha": "abc123"}, "base": {"ref": "main", "sha": "def456"}}]"#;

fn make_github_test_env(api_url: &str, dir: &std::path::Path) -> cursus::Env {
	use std::sync::Arc;

	use cursus::command::RealCommandRunner;
	use cursus::github::OctocrabGitHubClient;
	use cursus::github::client::CodeForgeClient;
	use cursus::github::remote::GitHubRepo;
	use cursus::path::AbsolutePath;

	let client = Arc::new(OctocrabGitHubClient::new(
		octocrab::Octocrab::builder()
			.personal_token("test-token".to_string())
			.base_uri(api_url)
			.unwrap()
			.build()
			.unwrap(),
		GitHubRepo::new("acme", "app").unwrap(),
	)) as Arc<dyn CodeForgeClient>;
	let runner = Arc::new(RealCommandRunner) as Arc<dyn cursus::command::CommandRunner>;
	let path = AbsolutePath::new(dir).unwrap();
	let git = Arc::new(cursus::git::GitWorkdir::new(Arc::clone(&runner), path));
	cursus::Env::new(runner, Arc::new(LocalFilesystem), git).with_code_forge_client(client)
}

async fn run_prepare(
	api_url: &str,
	dir: &std::path::Path,
) -> anyhow::Result<std::process::ExitCode> {
	let env = make_github_test_env(api_url, dir);
	let config = cursus::model::config::load(env.fs(), env.git().path())
		.await
		.unwrap();
	let cli: cursus::cli::Cli = clap::Parser::parse_from(["cursus", "--no-interactive", "prepare"]);
	cursus::run(cli, env, config).await
}

async fn setup_github_pr_test_repo() -> (tempfile::TempDir, tempfile::TempDir) {
	use cursus::model::config::{CargoConfig, Config, GitHubConfig};

	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, branch_strategy_config()).await;
	let cfg_env = common::test_env(dir.path());
	Config::new()
		.with_cargo(CargoConfig::enabled())
		.with_git(branch_strategy_config())
		.with_github(
			GitHubConfig::enabled_config()
				.with_owner("acme".into())
				.with_repo("app".into()),
		)
		.save(cfg_env.fs(), cfg_env.git().path())
		.await
		.unwrap();
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());
	(dir, remote)
}

#[tokio::test]
async fn prepare_branch_strategy_with_github_upserts_pr_on_rerun() {
	// Full end-to-end test: prepare with branch strategy + GitHub enabled.
	// First run creates a PR; second run finds the existing PR and updates it.
	use httpmock::prelude::*;

	let server = MockServer::start();
	let api_url = server.base_url();
	let (dir, _remote) = setup_github_pr_test_repo().await;

	let initial_branch = git_current_branch(dir.path());
	let release_branch = format!("cursus-release/{initial_branch}");
	let head_param = format!("acme:{release_branch}");

	// ── First run: no existing PR → find returns empty → create PR ───────────
	let mut mock_find_empty = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/acme/app/pulls")
			.query_param("head", &head_param)
			.query_param("state", "open");
		then.status(200)
			.header("Content-Type", "application/json")
			.body("[]");
	});
	let mut mock_create = server.mock(|when, then| {
		when.method(POST).path("/repos/acme/app/pulls");
		then.status(201)
			.header("Content-Type", "application/json")
			.body(PR_JSON);
	});
	run_prepare(&api_url, dir.path())
		.await
		.expect("first prepare failed");
	mock_find_empty.assert_calls(1);
	mock_create.assert_calls(1);
	mock_find_empty.delete();
	mock_create.delete();

	// ── Second run: existing PR found → find returns PR #7 → update PR ───────
	let mock_find_existing = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/acme/app/pulls")
			.query_param("head", &head_param)
			.query_param("state", "open");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(PR_LIST_JSON);
	});
	let mock_update = server.mock(|when, then| {
		when.method(PATCH).path("/repos/acme/app/pulls/7");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(PR_JSON);
	});
	run_prepare(&api_url, dir.path())
		.await
		.expect("second prepare (update) failed");
	mock_find_existing.assert_calls(1);
	mock_update.assert_calls(1);
}

// ── Commit reference integration tests ────────────────────────────────────────

fn extract_bracket_content(line: &str, start: usize) -> Option<&str> {
	line[start + 1..]
		.find(']')
		.map(|end| &line[start + 1..start + 1 + end])
}

#[tokio::test]
async fn prepare_with_git_adds_commit_references() {
	// A changeset committed to the repo should get its SHA added to the changelog.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nA feature\n",
	);
	// Commit the changeset — this is what log_added_commit will find.
	git_commit_all(dir.path(), "feat: add feature");

	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "prepare failed: {result:?}");

	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
	// Should contain a 7-character hex SHA reference like [abc1234] on a bullet line
	let has_sha_ref = changelog.lines().any(|line| {
		// Extract content between [ and ] and verify it's a 7-char hex string
		line.starts_with("- ")
			&& line
				.find('[')
				.and_then(|start| extract_bracket_content(line, start))
				.map(|hash| hash.len() == 7 && hash.chars().all(|c| c.is_ascii_hexdigit()))
				.unwrap_or(false)
	});
	assert!(
		has_sha_ref,
		"Expected 7-char hex SHA reference on a bullet line in changelog, got:\n{changelog}"
	);
}

#[tokio::test]
async fn prepare_with_squash_merge_includes_pr_number() {
	// A changeset introduced by a squash-merge commit (subject has `(#NN)`) should
	// include the PR number in the changelog entry.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix from PR\n",
	);
	// Commit with a squash-merge-style subject
	git_commit_all(dir.path(), "fix: resolve issue (#42)");

	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "prepare failed: {result:?}");

	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
	assert!(
		changelog.contains("via #42"),
		"Expected PR reference 'via #42' in changelog, got:\n{changelog}"
	);
}

#[tokio::test]
async fn prepare_with_merge_commit_includes_pr_number() {
	// A changeset introduced by a classic merge commit (subject starts with
	// "Merge pull request #NNN") should include the PR number in the changelog entry.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nFeature from PR\n",
	);
	git_commit_all(
		dir.path(),
		"Merge pull request #99 from user/feature-branch",
	);

	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "prepare failed: {result:?}");

	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
	assert!(
		changelog.contains("via #99"),
		"Expected PR reference 'via #99' in changelog, got:\n{changelog}"
	);
}

#[tokio::test]
async fn prepare_without_git_no_references() {
	// When git is disabled, no commit references should appear in the changelog.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config()).await;
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "fix: something (#10)");

	let result = common::run_cursus(
		["cursus", "--no-interactive", "prepare", "--no-git"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "prepare --no-git failed: {result:?}");

	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
	// When git is disabled no SHA references should be added
	assert!(
		!changelog.contains("via #"),
		"Expected no PR reference when --no-git, got:\n{changelog}"
	);
	// Also no SHA bracket references
	let has_sha_ref = changelog.lines().any(|l| {
		l.contains('[') && l.contains(']') && !l.starts_with("##") && !l.starts_with("###")
	});
	assert!(
		!has_sha_ref,
		"Expected no SHA reference in changelog when --no-git, got:\n{changelog}"
	);
}
