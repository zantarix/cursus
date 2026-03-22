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

	/// Returns the porcelain status of the working tree.
	///
	/// An empty string means the working tree is clean.
	fn status_porcelain(&self) -> anyhow::Result<String>;

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
