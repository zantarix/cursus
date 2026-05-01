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

#[cfg(test)]
mod tests {
	use super::*;

	fn local_fs() -> LocalFilesystem {
		LocalFilesystem
	}

	#[tokio::test]
	async fn read_to_string_reads_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("hello.txt");
		std::fs::write(&file, "hello world").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		assert_eq!(
			local_fs().read_to_string(&path).await.unwrap(),
			"hello world"
		);
	}

	#[tokio::test]
	async fn read_to_string_missing_file_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("missing.txt")).unwrap();
		let err = local_fs().read_to_string(&path).await.unwrap_err();
		assert!(err.to_string().contains("Failed to read"), "got: {err}");
	}

	#[tokio::test]
	async fn read_reads_bytes() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("data.bin");
		std::fs::write(&file, b"\x00\x01\x02").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		assert_eq!(local_fs().read(&path).await.unwrap(), b"\x00\x01\x02");
	}

	#[tokio::test]
	async fn read_missing_file_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("missing.bin")).unwrap();
		let err = local_fs().read(&path).await.unwrap_err();
		assert!(err.to_string().contains("Failed to read"), "got: {err}");
	}

	#[tokio::test]
	async fn write_creates_and_writes_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("out.txt");
		let path = AbsolutePath::new(&file).unwrap();
		local_fs().write(&path, b"content").await.unwrap();
		assert_eq!(std::fs::read_to_string(&file).unwrap(), "content");
	}

	#[tokio::test]
	async fn write_overwrites_existing_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("out.txt");
		std::fs::write(&file, "old").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		local_fs().write(&path, b"new").await.unwrap();
		assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
	}

	#[tokio::test]
	async fn create_dir_all_creates_nested_dirs() {
		let dir = tempfile::tempdir().unwrap();
		let nested = dir.path().join("a/b/c");
		let path = AbsolutePath::new(&nested).unwrap();
		local_fs().create_dir_all(&path).await.unwrap();
		assert!(nested.is_dir());
	}

	#[tokio::test]
	async fn remove_file_deletes_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("doomed.txt");
		std::fs::write(&file, "bye").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		local_fs().remove_file(&path).await.unwrap();
		assert!(!file.exists());
	}

	#[tokio::test]
	async fn remove_file_missing_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("missing.txt")).unwrap();
		let err = local_fs().remove_file(&path).await.unwrap_err();
		assert!(err.to_string().contains("Failed to remove"), "got: {err}");
	}

	#[tokio::test]
	async fn exists_returns_true_for_existing_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("here.txt");
		std::fs::write(&file, "").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		assert!(local_fs().exists(&path).await.unwrap());
	}

	#[tokio::test]
	async fn exists_returns_false_for_missing_path() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("nope")).unwrap();
		assert!(!local_fs().exists(&path).await.unwrap());
	}

	#[tokio::test]
	async fn is_dir_returns_true_for_directory() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path()).unwrap();
		assert!(local_fs().is_dir(&path).await.unwrap());
	}

	#[tokio::test]
	async fn is_dir_returns_false_for_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("file.txt");
		std::fs::write(&file, "").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		assert!(!local_fs().is_dir(&path).await.unwrap());
	}

	#[tokio::test]
	async fn canonicalize_resolves_path() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path()).unwrap();
		let canonical = local_fs().canonicalize(&path).await.unwrap();
		assert!(canonical.is_absolute());
	}

	#[tokio::test]
	async fn canonicalize_missing_path_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("missing")).unwrap();
		let err = local_fs().canonicalize(&path).await.unwrap_err();
		assert!(
			err.to_string().contains("Failed to canonicalize"),
			"got: {err}"
		);
	}

	#[tokio::test]
	async fn glob_matches_files() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("a.txt"), "").unwrap();
		std::fs::write(dir.path().join("b.txt"), "").unwrap();
		std::fs::write(dir.path().join("c.rs"), "").unwrap();
		let pattern = format!("{}/*.txt", dir.path().display());
		let results = local_fs().glob(&pattern).await.unwrap();
		assert_eq!(results.len(), 2);
	}

	#[tokio::test]
	async fn glob_no_matches_returns_empty() {
		let dir = tempfile::tempdir().unwrap();
		let pattern = format!("{}/*.xyz", dir.path().display());
		let results = local_fs().glob(&pattern).await.unwrap();
		assert!(results.is_empty());
	}

	#[tokio::test]
	async fn glob_invalid_pattern_returns_error() {
		let err = local_fs().glob("[invalid").await.unwrap_err();
		assert!(
			err.to_string().contains("Invalid glob pattern"),
			"got: {err}"
		);
	}

	#[tokio::test]
	async fn file_size_returns_correct_size() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("sized.bin");
		std::fs::write(&file, b"hello").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		assert_eq!(local_fs().file_size(&path).await.unwrap(), 5);
	}

	#[tokio::test]
	async fn file_size_missing_file_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("missing.bin")).unwrap();
		let err = local_fs().file_size(&path).await.unwrap_err();
		assert!(err.to_string().contains("Failed to stat"), "got: {err}");
	}
}
