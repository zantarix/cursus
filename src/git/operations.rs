//! Low-level git command wrappers.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use crate::command::CommandRunner;

/// Stages the given files for the next git commit.
///
/// # Errors
///
/// Returns an error if `git add` exits with a non-zero status.
pub(crate) fn git_add(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
	files: &[PathBuf],
) -> anyhow::Result<()> {
	if files.is_empty() {
		return Ok(());
	}

	// Convert PathBuf slice to &str slice for the runner.
	let file_str_storage: Vec<String> = files
		.iter()
		.map(|f| f.to_string_lossy().into_owned())
		.collect();
	let mut args = vec!["add", "--"];
	let file_str_refs: Vec<&str> = file_str_storage.iter().map(|s| s.as_str()).collect();
	args.extend_from_slice(&file_str_refs);

	let output = runner
		.run("git", &args, git_workdir)
		.context("Failed to run git add")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git add failed: {stderr}");
	}

	Ok(())
}

/// Creates a git commit with the given message.
///
/// # Errors
///
/// Returns an error if `git commit` exits with a non-zero status.
pub(crate) fn git_commit(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
	message: &str,
) -> anyhow::Result<()> {
	let output = runner
		.run("git", &["commit", "-m", message], git_workdir)
		.context("Failed to run git commit")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git commit failed: {stderr}");
	}

	Ok(())
}

/// Creates an annotated git tag with the given name and message.
///
/// # Errors
///
/// Returns an error if `git tag` exits with a non-zero status.
pub(crate) fn git_tag(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
	tag_name: &str,
	message: &str,
) -> anyhow::Result<()> {
	let output = runner
		.run("git", &["tag", "-a", tag_name, "-m", message], git_workdir)
		.context("Failed to run git tag")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git tag failed: {stderr}");
	}

	Ok(())
}

/// Pushes to origin, following tags.
///
/// Runs `git push origin HEAD --follow-tags`.
///
/// # Errors
///
/// Returns an error if `git push` exits with a non-zero status.
pub(crate) fn git_push(runner: &dyn CommandRunner, git_workdir: &Path) -> anyhow::Result<()> {
	let output = runner
		.run(
			"git",
			&["push", "origin", "HEAD", "--follow-tags"],
			git_workdir,
		)
		.context("Failed to run git push")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git push failed: {stderr}");
	}

	Ok(())
}

/// Returns the porcelain status of the working tree.
///
/// Runs `git status --porcelain` and returns the raw output as a string.
/// An empty string means the working tree is clean.
///
/// # Errors
///
/// Returns an error if `git status` exits with a non-zero status.
pub(crate) fn git_status_porcelain(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
) -> anyhow::Result<String> {
	let output = runner
		.run("git", &["status", "--porcelain"], git_workdir)
		.context("Failed to run git status")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git status failed: {stderr}");
	}

	Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Returns the name of the current branch, or `None` when HEAD is detached.
///
/// Runs `git rev-parse --abbrev-ref HEAD`. Returns `None` if the output is `"HEAD"`
/// (detached HEAD state).
///
/// # Errors
///
/// Returns an error if `git rev-parse` exits with a non-zero status.
pub(crate) fn git_current_branch(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
) -> anyhow::Result<Option<String>> {
	let output = runner
		.run("git", &["rev-parse", "--abbrev-ref", "HEAD"], git_workdir)
		.context("Failed to run git rev-parse")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git rev-parse failed: {stderr}");
	}

	let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
	if branch.is_empty() || branch == "HEAD" {
		Ok(None)
	} else {
		Ok(Some(branch))
	}
}

/// Creates and checks out a new branch.
///
/// Runs `git checkout -b <branch>`.
///
/// # Errors
///
/// Returns an error if `git checkout` exits with a non-zero status.
pub(crate) fn git_checkout_new_branch(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
	branch: &str,
) -> anyhow::Result<()> {
	let output = runner
		.run("git", &["checkout", "-b", branch], git_workdir)
		.context("Failed to run git checkout")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git checkout -b failed: {stderr}");
	}

	Ok(())
}

/// Checks out an existing branch.
///
/// Runs `git checkout <branch>`.
///
/// # Errors
///
/// Returns an error if `git checkout` exits with a non-zero status.
pub(crate) fn git_checkout(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
	branch: &str,
) -> anyhow::Result<()> {
	let output = runner
		.run("git", &["checkout", branch], git_workdir)
		.context("Failed to run git checkout")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git checkout failed: {stderr}");
	}

	Ok(())
}

/// Pushes a named branch to origin.
///
/// Runs `git push origin <branch>`.
///
/// # Errors
///
/// Returns an error if `git push` exits with a non-zero status.
pub(crate) fn git_push_branch(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
	branch: &str,
) -> anyhow::Result<()> {
	let output = runner
		.run("git", &["push", "origin", branch], git_workdir)
		.context("Failed to run git push branch")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git push branch failed: {stderr}");
	}

	Ok(())
}

/// Returns `true` if the given tag exists in the repository.
///
/// Runs `git tag -l <tag>` and checks whether the output is non-empty.
///
/// # Errors
///
/// Returns an error if `git tag` exits with a non-zero status.
pub(crate) fn git_tag_exists(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
	tag: &str,
) -> anyhow::Result<bool> {
	let output = runner
		.run("git", &["tag", "-l", tag], git_workdir)
		.context("Failed to run git tag -l")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git tag -l failed: {stderr}");
	}

	Ok(!output.stdout.is_empty())
}

/// Pushes a specific tag to origin.
///
/// Runs `git push origin <tag>`, pushing only that tag rather than all local tags.
///
/// # Errors
///
/// Returns an error if `git push` exits with a non-zero status.
pub(crate) fn git_push_tag(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
	tag: &str,
) -> anyhow::Result<()> {
	let output = runner
		.run("git", &["push", "origin", tag], git_workdir)
		.context("Failed to run git push tag")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git push tag failed: {stderr}");
	}

	Ok(())
}

/// Pushes all local tags to origin.
///
/// Runs `git push origin --tags`.
///
/// # Errors
///
/// Returns an error if `git push` exits with a non-zero status.
#[allow(dead_code)]
pub(crate) fn git_push_tags(runner: &dyn CommandRunner, git_workdir: &Path) -> anyhow::Result<()> {
	let output = runner
		.run("git", &["push", "origin", "--tags"], git_workdir)
		.context("Failed to run git push --tags")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git push --tags failed: {stderr}");
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use tempfile::TempDir;

	use super::*;
	use crate::command::test_support::RecordingCommandRunner;

	fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	fn recording(exit_code: i32) -> Arc<RecordingCommandRunner> {
		Arc::new(RecordingCommandRunner::new(exit_code))
	}

	fn recording_with_stderr(exit_code: i32, stderr: &[u8]) -> Arc<RecordingCommandRunner> {
		Arc::new(RecordingCommandRunner::new(exit_code).with_stderr(stderr.to_vec()))
	}

	#[test]
	fn git_add_empty_files_is_noop() {
		let dir = temp_dir();
		let runner = recording(0);
		let result = git_add(runner.as_ref(), dir.path(), &[]);
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
		let result = git_add(runner.as_ref(), dir.path(), &[dir.path().join("file.txt")]);
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
		let file = dir.path().join("file.txt");
		git_add(runner.as_ref(), dir.path(), &[file.clone()]).unwrap();
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
		let result = git_commit(runner.as_ref(), dir.path(), "test commit");
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
		git_commit(runner.as_ref(), dir.path(), "chore(release): my-pkg@1.0.0").unwrap();
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
		let result = git_tag(runner.as_ref(), dir.path(), "v1.0.0", "Release 1.0.0");
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
		git_tag(runner.as_ref(), dir.path(), "v1.0.0", "Release 1.0.0").unwrap();
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
		let result = git_push(runner.as_ref(), dir.path());
		assert!(result.is_ok());
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(
			invocations[0].args,
			["push", "origin", "HEAD", "--follow-tags"]
		);
		assert_eq!(invocations[0].cwd, dir.path());
	}

	#[test]
	fn git_push_failure_propagates() {
		let dir = temp_dir();
		let runner = recording_with_stderr(1, b"fatal: not a git repo");
		let result = git_push(runner.as_ref(), dir.path());
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
		git_status_porcelain(runner.as_ref(), dir.path()).unwrap();
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
		let result = git_status_porcelain(runner.as_ref(), dir.path());
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
		let runner =
			Arc::new(RecordingCommandRunner::new(0).with_stdout(b" M src/main.rs\n".to_vec()));
		let result = git_status_porcelain(runner.as_ref(), dir.path()).unwrap();
		assert_eq!(result, " M src/main.rs\n");
	}

	#[test]
	fn git_current_branch_passes_correct_args() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"main\n".to_vec()));
		git_current_branch(runner.as_ref(), dir.path()).unwrap();
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
		let result = git_current_branch(runner.as_ref(), dir.path()).unwrap();
		assert_eq!(result, Some("main".to_string()));
	}

	#[test]
	fn git_current_branch_returns_none_when_detached() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"HEAD\n".to_vec()));
		let result = git_current_branch(runner.as_ref(), dir.path()).unwrap();
		assert_eq!(result, None);
	}

	#[test]
	fn git_current_branch_failure_propagates() {
		let dir = temp_dir();
		let runner = recording_with_stderr(1, b"fatal: not a git repo");
		let result = git_current_branch(runner.as_ref(), dir.path());
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git rev-parse failed"),
			"Expected 'git rev-parse failed', got: {msg}"
		);
	}

	#[test]
	fn git_checkout_new_branch_passes_correct_args() {
		let dir = temp_dir();
		let runner = recording(0);
		git_checkout_new_branch(runner.as_ref(), dir.path(), "feature/my-branch").unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, ["checkout", "-b", "feature/my-branch"]);
		assert_eq!(invocations[0].cwd, dir.path());
	}

	#[test]
	fn git_checkout_new_branch_failure_propagates() {
		let dir = temp_dir();
		let runner = recording_with_stderr(1, b"fatal: branch already exists");
		let result = git_checkout_new_branch(runner.as_ref(), dir.path(), "main");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git checkout -b failed"),
			"Expected 'git checkout -b failed', got: {msg}"
		);
	}

	#[test]
	fn git_checkout_passes_correct_args() {
		let dir = temp_dir();
		let runner = recording(0);
		git_checkout(runner.as_ref(), dir.path(), "main").unwrap();
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
		let result = git_checkout(runner.as_ref(), dir.path(), "main");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git checkout failed"),
			"Expected 'git checkout failed', got: {msg}"
		);
	}

	#[test]
	fn git_push_tag_passes_correct_args() {
		let dir = temp_dir();
		let runner = recording(0);
		git_push_tag(runner.as_ref(), dir.path(), "v1.2.0").unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, ["push", "origin", "v1.2.0"]);
		assert_eq!(invocations[0].cwd, dir.path());
	}

	#[test]
	fn git_push_tag_failure_propagates() {
		let dir = temp_dir();
		let runner = recording_with_stderr(1, b"fatal: not a git repo");
		let result = git_push_tag(runner.as_ref(), dir.path(), "v1.0.0");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git push tag failed"),
			"Expected 'git push tag failed', got: {msg}"
		);
	}

	#[test]
	fn git_push_branch_passes_correct_args() {
		let dir = temp_dir();
		let runner = recording(0);
		git_push_branch(runner.as_ref(), dir.path(), "chronicle-release/main").unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(
			invocations[0].args,
			["push", "origin", "chronicle-release/main"]
		);
		assert_eq!(invocations[0].cwd, dir.path());
	}

	#[test]
	fn git_push_branch_failure_propagates() {
		let dir = temp_dir();
		let runner = recording_with_stderr(1, b"fatal: not a git repo");
		let result = git_push_branch(runner.as_ref(), dir.path(), "release/main");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git push branch failed"),
			"Expected 'git push branch failed', got: {msg}"
		);
	}

	#[test]
	fn git_tag_exists_passes_correct_args() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"v1.0.0\n".to_vec()));
		git_tag_exists(runner.as_ref(), dir.path(), "v1.0.0").unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, ["tag", "-l", "v1.0.0"]);
		assert_eq!(invocations[0].cwd, dir.path());
	}

	#[test]
	fn git_tag_exists_returns_true_when_output_nonempty() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"v1.0.0\n".to_vec()));
		let result = git_tag_exists(runner.as_ref(), dir.path(), "v1.0.0").unwrap();
		assert!(result);
	}

	#[test]
	fn git_tag_exists_returns_false_when_output_empty() {
		let dir = temp_dir();
		let runner = recording(0);
		let result = git_tag_exists(runner.as_ref(), dir.path(), "v1.0.0").unwrap();
		assert!(!result);
	}

	#[test]
	fn git_tag_exists_failure_propagates() {
		let dir = temp_dir();
		let runner = recording_with_stderr(1, b"fatal: not a git repo");
		let result = git_tag_exists(runner.as_ref(), dir.path(), "v1.0.0");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git tag -l failed"),
			"Expected 'git tag -l failed', got: {msg}"
		);
	}

	#[test]
	fn git_push_tags_passes_correct_args() {
		let dir = temp_dir();
		let runner = recording(0);
		git_push_tags(runner.as_ref(), dir.path()).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, ["push", "origin", "--tags"]);
		assert_eq!(invocations[0].cwd, dir.path());
	}

	#[test]
	fn git_push_tags_failure_propagates() {
		let dir = temp_dir();
		let runner = recording_with_stderr(1, b"fatal: not a git repo");
		let result = git_push_tags(runner.as_ref(), dir.path());
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git push --tags failed"),
			"Expected 'git push --tags failed', got: {msg}"
		);
	}
}
