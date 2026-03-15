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
mod tests {
	use std::sync::Arc;

	use tempfile::TempDir;

	use super::*;
	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
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
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, dir_abs);
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
		let runner =
			Arc::new(RecordingCommandRunner::new(0).with_stdout(b" M src/main.rs\n".to_vec()));
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
		assert_eq!(invocations[0].args, ["push", "origin", "v1.2.0"]);
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
		let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"v1.0.0\n".to_vec()));
		let dir_abs = abs(&dir);
		let (git, runner) = make_git(runner, dir_abs);
		git.tag_exists("v1.0.0").unwrap();
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
		let dir_abs = abs(&dir);
		let (git, _) = make_git(runner, dir_abs);
		let result = git.tag_exists("v1.0.0").unwrap();
		assert!(result);
	}

	#[test]
	fn git_tag_exists_returns_false_when_output_empty() {
		let dir = temp_dir();
		let runner = recording(0);
		let dir_abs = abs(&dir);
		let (git, _) = make_git(runner, dir_abs);
		let result = git.tag_exists("v1.0.0").unwrap();
		assert!(!result);
	}

	#[test]
	fn git_tag_exists_failure_propagates() {
		let dir = temp_dir();
		let runner = recording_with_stderr(1, b"fatal: not a git repo");
		let dir_abs = abs(&dir);
		let (git, _) = make_git(runner, dir_abs);
		let result = git.tag_exists("v1.0.0");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("git tag -l failed"),
			"Expected 'git tag -l failed', got: {msg}"
		);
	}

	// --- remote_origin_url ---

	#[test]
	fn remote_origin_url_returns_url_on_success() {
		let dir = temp_dir();
		let runner = Arc::new(
			RecordingCommandRunner::new(0)
				.with_stdout(b"https://github.com/owner/repo.git\n".to_vec()),
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
		let runner =
			Arc::new(RecordingCommandRunner::new(0).with_stdout(b"not-a-number\n".to_vec()));
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
		let runner = Arc::new(
			RecordingCommandRunner::new(0).with_stdout(b"src/main.rs\nCargo.toml\n".to_vec()),
		);
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
			RecordingCommandRunner::new(0)
				.with_stdout(b"  git@github.com:owner/repo.git  \n".to_vec()),
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
		let runner = Arc::new(
			RecordingCommandRunner::new(0).with_stdout(b"feat: add thing (#42)\n".to_vec()),
		);
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
}
