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
}
