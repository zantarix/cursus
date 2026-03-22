//! Integration tests for `cursus change --auto`.

mod common;

use std::process::ExitCode;

use common::{
	add_local_remote, git_cmd, git_current_branch, git_log, git_push_to_remote,
	git_set_remote_head, temp_real_git_repo_with_cargo_workspace, temp_real_git_repo_with_project,
};
use cursus::model::config::{CargoConfig, GitConfig, PackageManager};

/// Sets up a real git repo with a remote and origin/HEAD configured, ready for --auto tests.
///
/// Returns `(repo_dir, remote_dir)`. Keep `remote_dir` alive for the test duration.
fn setup_auto_repo(pm: PackageManager) -> (tempfile::TempDir, tempfile::TempDir) {
	let dir = temp_real_git_repo_with_project(pm);
	let remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());
	let branch = git_current_branch(dir.path());
	git_set_remote_head(dir.path(), &branch);
	(dir, remote)
}

/// Creates a conventional commit on the current branch of the repo.
fn make_conventional_commit(dir: &std::path::Path, file: &str, msg: &str) {
	std::fs::write(dir.join(file), format!("// changed by: {msg}")).unwrap();
	git_cmd(dir, &["add", "."]);
	git_cmd(dir, &["commit", "-m", msg]);
}

/// Finds the changeset `.md` files in the `.cursus` directory.
fn find_changesets(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
	let cursus_dir = dir.join(".cursus");
	if !cursus_dir.is_dir() {
		return vec![];
	}
	std::fs::read_dir(&cursus_dir)
		.unwrap()
		.filter_map(|e| e.ok())
		.filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
		.map(|e| e.path())
		.collect()
}

#[test]
fn change_auto_fix_commit_creates_patch_changeset() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "fix: resolve null pointer");

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let changesets = find_changesets(dir.path());
	assert_eq!(changesets.len(), 1, "Expected one changeset file");
	let content = std::fs::read_to_string(&changesets[0]).unwrap();
	assert!(
		content.contains("test-project = \"patch\""),
		"Expected patch changeset, got: {content}"
	);
	assert!(
		content.contains("resolve null pointer"),
		"Expected description in changeset, got: {content}"
	);
}

#[test]
fn change_auto_feat_commit_creates_minor_changeset() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "feat: add new feature");

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_ok());
	let changesets = find_changesets(dir.path());
	assert_eq!(changesets.len(), 1);
	let content = std::fs::read_to_string(&changesets[0]).unwrap();
	assert!(
		content.contains("test-project = \"minor\""),
		"Expected minor changeset, got: {content}"
	);
}

#[test]
fn change_auto_breaking_bang_creates_major_changeset() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "feat!: redesign public API");

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_ok());
	let changesets = find_changesets(dir.path());
	assert_eq!(changesets.len(), 1);
	let content = std::fs::read_to_string(&changesets[0]).unwrap();
	assert!(
		content.contains("test-project = \"major\""),
		"Expected major changeset, got: {content}"
	);
}

#[test]
fn change_auto_chore_commit_skips_changeset() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "chore: update dependencies");

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
	let changesets = find_changesets(dir.path());
	assert!(
		changesets.is_empty(),
		"Expected no changeset for chore commit"
	);
}

#[test]
fn change_auto_multiple_commits_skips() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "fix: first fix");
	make_conventional_commit(dir.path(), "src/lib.rs", "fix: second fix");

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
	let changesets = find_changesets(dir.path());
	assert!(
		changesets.is_empty(),
		"Expected no changeset with multiple commits"
	);
}

#[test]
fn change_auto_zero_commits_ahead_fails() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_err());
	let err = result.unwrap_err().to_string();
	assert!(
		err.contains("No commits ahead"),
		"Expected 'No commits ahead' error, got: {err}"
	);
}

#[test]
fn change_auto_invalid_commit_message_fails() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "not a conventional commit");

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_err());
}

#[test]
fn change_auto_dry_run_writes_nothing() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "fix: fix a bug");

	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"--dry-run",
			"change",
			"--auto",
			"--no-git",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
	let changesets = find_changesets(dir.path());
	assert!(
		changesets.is_empty(),
		"Expected no changeset written in dry-run mode"
	);
}

#[test]
fn change_auto_changeset_includes_body() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	std::fs::write(dir.path().join("src/lib.rs"), "// modified").unwrap();
	git_cmd(dir.path(), &["add", "."]);
	git_cmd(
		dir.path(),
		&[
			"commit",
			"-m",
			"fix: fix the thing\n\nThis fixes the issue described in #42.",
		],
	);

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_ok());
	let changesets = find_changesets(dir.path());
	assert_eq!(changesets.len(), 1);
	let content = std::fs::read_to_string(&changesets[0]).unwrap();
	assert!(
		content.contains("fix the thing"),
		"Expected description in changeset, got: {content}"
	);
	assert!(
		content.contains("fixes the issue"),
		"Expected body in changeset, got: {content}"
	);
}

#[test]
fn change_auto_no_matching_projects_skips() {
	// Use a cargo workspace with a package in a subfolder; modify only a file
	// at the root (outside any package) so no project matches.
	let dir = temp_real_git_repo_with_cargo_workspace(&[("pkg-a", "0.1.0")], GitConfig::default());
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());
	let branch = git_current_branch(dir.path());
	git_set_remote_head(dir.path(), &branch);

	std::fs::write(dir.path().join(".github-actions.yml"), "# ci").unwrap();
	git_cmd(dir.path(), &["add", "."]);
	git_cmd(dir.path(), &["commit", "-m", "fix: update ci config"]);

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
	let changesets = find_changesets(dir.path());
	assert!(
		changesets.is_empty(),
		"Expected no changeset when no project matched"
	);
}

#[test]
fn change_auto_monorepo_only_affected_project_in_changeset() {
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "0.1.0"), ("pkg-b", "0.1.0")],
		GitConfig::default(),
	);
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());
	let branch = git_current_branch(dir.path());
	git_set_remote_head(dir.path(), &branch);

	std::fs::write(dir.path().join("pkg-a/src/lib.rs"), "// modified").unwrap();
	git_cmd(dir.path(), &["add", "."]);
	git_cmd(dir.path(), &["commit", "-m", "fix: fix pkg-a only"]);

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_ok());
	let changesets = find_changesets(dir.path());
	assert_eq!(changesets.len(), 1);
	let content = std::fs::read_to_string(&changesets[0]).unwrap();
	assert!(
		content.contains("pkg-a"),
		"Expected pkg-a in changeset, got: {content}"
	);
	assert!(
		!content.contains("pkg-b"),
		"Expected pkg-b NOT in changeset, got: {content}"
	);
}

#[test]
fn change_auto_no_git_skips_commit() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "fix: a small bugfix");
	let commits_before = git_log(dir.path()).len();

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto", "--no-git"],
		dir.path(),
	);

	assert!(result.is_ok());
	let changesets = find_changesets(dir.path());
	assert_eq!(changesets.len(), 1, "Changeset should be written");

	let commits_after = git_log(dir.path()).len();
	assert_eq!(
		commits_after, commits_before,
		"Expected no new commit with --no-git"
	);
}

#[test]
fn change_auto_with_git_commits_and_pushes() {
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "fix: another fix");

	let config =
		cursus::model::config::Config::new(&cursus::path::AbsolutePath::new(dir.path()).unwrap())
			.with_cargo(CargoConfig::enabled())
			.with_git(GitConfig::enabled_config());
	config.with_env(common::test_env()).save().unwrap();

	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto"],
		dir.path(),
	);

	assert!(result.is_ok(), "Expected success, got: {:?}", result.err());
	let changesets = find_changesets(dir.path());
	assert_eq!(changesets.len(), 1, "Expected one changeset");

	let log = git_log(dir.path());
	assert!(
		log.iter().any(|s| s.contains("add changeset")),
		"Expected 'add changeset' commit in log, got: {log:?}"
	);
}

#[test]
fn change_auto_conflicts_with_change_type() {
	let (success, _stdout, stderr) = common::run_cursus_subprocess(
		&["--no-interactive", "change", "--auto", "-t", "minor"],
		std::env::temp_dir().as_path(),
	);
	assert!(!success);
	assert!(
		stderr.contains("auto") || stderr.contains("change-type") || stderr.contains("conflict"),
		"Expected conflict error, got: {stderr}"
	);
}

#[test]
fn change_auto_conflicts_with_message() {
	let (success, _stdout, stderr) = common::run_cursus_subprocess(
		&["--no-interactive", "change", "--auto", "-m", "hello"],
		std::env::temp_dir().as_path(),
	);
	assert!(!success);
	assert!(
		stderr.contains("auto") || stderr.contains("message") || stderr.contains("conflict"),
		"Expected conflict error, got: {stderr}"
	);
}

/// `--auto` with git disabled in config (no `--no-git` flag) must write changeset but not commit.
///
/// Guards `&&`→`||` on `commit_to_git = config.git.enabled() && !args.no_git`: if that
/// becomes `||`, a disabled-git config with no `--no-git` flag would still commit
/// (since `!args.no_git` is true), creating an unexpected commit.
#[test]
fn change_auto_git_disabled_in_config_no_commit() {
	// setup_auto_repo uses temp_real_git_repo_with_project which saves config WITHOUT git enabled.
	let (dir, _remote) = setup_auto_repo(PackageManager::Cargo);
	make_conventional_commit(dir.path(), "src/lib.rs", "fix: small fix for config test");
	let commits_before = git_log(dir.path()).len();

	// No --no-git flag — git commit_to_git must still be false because config disables git.
	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "--auto"],
		dir.path(),
	);

	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
	let changesets = find_changesets(dir.path());
	assert_eq!(changesets.len(), 1, "Changeset should be written");

	let commits_after = git_log(dir.path()).len();
	assert_eq!(
		commits_after, commits_before,
		"Expected no new commit when git disabled in config (without --no-git flag)"
	);
}
