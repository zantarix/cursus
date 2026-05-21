//! Git primitives for Cursus.
//!
//! Provides the [`Git`] trait so that code performing git operations can be
//! backed by different implementations — local command-line git, remote forge
//! APIs, etc.
//!
//! [`GitWorkdir`] is the production implementation that delegates to the `git`
//! binary via [`crate::command::CommandRunner`].

use std::path::{Path, PathBuf};

use async_trait::async_trait;

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
#[async_trait]
pub trait Git: Send + Sync + std::fmt::Debug {
	// ── identity ─────────────────────────────────────────────────────────

	/// Returns the repository root path.
	fn path(&self) -> &AbsolutePath;

	// ── read-only queries ────────────────────────────────────────────────

	/// Returns `true` if the working tree has uncommitted changes.
	async fn is_dirty(&self) -> anyhow::Result<bool>;

	/// Returns the name of the current branch, or `None` when HEAD is detached.
	async fn current_branch(&self) -> anyhow::Result<Option<String>>;

	/// Returns `true` if the given tag exists in the repository.
	async fn tag_exists(&self, tag: &str) -> anyhow::Result<bool>;

	/// Returns the URL of the `origin` remote, or `None` if there is no origin.
	async fn remote_origin_url(&self) -> anyhow::Result<Option<String>>;

	/// Returns the number of commits in the given revision range.
	async fn rev_list_count(&self, range: &str) -> anyhow::Result<usize>;

	/// Returns the full commit message for the given revision.
	async fn log_message(&self, rev: &str) -> anyhow::Result<String>;

	/// Returns the subject line of the given commit.
	async fn log_subject(&self, rev: &str) -> anyhow::Result<String>;

	/// Returns the full SHA of the commit that first added the given path.
	///
	/// Returns `Ok(None)` when no commit is found.
	async fn log_added_commit(&self, path: &Path) -> anyhow::Result<Option<String>>;

	/// Returns the list of files changed by the given commit.
	async fn diff_tree_names(&self, commit: &str) -> anyhow::Result<Vec<String>>;

	/// Returns the list of files reported by `git diff --name-only`.
	///
	/// `extra_args` is appended after `--name-only`.
	async fn diff_names(&self, extra_args: &[&str]) -> anyhow::Result<Vec<String>>;

	/// Returns the full SHA of the current HEAD commit.
	async fn head_sha(&self) -> anyhow::Result<String>;

	// ── mutations ────────────────────────────────────────────────────────

	/// Stages the given files for the next git commit.
	async fn add(&self, files: &[PathBuf]) -> anyhow::Result<()>;

	/// Creates a git commit with the given message.
	async fn commit(&self, message: &str) -> anyhow::Result<()>;

	/// Creates an annotated git tag with the given name and message.
	async fn tag(&self, tag_name: &str, message: &str) -> anyhow::Result<()>;

	/// Pushes HEAD to origin.
	async fn push(&self) -> anyhow::Result<()>;

	/// Checks out an existing branch.
	async fn checkout(&self, branch: &str) -> anyhow::Result<()>;

	/// Creates or resets a branch at the current HEAD.
	async fn checkout_or_reset_branch(&self, branch: &str) -> anyhow::Result<()>;

	/// Force-pushes a named branch to origin using `--force-with-lease`.
	async fn force_push_branch(&self, branch: &str) -> anyhow::Result<()>;

	/// Deletes a local git tag.
	async fn delete_tag(&self, tag: &str) -> anyhow::Result<()>;

	/// Pushes a specific tag to origin.
	async fn push_tag(&self, tag: &str) -> anyhow::Result<()>;
}

mod operations;
pub(crate) mod ref_format;
pub(crate) mod signed_commit;

pub use operations::GitWorkdir;
pub use signed_commit::SignedCommitGit;

#[cfg(test)]
mod tests;

/// Finds the git working directory by walking up from the given path.
///
/// Returns `Some(path)` if a `.git` directory is found, `None` otherwise.
pub async fn find_workdir(
	start: &AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> Option<AbsolutePath> {
	let mut dir = start.to_path_buf();
	loop {
		if let Ok(git_path) = AbsolutePath::new(dir.join(".git"))
			&& fs.exists(&git_path).await.unwrap_or(false)
		{
			return AbsolutePath::new(&dir).ok();
		}
		if !dir.pop() {
			return None;
		}
	}
}
