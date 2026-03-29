//! Integration tests for `GitWorkdir` that exercise real git operations.
//!
//! Each test sets up a temporary git repository using the real git binary via
//! [`RealCommandRunner`], calls `GitWorkdir` methods, and verifies the resulting
//! repository state.  Push operations use a local bare repository as the remote
//! so that no network access is required.

use std::path::Path;
use std::sync::Arc;

use tempfile::TempDir;

use crate::command::{CommandRunner, RealCommandRunner};
use crate::git::Git as _;
use crate::path::AbsolutePath;

// --- helpers ---

/// Runs a raw git command for test setup.  Panics on failure.
fn git_cmd(dir: &Path, args: &[&str]) {
	let output = std::process::Command::new("git")
		.args(args)
		.current_dir(dir)
		.output()
		.unwrap_or_else(|e| panic!("failed to spawn git: {e}"));
	if !output.status.success() {
		panic!(
			"git {} failed:\n{}",
			args.join(" "),
			String::from_utf8_lossy(&output.stderr)
		);
	}
}

fn make_workdir(dir: &TempDir) -> super::GitWorkdir {
	let path = AbsolutePath::new(dir.path()).unwrap();
	super::GitWorkdir::new(Arc::new(RealCommandRunner) as Arc<dyn CommandRunner>, path)
}

/// Returns the current branch name for a repository directory using a raw git call.
fn current_branch_raw(dir: &Path) -> String {
	let output = std::process::Command::new("git")
		.args(["rev-parse", "--abbrev-ref", "HEAD"])
		.current_dir(dir)
		.output()
		.expect("git rev-parse");
	String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Returns the HEAD SHA for a repository directory using a raw git call.
fn head_sha(dir: &Path) -> String {
	let output = std::process::Command::new("git")
		.args(["rev-parse", "HEAD"])
		.current_dir(dir)
		.output()
		.expect("git rev-parse HEAD");
	String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Creates a git repo with one initial commit and returns `(TempDir, GitWorkdir)`.
///
/// The caller must keep the `TempDir` alive for the duration of the test.
async fn setup_repo() -> (TempDir, super::GitWorkdir) {
	let dir = tempfile::tempdir().expect("tempdir");
	git_cmd(dir.path(), &["init"]);
	git_cmd(dir.path(), &["config", "user.email", "test@cursus.test"]);
	git_cmd(dir.path(), &["config", "user.name", "Cursus Test"]);
	git_cmd(dir.path(), &["config", "commit.gpgsign", "false"]);
	git_cmd(dir.path(), &["config", "tag.gpgsign", "false"]);

	let git = make_workdir(&dir);

	let readme = dir.path().join("README.md");
	std::fs::write(&readme, "# test").unwrap();
	git.add(&[readme]).await.unwrap();
	git.commit("chore: initial commit").await.unwrap();

	(dir, git)
}

/// Creates a git repo paired with a local bare "remote" and returns
/// `(bare_dir, work_dir, GitWorkdir)`.
///
/// The initial commit is already pushed so that subsequent `push` calls do not
/// need to establish the upstream.  Both `TempDir` values must be kept alive.
async fn setup_repo_with_remote() -> (TempDir, TempDir, super::GitWorkdir) {
	let work_dir = tempfile::tempdir().expect("work tempdir");
	let bare_dir = tempfile::tempdir().expect("bare tempdir");

	git_cmd(work_dir.path(), &["init"]);
	git_cmd(
		work_dir.path(),
		&["config", "user.email", "test@cursus.test"],
	);
	git_cmd(work_dir.path(), &["config", "user.name", "Cursus Test"]);
	git_cmd(work_dir.path(), &["config", "commit.gpgsign", "false"]);
	git_cmd(work_dir.path(), &["config", "tag.gpgsign", "false"]);

	let git = make_workdir(&work_dir);

	let readme = work_dir.path().join("README.md");
	std::fs::write(&readme, "# test").unwrap();
	git.add(&[readme]).await.unwrap();
	git.commit("chore: initial commit").await.unwrap();

	// Create a bare repo and wire it up as `origin`.
	git_cmd(bare_dir.path(), &["init", "--bare"]);
	let bare_path = bare_dir.path().to_string_lossy().into_owned();
	git_cmd(work_dir.path(), &["remote", "add", "origin", &bare_path]);

	// Push the initial commit and record upstream tracking.
	let branch = current_branch_raw(work_dir.path());
	git_cmd(
		work_dir.path(),
		&["push", "--set-upstream", "origin", &branch],
	);

	(bare_dir, work_dir, git)
}

// --- add ---

#[tokio::test]
async fn add_stages_file_visible_in_status() {
	let (dir, git) = setup_repo().await;
	let new_file = dir.path().join("hello.txt");
	std::fs::write(&new_file, "hello").unwrap();

	// Before staging the file should show as dirty.
	assert!(
		git.is_dirty().await.unwrap(),
		"untracked file should make repo dirty"
	);

	git.add(&[new_file]).await.unwrap();

	// After staging the repo is still dirty (staged but uncommitted).
	assert!(
		git.is_dirty().await.unwrap(),
		"staged file should still be dirty"
	);
}

#[tokio::test]
async fn add_empty_list_leaves_repo_clean() {
	let (_dir, git) = setup_repo().await;
	git.add(&[]).await.unwrap();
	assert!(!git.is_dirty().await.unwrap(), "repo should still be clean");
}

// --- commit ---

#[tokio::test]
async fn commit_creates_commit_with_correct_message() {
	let (dir, git) = setup_repo().await;
	let file = dir.path().join("feature.txt");
	std::fs::write(&file, "content").unwrap();
	git.add(&[file]).await.unwrap();
	git.commit("feat: add feature").await.unwrap();
	assert_eq!(git.log_message("HEAD").await.unwrap(), "feat: add feature");
}

#[tokio::test]
async fn commit_clears_staged_changes() {
	let (dir, git) = setup_repo().await;
	let file = dir.path().join("feature.txt");
	std::fs::write(&file, "content").unwrap();
	git.add(&[file]).await.unwrap();
	git.commit("feat: add feature").await.unwrap();
	assert!(
		!git.is_dirty().await.unwrap(),
		"working tree should be clean after commit"
	);
}

// --- tag / tag_exists / delete_tag ---

#[tokio::test]
async fn tag_creates_annotated_tag_visible_via_tag_exists() {
	let (_dir, git) = setup_repo().await;
	git.tag("v1.0.0", "Release 1.0.0").await.unwrap();
	assert!(git.tag_exists("v1.0.0").await.unwrap(), "tag should exist");
}

#[tokio::test]
async fn tag_exists_returns_false_for_absent_tag() {
	let (_dir, git) = setup_repo().await;
	assert!(
		!git.tag_exists("v99.0.0").await.unwrap(),
		"non-existent tag should not exist"
	);
}

#[tokio::test]
async fn delete_tag_removes_existing_tag() {
	let (_dir, git) = setup_repo().await;
	git.tag("v1.0.0", "Release 1.0.0").await.unwrap();
	git.delete_tag("v1.0.0").await.unwrap();
	assert!(
		!git.tag_exists("v1.0.0").await.unwrap(),
		"tag should be gone after delete"
	);
}

#[tokio::test]
async fn delete_tag_fails_for_nonexistent_tag() {
	let (_dir, git) = setup_repo().await;
	let result = git.delete_tag("nonexistent-tag").await;
	assert!(result.is_err(), "deleting an absent tag must error");
	let msg = result.unwrap_err().to_string();
	assert!(msg.contains("git tag -d failed"), "got: {msg}");
}

// --- current_branch ---

#[tokio::test]
async fn current_branch_returns_some_after_init() {
	let (_dir, git) = setup_repo().await;
	let branch = git.current_branch().await.unwrap();
	assert!(branch.is_some(), "expected a branch name after init");
	let name = branch.unwrap();
	assert!(!name.is_empty(), "branch name should not be empty");
}

#[tokio::test]
async fn current_branch_returns_none_in_detached_head() {
	let (dir, git) = setup_repo().await;
	let sha = head_sha(dir.path());
	// Detach HEAD by checking out the raw SHA.
	git_cmd(dir.path(), &["checkout", &sha]);
	assert_eq!(
		git.current_branch().await.unwrap(),
		None,
		"detached HEAD should yield None"
	);
}

// --- checkout ---

#[tokio::test]
async fn checkout_switches_to_existing_branch() {
	let (dir, git) = setup_repo().await;
	// Capture the default branch name before creating a new one.
	let default = current_branch_raw(dir.path());
	git_cmd(dir.path(), &["checkout", "-b", "feature"]);
	// Return to the default branch so that checkout("feature") is a real switch.
	git_cmd(dir.path(), &["checkout", &default]);

	git.checkout("feature").await.unwrap();
	assert_eq!(
		git.current_branch().await.unwrap(),
		Some("feature".to_string()),
		"should be on feature branch"
	);
}

#[tokio::test]
async fn checkout_fails_for_nonexistent_branch() {
	let (_dir, git) = setup_repo().await;
	let result = git.checkout("no-such-branch").await;
	assert!(result.is_err(), "checkout of absent branch must error");
	let msg = result.unwrap_err().to_string();
	assert!(msg.contains("git checkout failed"), "got: {msg}");
}

// --- checkout_or_reset_branch ---

#[tokio::test]
async fn checkout_or_reset_branch_creates_new_branch() {
	let (_dir, git) = setup_repo().await;
	git.checkout_or_reset_branch("release/1.0").await.unwrap();
	assert_eq!(
		git.current_branch().await.unwrap(),
		Some("release/1.0".to_string())
	);
}

#[tokio::test]
async fn checkout_or_reset_branch_resets_to_current_head() {
	let (dir, git) = setup_repo().await;
	// Capture the default branch name before creating a new one.
	let default = current_branch_raw(dir.path());
	git_cmd(dir.path(), &["checkout", "-b", "release/1.0"]);
	git_cmd(dir.path(), &["checkout", &default]);

	// Running checkout_or_reset_branch on a branch that already exists resets it.
	git.checkout_or_reset_branch("release/1.0").await.unwrap();
	assert_eq!(
		git.current_branch().await.unwrap(),
		Some("release/1.0".to_string())
	);
}

// --- is_dirty ---

#[tokio::test]
async fn is_dirty_returns_false_for_clean_repo() {
	let (_dir, git) = setup_repo().await;
	assert!(!git.is_dirty().await.unwrap());
}

#[tokio::test]
async fn is_dirty_returns_true_for_modified_file() {
	let (dir, git) = setup_repo().await;
	std::fs::write(dir.path().join("README.md"), "changed").unwrap();
	assert!(git.is_dirty().await.unwrap());
}

// --- log_message / log_subject ---

#[tokio::test]
async fn log_message_returns_full_commit_message() {
	let (dir, git) = setup_repo().await;
	let file = dir.path().join("feat.txt");
	std::fs::write(&file, "x").unwrap();
	git.add(&[file]).await.unwrap();
	git.commit("feat: something important").await.unwrap();
	assert_eq!(
		git.log_message("HEAD").await.unwrap(),
		"feat: something important"
	);
}

#[tokio::test]
async fn log_subject_returns_subject_line_only() {
	let (dir, git) = setup_repo().await;
	let file = dir.path().join("feat.txt");
	std::fs::write(&file, "x").unwrap();
	git.add(&[file]).await.unwrap();
	// Multi-paragraph message: subject + blank line + body.
	git.commit("feat: subject\n\nBody paragraph here")
		.await
		.unwrap();
	assert_eq!(git.log_subject("HEAD").await.unwrap(), "feat: subject");
}

// --- diff_tree_names ---

#[tokio::test]
async fn diff_tree_names_lists_files_changed_by_commit() {
	let (dir, git) = setup_repo().await;
	let file = dir.path().join("alpha.txt");
	std::fs::write(&file, "a").unwrap();
	git.add(&[file]).await.unwrap();
	git.commit("feat: add alpha").await.unwrap();
	let files = git.diff_tree_names("HEAD").await.unwrap();
	assert!(
		files.contains(&"alpha.txt".to_string()),
		"expected alpha.txt in: {files:?}"
	);
}

#[tokio::test]
async fn diff_tree_names_only_lists_files_from_target_commit() {
	// Verifies that diff_tree_names("HEAD") returns only the files touched by
	// the specified commit, not files from earlier commits.
	let (dir, git) = setup_repo().await;
	let file = dir.path().join("second.txt");
	std::fs::write(&file, "s").unwrap();
	git.add(&[file]).await.unwrap();
	git.commit("chore: second").await.unwrap();
	let files = git.diff_tree_names("HEAD").await.unwrap();
	assert!(files.contains(&"second.txt".to_string()), "{files:?}");
	assert!(!files.contains(&"README.md".to_string()), "{files:?}");
}

// --- rev_list_count ---

#[tokio::test]
async fn rev_list_count_returns_zero_for_empty_range() {
	let (_dir, git) = setup_repo().await;
	assert_eq!(git.rev_list_count("HEAD..HEAD").await.unwrap(), 0);
}

#[tokio::test]
async fn rev_list_count_counts_commits_after_base() {
	let (dir, git) = setup_repo().await;
	let base_sha = head_sha(dir.path());

	for i in 1..=3 {
		let file = dir.path().join(format!("file{i}.txt"));
		std::fs::write(&file, "x").unwrap();
		git.add(&[file]).await.unwrap();
		git.commit(&format!("commit {i}")).await.unwrap();
	}

	let count = git
		.rev_list_count(&format!("{base_sha}..HEAD"))
		.await
		.unwrap();
	assert_eq!(count, 3);
}

// --- diff_names ---

#[tokio::test]
async fn diff_names_returns_unstaged_modified_file() {
	let (dir, git) = setup_repo().await;
	std::fs::write(dir.path().join("README.md"), "modified content").unwrap();
	let names = git.diff_names(&[]).await.unwrap();
	assert!(
		names.contains(&"README.md".to_string()),
		"expected README.md in unstaged diff: {names:?}"
	);
}

#[tokio::test]
async fn diff_names_returns_staged_file_with_cached_flag() {
	let (dir, git) = setup_repo().await;
	let readme = dir.path().join("README.md");
	std::fs::write(&readme, "modified content").unwrap();
	git.add(&[readme]).await.unwrap();
	let names = git.diff_names(&["--cached"]).await.unwrap();
	assert!(
		names.contains(&"README.md".to_string()),
		"expected README.md in staged diff: {names:?}"
	);
}

// --- log_added_commit ---

#[tokio::test]
async fn log_added_commit_returns_sha_of_introducing_commit() {
	let (dir, git) = setup_repo().await;
	let file = dir.path().join("brand_new.txt");
	std::fs::write(&file, "new").unwrap();
	git.add(&[file]).await.unwrap();
	git.commit("feat: add brand_new").await.unwrap();

	let expected_sha = head_sha(dir.path());
	let found = git
		.log_added_commit(Path::new("brand_new.txt"))
		.await
		.unwrap();
	assert_eq!(found, Some(expected_sha));
}

#[tokio::test]
async fn log_added_commit_returns_none_for_nonexistent_file() {
	let (_dir, git) = setup_repo().await;
	let result = git
		.log_added_commit(Path::new("does-not-exist.txt"))
		.await
		.unwrap();
	assert_eq!(result, None);
}

// --- remote_origin_url ---

#[tokio::test]
async fn remote_origin_url_returns_none_when_no_remote_configured() {
	let (_dir, git) = setup_repo().await;
	assert_eq!(git.remote_origin_url().await.unwrap(), None);
}

#[tokio::test]
async fn remote_origin_url_returns_url_when_remote_is_set() {
	let (_bare, _work, git) = setup_repo_with_remote().await;
	let url = git.remote_origin_url().await.unwrap();
	assert!(url.is_some(), "expected a remote URL, got None");
}

// --- push ---

#[tokio::test]
async fn push_sends_new_commit_to_origin() {
	let (_bare, work_dir, git) = setup_repo_with_remote().await;
	let file = work_dir.path().join("extra.txt");
	std::fs::write(&file, "extra").unwrap();
	git.add(&[file]).await.unwrap();
	git.commit("feat: extra").await.unwrap();
	git.push().await.unwrap();
}

// --- push_tag ---

#[tokio::test]
async fn push_tag_sends_annotated_tag_to_origin() {
	let (_bare, _work, git) = setup_repo_with_remote().await;
	git.tag("v1.0.0", "Release 1.0.0").await.unwrap();
	git.push_tag("v1.0.0").await.unwrap();
}

#[tokio::test]
async fn push_tag_uses_tag_keyword_to_avoid_branch_ambiguity() {
	// Create a branch and a tag with the same name and confirm that push_tag
	// pushes only the tag (the bare repo receives a tag ref, not a branch ref).
	let (bare_dir, work_dir, git) = setup_repo_with_remote().await;

	// Create a branch named "v2.0.0"
	git_cmd(work_dir.path(), &["checkout", "-b", "v2.0.0"]);
	// Create an annotated tag with the same name
	git.tag("v2.0.0", "Release 2.0.0").await.unwrap();

	git.push_tag("v2.0.0").await.unwrap();

	// The bare repo must have a tag ref, not a branch ref, for v2.0.0.
	let tag_check = std::process::Command::new("git")
		.args(["show-ref", "--tags", "v2.0.0"])
		.current_dir(bare_dir.path())
		.output()
		.unwrap();
	assert!(
		tag_check.status.success(),
		"bare repo should have the tag ref"
	);

	let branch_check = std::process::Command::new("git")
		.args(["show-ref", "--heads", "v2.0.0"])
		.current_dir(bare_dir.path())
		.output()
		.unwrap();
	assert!(
		!branch_check.status.success(),
		"bare repo must NOT have a branch named v2.0.0"
	);
}

// --- force_push_branch ---

#[tokio::test]
async fn force_push_branch_pushes_new_branch_to_origin() {
	let (_bare, work_dir, git) = setup_repo_with_remote().await;
	git.checkout_or_reset_branch("cursus-release/main")
		.await
		.unwrap();
	let file = work_dir.path().join("release.txt");
	std::fs::write(&file, "content").unwrap();
	git.add(&[file]).await.unwrap();
	git.commit("chore: release commit").await.unwrap();
	git.force_push_branch("cursus-release/main").await.unwrap();
}
