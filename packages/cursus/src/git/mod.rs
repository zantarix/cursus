//! Git primitives for Cursus.
//!
//! Provides the [`Git`] trait so that code performing git operations can be
//! backed by different implementations — local command-line git, remote forge
//! APIs, etc.
//!
//! [`GitWorkdir`] is the production implementation that delegates to the `git`
//! binary via [`crate::command::CommandRunner`].

use std::path::{Path, PathBuf};

use crate::path::AbsolutePath;

/// Abstracts git operations for testability and extensibility.
///
/// Methods are split into read-only queries and mutations. The mutation
/// boundary is enforced by the underlying [`crate::command::CommandRunner`]
/// (via [`crate::command::DryRunCommandRunner`]) rather than at the trait
/// level — this matches the existing dry-run architecture (ADR-017).
///
/// Implementations must be thread-safe since the trait is stored as
/// `Arc<dyn Git>` in [`crate::Env`].
pub trait Git: Send + Sync + std::fmt::Debug {
	// ── identity ─────────────────────────────────────────────────────────

	/// Returns the repository root path.
	fn path(&self) -> &AbsolutePath;

	// ── read-only queries ────────────────────────────────────────────────

	/// Returns `true` if the working tree has uncommitted changes.
	fn is_dirty(&self) -> anyhow::Result<bool>;

	/// Returns the name of the current branch, or `None` when HEAD is detached.
	fn current_branch(&self) -> anyhow::Result<Option<String>>;

	/// Returns `true` if the given tag exists in the repository.
	fn tag_exists(&self, tag: &str) -> anyhow::Result<bool>;

	/// Returns the URL of the `origin` remote, or `None` if there is no origin.
	fn remote_origin_url(&self) -> anyhow::Result<Option<String>>;

	/// Returns the number of commits in the given revision range.
	fn rev_list_count(&self, range: &str) -> anyhow::Result<usize>;

	/// Returns the full commit message for the given revision.
	fn log_message(&self, rev: &str) -> anyhow::Result<String>;

	/// Returns the subject line of the given commit.
	fn log_subject(&self, rev: &str) -> anyhow::Result<String>;

	/// Returns the full SHA of the commit that first added the given path.
	///
	/// Returns `Ok(None)` when no commit is found.
	fn log_added_commit(&self, path: &Path) -> anyhow::Result<Option<String>>;

	/// Returns the list of files changed by the given commit.
	fn diff_tree_names(&self, commit: &str) -> anyhow::Result<Vec<String>>;

	/// Returns the list of files reported by `git diff --name-only`.
	///
	/// `extra_args` is appended after `--name-only`.
	fn diff_names(&self, extra_args: &[&str]) -> anyhow::Result<Vec<String>>;

	// ── mutations ────────────────────────────────────────────────────────

	/// Stages the given files for the next git commit.
	fn add(&self, files: &[PathBuf]) -> anyhow::Result<()>;

	/// Creates a git commit with the given message.
	fn commit(&self, message: &str) -> anyhow::Result<()>;

	/// Creates an annotated git tag with the given name and message.
	fn tag(&self, tag_name: &str, message: &str) -> anyhow::Result<()>;

	/// Pushes HEAD to origin.
	fn push(&self) -> anyhow::Result<()>;

	/// Checks out an existing branch.
	fn checkout(&self, branch: &str) -> anyhow::Result<()>;

	/// Creates or resets a branch at the current HEAD.
	fn checkout_or_reset_branch(&self, branch: &str) -> anyhow::Result<()>;

	/// Force-pushes a named branch to origin using `--force-with-lease`.
	fn force_push_branch(&self, branch: &str) -> anyhow::Result<()>;

	/// Deletes a local git tag.
	fn delete_tag(&self, tag: &str) -> anyhow::Result<()>;

	/// Pushes a specific tag to origin.
	fn push_tag(&self, tag: &str) -> anyhow::Result<()>;
}

mod operations;

pub use operations::GitWorkdir;

/// Finds the git working directory by walking up from the given path.
///
/// Returns `Some(path)` if a `.git` directory is found, `None` otherwise.
pub fn find_workdir(
	start: &AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> Option<AbsolutePath> {
	std::iter::successors(Some(start.to_path_buf()), |dir| {
		dir.parent().map(std::path::Path::to_path_buf)
	})
	.find(|dir| AbsolutePath::new(dir.join(".git")).is_ok_and(|p| fs.exists(&p)))
	.and_then(|p| AbsolutePath::new(p).ok())
}

#[cfg(test)]
mod find_workdir_tests {
	use super::*;
	use crate::filesystem::LocalFilesystem;

	fn fs() -> LocalFilesystem {
		LocalFilesystem
	}

	#[test]
	fn returns_none_when_no_git() {
		let dir = tempfile::tempdir().unwrap();
		assert!(find_workdir(&AbsolutePath::new(dir.path()).unwrap(), &fs()).is_none());
	}

	#[test]
	fn finds_git_in_current_dir() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let result = find_workdir(&AbsolutePath::new(dir.path()).unwrap(), &fs());
		assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
	}

	#[test]
	fn finds_git_in_parent_dir() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let subdir = dir.path().join("subdir");
		std::fs::create_dir(&subdir).unwrap();
		let result = find_workdir(&AbsolutePath::new(&subdir).unwrap(), &fs());
		assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
	}

	#[test]
	fn finds_git_in_nested_parent() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let nested = dir.path().join("a/b/c");
		std::fs::create_dir_all(&nested).unwrap();
		let result = find_workdir(&AbsolutePath::new(&nested).unwrap(), &fs());
		assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
	}

	#[test]
	fn stops_at_first_git() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let inner = dir.path().join("inner");
		std::fs::create_dir_all(inner.join(".git")).unwrap();
		let result = find_workdir(&AbsolutePath::new(&inner).unwrap(), &fs());
		assert_eq!(result, Some(AbsolutePath::new(inner).unwrap()));
	}
}
