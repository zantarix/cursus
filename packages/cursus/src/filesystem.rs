//! Abstraction over filesystem operations for testability and extensibility.
//!
//! Provides the [`Filesystem`] trait so that code performing file I/O can be
//! backed by different implementations — local disk, remote forge APIs, etc.
//!
//! [`LocalFilesystem`] is the production implementation used by the binary
//! and tests.

use std::path::PathBuf;

use anyhow::Context as _;
use async_trait::async_trait;

use crate::path::AbsolutePath;

/// Abstracts filesystem operations for testability and extensibility.
///
/// All path parameters use [`AbsolutePath`] to enforce at the type level that
/// only absolute paths reach the filesystem. Implementations must be
/// thread-safe since the trait is stored as `Arc<dyn Filesystem>` in
/// [`crate::Env`].
#[async_trait]
pub trait Filesystem: Send + Sync + std::fmt::Debug {
	/// Reads a file's entire contents as a UTF-8 string.
	async fn read_to_string(&self, path: &AbsolutePath) -> anyhow::Result<String>;

	/// Reads a file's entire contents as raw bytes.
	async fn read(&self, path: &AbsolutePath) -> anyhow::Result<Vec<u8>>;

	/// Writes the given contents to a file, creating it if it doesn't exist
	/// and truncating it if it does.
	async fn write(&self, path: &AbsolutePath, contents: &[u8]) -> anyhow::Result<()>;

	/// Creates the directory and all parent directories if they don't exist.
	async fn create_dir_all(&self, path: &AbsolutePath) -> anyhow::Result<()>;

	/// Deletes a file.
	async fn remove_file(&self, path: &AbsolutePath) -> anyhow::Result<()>;

	/// Returns `true` if the path exists on the filesystem.
	async fn exists(&self, path: &AbsolutePath) -> anyhow::Result<bool>;

	/// Returns `true` if the path is a directory.
	async fn is_dir(&self, path: &AbsolutePath) -> anyhow::Result<bool>;

	/// Canonicalizes a path, resolving symlinks and `.`/`..` components.
	async fn canonicalize(&self, path: &AbsolutePath) -> anyhow::Result<PathBuf>;

	/// Expands a glob pattern and returns matching paths.
	async fn glob(&self, pattern: &str) -> anyhow::Result<Vec<PathBuf>>;

	/// Returns the size of a file in bytes.
	async fn file_size(&self, path: &AbsolutePath) -> anyhow::Result<u64>;
}

/// A filesystem implementation backed by the local operating system.
///
/// Delegates each operation to [`tokio::fs`] or [`glob::glob`]. This is the
/// production implementation used by the binary and all tests.
#[derive(Debug)]
pub struct LocalFilesystem;

#[async_trait]
impl Filesystem for LocalFilesystem {
	async fn read_to_string(&self, path: &AbsolutePath) -> anyhow::Result<String> {
		tokio::fs::read_to_string(path.as_path())
			.await
			.with_context(|| format!("Failed to read {}", path.display()))
	}

	async fn read(&self, path: &AbsolutePath) -> anyhow::Result<Vec<u8>> {
		tokio::fs::read(path.as_path())
			.await
			.with_context(|| format!("Failed to read {}", path.display()))
	}

	async fn write(&self, path: &AbsolutePath, contents: &[u8]) -> anyhow::Result<()> {
		tokio::fs::write(path.as_path(), contents)
			.await
			.with_context(|| format!("Failed to write {}", path.display()))
	}

	async fn create_dir_all(&self, path: &AbsolutePath) -> anyhow::Result<()> {
		tokio::fs::create_dir_all(path.as_path())
			.await
			.with_context(|| format!("Failed to create directory {}", path.display()))
	}

	async fn remove_file(&self, path: &AbsolutePath) -> anyhow::Result<()> {
		tokio::fs::remove_file(path.as_path())
			.await
			.with_context(|| format!("Failed to remove {}", path.display()))
	}

	async fn exists(&self, path: &AbsolutePath) -> anyhow::Result<bool> {
		tokio::fs::try_exists(path.as_path())
			.await
			.with_context(|| format!("Failed to check if {} exists", path.display()))
	}

	async fn is_dir(&self, path: &AbsolutePath) -> anyhow::Result<bool> {
		match tokio::fs::metadata(path.as_path()).await {
			Ok(m) => Ok(m.is_dir()),
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
			Err(e) => Err(anyhow::anyhow!(e))
				.with_context(|| format!("Failed to check if {} is a directory", path.display())),
		}
	}

	async fn canonicalize(&self, path: &AbsolutePath) -> anyhow::Result<PathBuf> {
		tokio::fs::canonicalize(path.as_path())
			.await
			.with_context(|| format!("Failed to canonicalize {}", path.display()))
	}

	async fn glob(&self, pattern: &str) -> anyhow::Result<Vec<PathBuf>> {
		let pattern = pattern.to_string();
		tokio::task::spawn_blocking(move || {
			glob::glob(&pattern)
				.with_context(|| format!("Invalid glob pattern: {pattern}"))?
				.collect::<Result<Vec<_>, _>>()
				.context("Failed to read glob entry")
		})
		.await
		.context("spawn_blocking panicked")?
	}

	async fn file_size(&self, path: &AbsolutePath) -> anyhow::Result<u64> {
		tokio::fs::metadata(path.as_path())
			.await
			.map(|m| m.len())
			.with_context(|| format!("Failed to stat {}", path.display()))
	}
}
