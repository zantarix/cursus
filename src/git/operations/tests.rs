use super::*;

use std::sync::Arc;

use tempfile::TempDir;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::filesystem::LocalFilesystem;
use crate::path::AbsolutePath;

fn temp_dir() -> TempDir {
	tempfile::tempdir().expect("Failed to create temp dir")
}

fn abs(dir: &TempDir) -> AbsolutePath {
	AbsolutePath::new(dir.path()).unwrap()
}

fn recording(exit_code: i32) -> Arc<RecordingCommandRunner> {
	Arc::new(RecordingCommandRunner::new(exit_code))
}

fn recording_with_stderr(exit_code: i32, stderr: &[u8]) -> Arc<RecordingCommandRunner> {
	Arc::new(RecordingCommandRunner::new(exit_code).with_stderr(stderr.to_vec()))
}

fn make_git(
	runner: Arc<RecordingCommandRunner>,
	dir_abs: AbsolutePath,
) -> (GitWorkdir, Arc<RecordingCommandRunner>) {
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
	);
	let git = GitWorkdir::new(env.runner(), dir_abs);
	(git, runner)
}

#[test]
fn git_add_empty_files_is_noop() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	let result = git.add(&[]);
	assert!(result.is_ok());
	assert!(
		runner.invocations().is_empty(),
		"No command should run for empty file list"
	);
}

#[test]
fn git_add_failure_propagates_error() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repository");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.add(&[dir.path().join("file.txt")]);
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git add failed"),
		"Expected 'git add failed', got: {msg}"
	);
}

#[test]
fn git_add_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	let file = dir.path().join("file.txt");
	git.add(&[file.clone()]).unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args[0], "add");
	assert_eq!(invocations[0].args[1], "--");
	assert!(invocations[0].args[2].contains("file.txt"));
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_commit_failure_propagates_error() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repository");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.commit("test commit");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git commit failed"),
		"Expected 'git commit failed', got: {msg}"
	);
}

#[test]
fn git_commit_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.commit("chore(release): my-pkg@1.0.0").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(
		invocations[0].args,
		["commit", "-m", "chore(release): my-pkg@1.0.0"]
	);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_tag_failure_propagates_error() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repository");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.tag("v1.0.0", "Release 1.0.0");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git tag failed"),
		"Expected 'git tag failed', got: {msg}"
	);
}

#[test]
fn git_tag_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.tag("v1.0.0", "Release 1.0.0").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(
		invocations[0].args,
		["tag", "-a", "v1.0.0", "-m", "Release 1.0.0"]
	);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_push_invokes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	let result = git.push();
	assert!(result.is_ok());
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["push", "origin", "HEAD"]);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_push_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repo");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.push();
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git push failed"),
		"Expected 'git push failed', got: {msg}"
	);
}

#[test]
fn git_status_porcelain_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.status_porcelain().unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["status", "--porcelain"]);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_status_porcelain_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repo");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.status_porcelain();
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git status failed"),
		"Expected 'git status failed', got: {msg}"
	);
}

#[test]
fn git_status_porcelain_returns_stdout() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b" M src/main.rs\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.status_porcelain().unwrap();
	assert_eq!(result, " M src/main.rs\n");
}

#[test]
fn git_current_branch_passes_correct_args() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"main\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.current_branch().unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["rev-parse", "--abbrev-ref", "HEAD"]);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_current_branch_returns_branch_name() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"main\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.current_branch().unwrap();
	assert_eq!(result, Some("main".to_string()));
}

#[test]
fn git_current_branch_returns_none_when_detached() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"HEAD\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.current_branch().unwrap();
	assert_eq!(result, None);
}

#[test]
fn git_current_branch_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repo");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.current_branch();
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git rev-parse failed"),
		"Expected 'git rev-parse failed', got: {msg}"
	);
}
#[test]
fn git_checkout_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.checkout("main").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["checkout", "main"]);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_checkout_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"error: pathspec 'main' did not match");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.checkout("main");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git checkout failed"),
		"Expected 'git checkout failed', got: {msg}"
	);
}

#[test]
fn git_delete_tag_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.delete_tag("v1.0.0").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["tag", "-d", "v1.0.0"]);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_delete_tag_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"error: tag 'v1.0.0' not found");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.delete_tag("v1.0.0");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git tag -d failed"),
		"Expected 'git tag -d failed', got: {msg}"
	);
}

#[test]
fn git_push_tag_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.push_tag("v1.2.0").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["push", "origin", "tag", "v1.2.0"]);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_push_tag_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repo");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.push_tag("v1.0.0");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git push tag failed"),
		"Expected 'git push tag failed', got: {msg}"
	);
}

#[test]
fn git_checkout_or_reset_branch_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.checkout_or_reset_branch("cursus-release/main").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(
		invocations[0].args,
		["checkout", "-B", "cursus-release/main"]
	);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_checkout_or_reset_branch_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repo");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.checkout_or_reset_branch("release/main");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git checkout -B failed"),
		"Expected 'git checkout -B failed', got: {msg}"
	);
}

#[test]
fn git_force_push_branch_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.force_push_branch("cursus-release/main").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(
		invocations[0].args,
		[
			"push",
			"--force-with-lease",
			"origin",
			"cursus-release/main"
		]
	);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_force_push_branch_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repo");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.force_push_branch("release/main");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git force push branch failed"),
		"Expected 'git force push branch failed', got: {msg}"
	);
}

#[test]
fn git_tag_exists_passes_correct_args() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"abc123\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.tag_exists("v1.0.0").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(
		invocations[0].args,
		["rev-parse", "--verify", "refs/tags/v1.0.0"]
	);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn git_tag_exists_returns_true_on_zero_exit() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"abc123\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	assert!(git.tag_exists("v1.0.0").unwrap());
}

#[test]
fn git_tag_exists_returns_false_on_nonzero_exit() {
	let dir = temp_dir();
	let runner = recording(1);
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	assert!(!git.tag_exists("v1.0.0").unwrap());
}

#[test]
fn git_tag_exists_passes_glob_chars_as_literal_ref() {
	// Tag names containing glob metacharacters must be passed verbatim in the
	// ref path so git treats them literally, not as a glob pattern.
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"abc123\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.tag_exists("v1.0.0-rc[1]").unwrap();
	let invocations = runner.invocations();
	assert_eq!(
		invocations[0].args,
		["rev-parse", "--verify", "refs/tags/v1.0.0-rc[1]"]
	);
}

// --- remote_origin_url ---

#[test]
fn remote_origin_url_returns_url_on_success() {
	let dir = temp_dir();
	let runner = Arc::new(
		RecordingCommandRunner::new(0).with_stdout(b"https://github.com/owner/repo.git\n".to_vec()),
	);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	let result = git.remote_origin_url().unwrap();
	assert_eq!(
		result,
		Some("https://github.com/owner/repo.git".to_string())
	);
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].args, ["remote", "get-url", "origin"]);
}

#[test]
fn remote_origin_url_returns_none_when_git_fails() {
	let dir = temp_dir();
	let runner = recording(1); // non-zero exit → no origin remote
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.remote_origin_url().unwrap();
	assert_eq!(result, None);
}

// --- diff_names ---

#[test]
fn diff_names_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.diff_names(&["origin/HEAD..HEAD"]).unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(
		invocations[0].args,
		["diff", "--name-only", "origin/HEAD..HEAD"]
	);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[test]
fn diff_names_staged_passes_cached_flag() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.diff_names(&["--cached"]).unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations[0].args, ["diff", "--name-only", "--cached"]);
}

#[test]
fn diff_names_unstaged_passes_no_extra_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.diff_names(&[]).unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations[0].args, ["diff", "--name-only"]);
}

#[test]
fn diff_names_returns_parsed_lines() {
	let dir = temp_dir();
	let runner = Arc::new(
		RecordingCommandRunner::new(0)
			.with_stdout(b"packages/a/src/lib.rs\npackages/b/package.json\n".to_vec()),
	);
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.diff_names(&["origin/HEAD..HEAD"]).unwrap();
	assert_eq!(
		result,
		vec![
			"packages/a/src/lib.rs".to_string(),
			"packages/b/package.json".to_string(),
		]
	);
}

#[test]
fn diff_names_returns_empty_on_no_changes() {
	let dir = temp_dir();
	let runner = recording(0); // empty stdout
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.diff_names(&["origin/HEAD..HEAD"]).unwrap();
	assert!(result.is_empty());
}

#[test]
fn diff_names_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: ambiguous argument 'origin/HEAD'");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.diff_names(&["origin/HEAD..HEAD"]);
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git diff --name-only failed"),
		"Expected 'git diff --name-only failed', got: {msg}"
	);
}

// --- rev_list_count ---

#[test]
fn rev_list_count_passes_correct_args() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"3\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.rev_list_count("origin/HEAD..HEAD").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(
		invocations[0].args,
		["rev-list", "--count", "origin/HEAD..HEAD"]
	);
}

#[test]
fn rev_list_count_parses_output() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"5\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	assert_eq!(git.rev_list_count("origin/HEAD..HEAD").unwrap(), 5);
}

#[test]
fn rev_list_count_parses_zero() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"0\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	assert_eq!(git.rev_list_count("origin/HEAD..HEAD").unwrap(), 0);
}

#[test]
fn rev_list_count_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: bad revision");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.rev_list_count("origin/HEAD..HEAD");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git rev-list --count failed"),
		"Expected 'git rev-list --count failed', got: {msg}"
	);
}

#[test]
fn rev_list_count_invalid_output_propagates_error() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"not-a-number\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.rev_list_count("origin/HEAD..HEAD");
	assert!(result.is_err());
}

// --- log_message ---

#[test]
fn log_message_passes_correct_args() {
	let dir = temp_dir();
	let runner =
		Arc::new(RecordingCommandRunner::new(0).with_stdout(b"feat: add thing\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.log_message("HEAD").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["log", "-1", "--format=%B", "HEAD"]);
}

#[test]
fn log_message_returns_trimmed_message() {
	let dir = temp_dir();
	let runner =
		Arc::new(RecordingCommandRunner::new(0).with_stdout(b"feat: add thing\n\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.log_message("HEAD").unwrap();
	assert_eq!(result, "feat: add thing");
}

#[test]
fn log_message_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: bad revision 'HEAD'");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.log_message("HEAD");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git log failed"),
		"Expected 'git log failed', got: {msg}"
	);
}

// --- diff_tree_names ---

#[test]
fn diff_tree_names_passes_correct_args() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.diff_tree_names("HEAD").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(
		invocations[0].args,
		["diff-tree", "--no-commit-id", "-r", "--name-only", "HEAD"]
	);
}

#[test]
fn diff_tree_names_returns_parsed_lines() {
	let dir = temp_dir();
	let runner =
		Arc::new(RecordingCommandRunner::new(0).with_stdout(b"src/main.rs\nCargo.toml\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.diff_tree_names("HEAD").unwrap();
	assert_eq!(
		result,
		vec!["src/main.rs".to_string(), "Cargo.toml".to_string()]
	);
}

#[test]
fn diff_tree_names_returns_empty_on_no_files() {
	let dir = temp_dir();
	let runner = recording(0);
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.diff_tree_names("HEAD").unwrap();
	assert!(result.is_empty());
}

#[test]
fn diff_tree_names_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: bad object HEAD");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.diff_tree_names("HEAD");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git diff-tree failed"),
		"Expected 'git diff-tree failed', got: {msg}"
	);
}

#[test]
fn remote_origin_url_trims_whitespace() {
	let dir = temp_dir();
	let runner = Arc::new(
		RecordingCommandRunner::new(0).with_stdout(b"  git@github.com:owner/repo.git  \n".to_vec()),
	);
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.remote_origin_url().unwrap();
	assert_eq!(result, Some("git@github.com:owner/repo.git".to_string()));
}

// --- log_added_commit ---

#[test]
fn log_added_commit_passes_correct_args() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"abc1234\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.log_added_commit(std::path::Path::new(".cursus/change.md"))
		.unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(
		invocations[0].args,
		[
			"log",
			"--first-parent",
			"--diff-filter=A",
			"--format=%H",
			"--",
			".cursus/change.md"
		]
	);
}

#[test]
fn log_added_commit_returns_sha_when_found() {
	let dir = temp_dir();
	let runner = Arc::new(
		RecordingCommandRunner::new(0)
			.with_stdout(b"abcdef1234567890abcdef1234567890abcdef12\n".to_vec()),
	);
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git
		.log_added_commit(std::path::Path::new(".cursus/change.md"))
		.unwrap();
	assert_eq!(
		result,
		Some("abcdef1234567890abcdef1234567890abcdef12".to_string())
	);
}

#[test]
fn log_added_commit_returns_none_on_empty_output() {
	let dir = temp_dir();
	let runner = recording(0); // empty stdout
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git
		.log_added_commit(std::path::Path::new(".cursus/change.md"))
		.unwrap();
	assert_eq!(result, None);
}

#[test]
fn log_added_commit_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: not a git repo");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.log_added_commit(std::path::Path::new(".cursus/change.md"));
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git log --diff-filter=A failed"),
		"Expected 'git log --diff-filter=A failed', got: {msg}"
	);
}

// --- log_subject ---

#[test]
fn log_subject_passes_correct_args() {
	let dir = temp_dir();
	let runner =
		Arc::new(RecordingCommandRunner::new(0).with_stdout(b"feat: add thing\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, runner) = make_git(runner, dir_abs);
	git.log_subject("abc1234").unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["log", "-1", "--format=%s", "abc1234"]);
}

#[test]
fn log_subject_returns_trimmed_subject() {
	let dir = temp_dir();
	let runner =
		Arc::new(RecordingCommandRunner::new(0).with_stdout(b"feat: add thing (#42)\n".to_vec()));
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.log_subject("abc1234").unwrap();
	assert_eq!(result, "feat: add thing (#42)");
}

#[test]
fn log_subject_failure_propagates() {
	let dir = temp_dir();
	let runner = recording_with_stderr(1, b"fatal: bad revision 'abc1234'");
	let dir_abs = abs(&dir);
	let (git, _) = make_git(runner, dir_abs);
	let result = git.log_subject("abc1234");
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("git log --format=%s failed"),
		"Expected 'git log --format=%s failed', got: {msg}"
	);
}
