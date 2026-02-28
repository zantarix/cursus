//! Low-level git command wrappers.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

/// Stages the given files for the next git commit.
///
/// # Errors
///
/// Returns an error if `git add` exits with a non-zero status.
pub(crate) fn git_add(git_workdir: &Path, files: &[PathBuf]) -> anyhow::Result<()> {
	if files.is_empty() {
		return Ok(());
	}

	let output = Command::new("git")
		.arg("-C")
		.arg(git_workdir)
		.arg("add")
		.arg("--")
		.args(files)
		.output()
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
pub(crate) fn git_commit(git_workdir: &Path, message: &str) -> anyhow::Result<()> {
	let output = Command::new("git")
		.arg("-C")
		.arg(git_workdir)
		.arg("commit")
		.arg("-m")
		.arg(message)
		.output()
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
pub(crate) fn git_tag(git_workdir: &Path, tag_name: &str, message: &str) -> anyhow::Result<()> {
	let output = Command::new("git")
		.arg("-C")
		.arg(git_workdir)
		.arg("tag")
		.arg("-a")
		.arg(tag_name)
		.arg("-m")
		.arg(message)
		.output()
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
#[coverage(off)]
#[mutants::skip]
pub(crate) fn git_push(git_workdir: &Path) -> anyhow::Result<()> {
	let output = Command::new("git")
		.arg("-C")
		.arg(git_workdir)
		.arg("push")
		.arg("origin")
		.arg("HEAD")
		.arg("--follow-tags")
		.output()
		.context("Failed to run git push")?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		bail!("git push failed: {stderr}");
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn non_git_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	/// Creates a real, minimal git repo with user config and signing disabled.
	fn real_git_repo() -> TempDir {
		let dir = non_git_dir();
		let run = |args: &[&str]| {
			Command::new("git")
				.args(args)
				.current_dir(dir.path())
				.stdout(std::process::Stdio::null())
				.stderr(std::process::Stdio::null())
				.status()
				.expect("git failed to run")
		};
		run(&["init"]);
		run(&["config", "user.name", "Test"]);
		run(&["config", "user.email", "test@test.local"]);
		run(&["config", "commit.gpgsign", "false"]);
		run(&["config", "tag.gpgsign", "false"]);
		run(&["commit", "--allow-empty", "-m", "init"]);
		dir
	}

	#[test]
	fn git_add_empty_files_is_noop() {
		let dir = non_git_dir();
		let result = git_add(dir.path(), &[]);
		assert!(result.is_ok());
	}

	#[test]
	fn git_add_error_in_non_git_dir() {
		let dir = non_git_dir();
		let result = git_add(dir.path(), &[dir.path().join("nonexistent.txt")]);
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git add failed"),
			"Expected 'git add failed', got: {msg}"
		);
	}

	#[test]
	fn git_commit_error_in_non_git_dir() {
		let dir = non_git_dir();
		let result = git_commit(dir.path(), "test commit");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git commit failed"),
			"Expected 'git commit failed', got: {msg}"
		);
	}

	#[test]
	fn git_tag_error_in_non_git_dir() {
		let dir = non_git_dir();
		let result = git_tag(dir.path(), "v1.0.0", "Release 1.0.0");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git tag failed"),
			"Expected 'git tag failed', got: {msg}"
		);
	}

	#[test]
	fn git_add_success_in_real_repo() {
		let dir = real_git_repo();
		std::fs::write(dir.path().join("file.txt"), "content").unwrap();
		let result = git_add(dir.path(), &[dir.path().join("file.txt")]);
		assert!(result.is_ok());
	}

	#[test]
	fn git_commit_success_in_real_repo() {
		let dir = real_git_repo();
		std::fs::write(dir.path().join("file.txt"), "content").unwrap();
		git_add(dir.path(), &[dir.path().join("file.txt")]).unwrap();
		let result = git_commit(dir.path(), "test: add file");
		assert!(result.is_ok());
	}

	#[test]
	fn git_tag_success_in_real_repo() {
		let dir = real_git_repo();
		let result = git_tag(dir.path(), "v1.0.0", "Release 1.0.0");
		assert!(result.is_ok());
	}
}
