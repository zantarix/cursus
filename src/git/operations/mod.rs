//! Low-level git command wrappers.

use std::path::PathBuf;

use anyhow::{Context, bail};

use crate::path::AbsolutePath;

/// A git working directory paired with a command runner.
///
/// Bundles the repository root path and a command runner so that
/// every git operation can be called as a method without repeating
/// both parameters.
#[derive(Debug)]
pub(crate) struct GitWorkdir {
	path: AbsolutePath,
	env: crate::Env,
}

impl GitWorkdir {
	/// Creates a new `GitWorkdir` from an environment and repository root path.
	pub(crate) fn new(env: &crate::Env, path: AbsolutePath) -> Self {
		Self {
			path,
			env: env.clone(),
		}
	}

	/// Returns the repository root path.
	pub(crate) fn path(&self) -> &AbsolutePath {
		&self.path
	}

	/// Stages the given files for the next git commit.
	///
	/// # Errors
	///
	/// Returns an error if `git add` exits with a non-zero status.
	pub(crate) fn add(&self, files: &[PathBuf]) -> anyhow::Result<()> {
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
			.env
			.run_mut("git", &args, &self.path)
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
	pub(crate) fn commit(&self, message: &str) -> anyhow::Result<()> {
		let output = self
			.env
			.run_mut("git", &["commit", "-m", message], &self.path)
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
	pub(crate) fn tag(&self, tag_name: &str, message: &str) -> anyhow::Result<()> {
		let output = self
			.env
			.run_mut("git", &["tag", "-a", tag_name, "-m", message], &self.path)
			.context("Failed to run git tag")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
	pub(crate) fn push(&self) -> anyhow::Result<()> {
		let output = self
			.env
			.run_mut("git", &["push", "origin", "HEAD"], &self.path)
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
	pub(crate) fn status_porcelain(&self) -> anyhow::Result<String> {
		let output = self
			.env
			.run("git", &["status", "--porcelain"], &self.path)
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
	pub(crate) fn current_branch(&self) -> anyhow::Result<Option<String>> {
		let output = self
			.env
			.run("git", &["rev-parse", "--abbrev-ref", "HEAD"], &self.path)
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

	/// Checks out an existing branch.
	///
	/// Runs `git checkout <branch>`.
	///
	/// # Errors
	///
	/// Returns an error if `git checkout` exits with a non-zero status.
	pub(crate) fn checkout(&self, branch: &str) -> anyhow::Result<()> {
		let output = self
			.env
			.run_mut("git", &["checkout", branch], &self.path)
			.context("Failed to run git checkout")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			bail!("git checkout failed: {stderr}");
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
	pub(crate) fn tag_exists(&self, tag: &str) -> anyhow::Result<bool> {
		let output = self
			.env
			.run("git", &["tag", "-l", tag], &self.path)
			.context("Failed to run git tag -l")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			bail!("git tag -l failed: {stderr}");
		}

		Ok(!output.stdout.is_empty())
	}

	/// Returns the URL of the `origin` remote, or `None` if there is no origin.
	///
	/// Runs `git remote get-url origin`. A non-zero exit status is treated as
	/// "no origin" and returns `None` rather than an error.
	///
	/// # Errors
	///
	/// Returns an error if the git command cannot be executed at all.
	pub(crate) fn remote_origin_url(&self) -> anyhow::Result<Option<String>> {
		let output = self
			.env
			.run("git", &["remote", "get-url", "origin"], &self.path)
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
	pub(crate) fn checkout_or_reset_branch(&self, branch: &str) -> anyhow::Result<()> {
		let output = self
			.env
			.run_mut("git", &["checkout", "-B", branch], &self.path)
			.context("Failed to run git checkout")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
	pub(crate) fn force_push_branch(&self, branch: &str) -> anyhow::Result<()> {
		let output = self
			.env
			.run_mut(
				"git",
				&["push", "--force-with-lease", "origin", branch],
				&self.path,
			)
			.context("Failed to run git force push branch")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
	pub(crate) fn delete_tag(&self, tag: &str) -> anyhow::Result<()> {
		let output = self
			.env
			.run_mut("git", &["tag", "-d", tag], &self.path)
			.context("Failed to run git tag -d")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			bail!("git tag -d failed: {stderr}");
		}

		Ok(())
	}

	/// Pushes a specific tag to origin.
	///
	/// Runs `git push origin <tag>`, pushing only that tag rather than all local tags.
	///
	/// # Errors
	///
	/// Returns an error if `git push` exits with a non-zero status.
	pub(crate) fn push_tag(&self, tag: &str) -> anyhow::Result<()> {
		let output = self
			.env
			.run_mut("git", &["push", "origin", tag], &self.path)
			.context("Failed to run git push tag")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
	pub(crate) fn rev_list_count(&self, range: &str) -> anyhow::Result<usize> {
		let output = self
			.env
			.run("git", &["rev-list", "--count", range], &self.path)
			.context("Failed to run git rev-list --count")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
	pub(crate) fn log_message(&self, rev: &str) -> anyhow::Result<String> {
		let output = self
			.env
			.run("git", &["log", "-1", "--format=%B", rev], &self.path)
			.context("Failed to run git log")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
	pub(crate) fn diff_tree_names(&self, commit: &str) -> anyhow::Result<Vec<String>> {
		let output = self
			.env
			.run(
				"git",
				&["diff-tree", "--no-commit-id", "-r", "--name-only", commit],
				&self.path,
			)
			.context("Failed to run git diff-tree")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
	pub(crate) fn log_added_commit(
		&self,
		path: &std::path::Path,
	) -> anyhow::Result<Option<String>> {
		let path_str = path.to_string_lossy();
		let output = self
			.env
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
			.context("Failed to run git log --diff-filter=A")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
	pub(crate) fn log_subject(&self, rev: &str) -> anyhow::Result<String> {
		let output = self
			.env
			.run("git", &["log", "-1", "--format=%s", rev], &self.path)
			.context("Failed to run git log --format=%s")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
	/// # Errors
	///
	/// Returns an error if `git diff` exits with a non-zero status.
	pub(crate) fn diff_names(&self, extra_args: &[&str]) -> anyhow::Result<Vec<String>> {
		let mut args = vec!["diff", "--name-only"];
		args.extend_from_slice(extra_args);
		let output = self
			.env
			.run("git", &args, &self.path)
			.context("Failed to run git diff --name-only")?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
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
