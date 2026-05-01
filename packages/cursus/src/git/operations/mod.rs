//! Low-level git command wrappers.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use async_trait::async_trait;

use crate::command::CommandRunner;
use crate::git::Git;
use crate::git::ref_format::{validate_branch_name, validate_revision, validate_tag_name};
use crate::path::AbsolutePath;
use crate::redact::redact_credentials;

/// A git working directory paired with a command runner.
///
/// Bundles the repository root path and a [`CommandRunner`] so that
/// every git operation can be called as a method without repeating
/// both parameters. Implements the [`Git`] trait.
#[derive(Debug)]
pub struct GitWorkdir {
	path: AbsolutePath,
	runner: Arc<dyn CommandRunner>,
}

impl GitWorkdir {
	/// Creates a new `GitWorkdir` from a command runner and repository root path.
	pub fn new(runner: Arc<dyn CommandRunner>, path: AbsolutePath) -> Self {
		Self { path, runner }
	}
}

#[async_trait]
impl Git for GitWorkdir {
	fn path(&self) -> &AbsolutePath {
		&self.path
	}

	/// Stages the given files for the next git commit.
	///
	/// # Errors
	///
	/// Returns an error if `git add` exits with a non-zero status.
	async fn add(&self, files: &[PathBuf]) -> anyhow::Result<()> {
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

		let output = self
			.runner
			.run_mut("git", &args, &self.path)
			.await
			.context("Failed to run git add")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git add failed: {stderr}");
		}

		Ok(())
	}

	/// Creates a git commit with the given message.
	///
	/// # Errors
	///
	/// Returns an error if `git commit` exits with a non-zero status.
	async fn commit(&self, message: &str) -> anyhow::Result<()> {
		let output = self
			.runner
			.run_mut("git", &["commit", "-m", message], &self.path)
			.await
			.context("Failed to run git commit")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git commit failed: {stderr}");
		}

		Ok(())
	}

	/// Creates an annotated git tag with the given name and message.
	///
	/// # Errors
	///
	/// Returns an error if `git tag` exits with a non-zero status.
	async fn tag(&self, tag_name: &str, message: &str) -> anyhow::Result<()> {
		validate_tag_name(tag_name)?;
		let output = self
			.runner
			.run_mut("git", &["tag", "-a", tag_name, "-m", message], &self.path)
			.await
			.context("Failed to run git tag")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git tag failed: {stderr}");
		}

		Ok(())
	}

	/// Pushes HEAD to origin.
	///
	/// Runs `git push origin HEAD`.
	///
	/// Tags are never pushed here — tag pushing is the responsibility of the
	/// `publish` command, which pushes each tag individually via [`GitWorkdir::push_tag`].
	///
	/// # Errors
	///
	/// Returns an error if `git push` exits with a non-zero status.
	async fn push(&self) -> anyhow::Result<()> {
		let output = self
			.runner
			.run_mut("git", &["push", "origin", "HEAD"], &self.path)
			.await
			.context("Failed to run git push")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git push failed: {stderr}");
		}

		Ok(())
	}

	/// Returns `true` if the working tree has uncommitted changes.
	///
	/// # Errors
	///
	/// Returns an error if `git status` exits with a non-zero status.
	async fn is_dirty(&self) -> anyhow::Result<bool> {
		let output = self
			.runner
			.run("git", &["status", "--porcelain"], &self.path)
			.await
			.context("Failed to run git status")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git status failed: {stderr}");
		}

		Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
	}

	/// Returns the full SHA of the current HEAD commit.
	///
	/// Runs `git rev-parse HEAD`.
	///
	/// # Errors
	///
	/// Returns an error if `git rev-parse` exits with a non-zero status.
	async fn head_sha(&self) -> anyhow::Result<String> {
		let output = self
			.runner
			.run("git", &["rev-parse", "HEAD"], &self.path)
			.await
			.context("Failed to run git rev-parse HEAD")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git rev-parse HEAD failed: {stderr}");
		}

		Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
	}

	/// Returns the name of the current branch, or `None` when HEAD is detached.
	///
	/// Runs `git rev-parse --abbrev-ref HEAD`. Returns `None` if the output is `"HEAD"`
	/// (detached HEAD state).
	///
	/// # Errors
	///
	/// Returns an error if `git rev-parse` exits with a non-zero status.
	async fn current_branch(&self) -> anyhow::Result<Option<String>> {
		let output = self
			.runner
			.run("git", &["rev-parse", "--abbrev-ref", "HEAD"], &self.path)
			.await
			.context("Failed to run git rev-parse")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git rev-parse failed: {stderr}");
		}

		let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
		if branch.is_empty() || branch == "HEAD" {
			Ok(None)
		} else {
			Ok(Some(branch))
		}
	}

	/// Checks out an existing branch.
	///
	/// Runs `git checkout <branch>`.
	///
	/// # Errors
	///
	/// Returns an error if `git checkout` exits with a non-zero status.
	async fn checkout(&self, branch: &str) -> anyhow::Result<()> {
		validate_branch_name(branch)?;
		let output = self
			.runner
			.run_mut("git", &["checkout", branch], &self.path)
			.await
			.context("Failed to run git checkout")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git checkout failed: {stderr}");
		}

		Ok(())
	}

	/// Returns `true` if the given tag exists in the repository.
	///
	/// Runs `git rev-parse --verify refs/tags/<tag>` and checks the exit status.
	/// A zero exit status means the ref exists; non-zero means it does not.
	/// Using the full ref path avoids the glob interpretation that
	/// `git tag -l <pattern>` applies to its argument.
	///
	/// # Errors
	///
	/// Returns an error only if the `git` binary cannot be executed. A non-zero
	/// exit code — whether due to a missing ref, not being in a git repository,
	/// or any other reason — is treated as the tag not existing and returns
	/// `Ok(false)`.
	async fn tag_exists(&self, tag: &str) -> anyhow::Result<bool> {
		validate_tag_name(tag)?;
		let ref_path = format!("refs/tags/{tag}");
		let output = self
			.runner
			.run("git", &["rev-parse", "--verify", &ref_path], &self.path)
			.await
			.context("Failed to run git rev-parse")?;

		Ok(output.status.success())
	}

	/// Returns the URL of the `origin` remote, or `None` if there is no origin.
	///
	/// Runs `git remote get-url origin`. A non-zero exit status is treated as
	/// "no origin" and returns `None` rather than an error.
	///
	/// # Errors
	///
	/// Returns an error if the git command cannot be executed at all.
	async fn remote_origin_url(&self) -> anyhow::Result<Option<String>> {
		let output = self
			.runner
			.run("git", &["remote", "get-url", "origin"], &self.path)
			.await
			.context("Failed to query git remote URL")?;

		if !output.status.success() {
			return Ok(None);
		}

		Ok(Some(
			String::from_utf8_lossy(&output.stdout).trim().to_string(),
		))
	}

	/// Creates or resets a branch at the current HEAD.
	///
	/// Runs `git checkout -B <branch>`. If the branch already exists, it is reset
	/// to the current HEAD, making this operation idempotent.
	///
	/// # Errors
	///
	/// Returns an error if `git checkout` exits with a non-zero status.
	async fn checkout_or_reset_branch(&self, branch: &str) -> anyhow::Result<()> {
		validate_branch_name(branch)?;
		let output = self
			.runner
			.run_mut("git", &["checkout", "-B", branch], &self.path)
			.await
			.context("Failed to run git checkout")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git checkout -B failed: {stderr}");
		}

		Ok(())
	}

	/// Force-pushes a named branch to origin using `--force-with-lease`.
	///
	/// Runs `git push --force-with-lease origin <branch>`. The `--force-with-lease`
	/// flag ensures the push is rejected if the remote branch has been updated by
	/// someone else since the last fetch, preventing accidental overwrites.
	///
	/// # Errors
	///
	/// Returns an error if `git push` exits with a non-zero status.
	async fn force_push_branch(&self, branch: &str) -> anyhow::Result<()> {
		validate_branch_name(branch)?;
		let output = self
			.runner
			.run_mut(
				"git",
				&["push", "--force-with-lease", "origin", branch],
				&self.path,
			)
			.await
			.context("Failed to run git force push branch")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git force push branch failed: {stderr}");
		}

		Ok(())
	}

	/// Deletes a local git tag.
	///
	/// Runs `git tag -d <tag>`. Used as cleanup after a failed tag push, so that
	/// a retry can re-create and re-push the tag without hitting "tag already exists".
	///
	/// # Errors
	///
	/// Returns an error if `git tag -d` exits with a non-zero status.
	async fn delete_tag(&self, tag: &str) -> anyhow::Result<()> {
		validate_tag_name(tag)?;
		let output = self
			.runner
			.run_mut("git", &["tag", "-d", tag], &self.path)
			.await
			.context("Failed to run git tag -d")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git tag -d failed: {stderr}");
		}

		Ok(())
	}

	/// Pushes a specific tag to origin.
	///
	/// Runs `git push origin tag <tag>`, using the `tag` keyword to unambiguously
	/// push a tag ref rather than a branch ref of the same name.
	///
	/// # Errors
	///
	/// Returns an error if `git push` exits with a non-zero status.
	async fn push_tag(&self, tag: &str) -> anyhow::Result<()> {
		validate_tag_name(tag)?;
		let output = self
			.runner
			.run_mut("git", &["push", "origin", "tag", tag], &self.path)
			.await
			.context("Failed to run git push tag")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git push tag failed: {stderr}");
		}

		Ok(())
	}

	/// Returns the number of commits in the given revision range.
	///
	/// Runs `git rev-list --count <range>` and parses the output as a `usize`.
	///
	/// # Errors
	///
	/// Returns an error if `git rev-list` exits with a non-zero status or the
	/// output cannot be parsed as an integer.
	async fn rev_list_count(&self, range: &str) -> anyhow::Result<usize> {
		validate_revision(range)?;
		let output = self
			.runner
			.run("git", &["rev-list", "--count", range, "--"], &self.path)
			.await
			.context("Failed to run git rev-list --count")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git rev-list --count failed: {stderr}");
		}

		let count_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
		count_str
			.parse::<usize>()
			.with_context(|| format!("Failed to parse git rev-list count: '{count_str}'"))
	}

	/// Returns the full commit message for the given revision.
	///
	/// Runs `git log -1 --format=%B <rev>` and returns the trimmed message.
	///
	/// # Errors
	///
	/// Returns an error if `git log` exits with a non-zero status.
	async fn log_message(&self, rev: &str) -> anyhow::Result<String> {
		validate_revision(rev)?;
		let output = self
			.runner
			.run("git", &["log", "-1", "--format=%B", rev, "--"], &self.path)
			.await
			.context("Failed to run git log")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git log failed: {stderr}");
		}

		Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
	}

	/// Returns the list of files changed by the given commit.
	///
	/// Runs `git diff-tree --no-commit-id -r --name-only <commit>` and returns
	/// one relative path per line, filtering empty lines.
	///
	/// # Errors
	///
	/// Returns an error if `git diff-tree` exits with a non-zero status.
	async fn diff_tree_names(&self, commit: &str) -> anyhow::Result<Vec<String>> {
		validate_revision(commit)?;
		let output = self
			.runner
			.run(
				"git",
				&["diff-tree", "--no-commit-id", "-r", "--name-only", commit],
				&self.path,
			)
			.await
			.context("Failed to run git diff-tree")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git diff-tree failed: {stderr}");
		}

		Ok(String::from_utf8_lossy(&output.stdout)
			.lines()
			.filter(|l| !l.is_empty())
			.map(|l| l.to_string())
			.collect())
	}

	/// Returns the full SHA of the commit that first added the given path.
	///
	/// Runs `git log --first-parent --diff-filter=A --format=%H -- <path>` and returns
	/// the first SHA found. Returns `Ok(None)` when no commit is found (file was not
	/// added via a tracked commit, or the history was rewritten).
	///
	/// # Errors
	///
	/// Returns an error if `git log` exits with a non-zero status.
	async fn log_added_commit(&self, path: &std::path::Path) -> anyhow::Result<Option<String>> {
		let path_str = path.to_string_lossy();
		let output = self
			.runner
			.run(
				"git",
				&[
					"log",
					"--first-parent",
					"--diff-filter=A",
					"--format=%H",
					"--",
					path_str.as_ref(),
				],
				&self.path,
			)
			.await
			.context("Failed to run git log --diff-filter=A")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git log --diff-filter=A failed: {stderr}");
		}

		let sha = String::from_utf8_lossy(&output.stdout)
			.lines()
			.next()
			.map(|l| l.trim().to_string())
			.filter(|s| !s.is_empty());

		Ok(sha)
	}

	/// Returns the subject line of the given commit.
	///
	/// Runs `git log -1 --format=%s <rev>` and returns the trimmed subject.
	///
	/// # Errors
	///
	/// Returns an error if `git log` exits with a non-zero status.
	async fn log_subject(&self, rev: &str) -> anyhow::Result<String> {
		validate_revision(rev)?;
		let output = self
			.runner
			.run("git", &["log", "-1", "--format=%s", rev, "--"], &self.path)
			.await
			.context("Failed to run git log --format=%s")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git log --format=%s failed: {stderr}");
		}

		Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
	}

	/// Returns the list of files reported by `git diff --name-only <extra_args>`.
	///
	/// `extra_args` is appended after `--name-only`. Pass `&["origin/HEAD..HEAD"]`
	/// for committed-but-not-pushed changes, `&["--cached"]` for staged changes,
	/// or `&[]` for unstaged working-tree changes.
	///
	/// Returns one relative path per line, filtering empty lines.
	///
	/// # Safety contract
	///
	/// All callers must pass only trusted or pre-validated values in `extra_args`.
	/// Values sourced from CLI flags must be validated upstream before reaching
	/// this method (e.g. `--base` is validated in `cmd_verify`).
	///
	/// # Errors
	///
	/// Returns an error if `git diff` exits with a non-zero status.
	async fn diff_names(&self, extra_args: &[&str]) -> anyhow::Result<Vec<String>> {
		let mut args = vec!["diff", "--name-only"];
		args.extend_from_slice(extra_args);
		let output = self
			.runner
			.run("git", &args, &self.path)
			.await
			.context("Failed to run git diff --name-only")?;

		if !output.status.success() {
			let raw = String::from_utf8_lossy(&output.stderr);
			let stderr = redact_credentials(&raw);
			bail!("git diff --name-only failed: {stderr}");
		}

		Ok(String::from_utf8_lossy(&output.stdout)
			.lines()
			.filter(|l| !l.is_empty())
			.map(|l| l.to_string())
			.collect())
	}
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;
