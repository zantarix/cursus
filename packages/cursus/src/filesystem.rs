//! Abstraction over filesystem operations for testability and extensibility.
//!
//! Provides the [`Filesystem`] trait so that code performing file I/O can be
//! backed by different implementations — local disk, remote forge APIs, etc.
//!
//! [`LocalFilesystem`] is the production implementation used by the binary
//! and tests.

use std::path::PathBuf;

use anyhow::Context as _;

use crate::path::AbsolutePath;

/// Abstracts filesystem operations for testability and extensibility.
///
/// All path parameters use [`AbsolutePath`] to enforce at the type level that
/// only absolute paths reach the filesystem. Implementations must be
/// thread-safe since the trait is stored as `Arc<dyn Filesystem>` in
/// [`crate::Env`].
pub trait Filesystem: Send + Sync + std::fmt::Debug {
	/// Reads a file's entire contents as a UTF-8 string.
	fn read_to_string(&self, path: &AbsolutePath) -> anyhow::Result<String>;

	/// Reads a file's entire contents as raw bytes.
	fn read(&self, path: &AbsolutePath) -> anyhow::Result<Vec<u8>>;

	/// Writes the given contents to a file, creating it if it doesn't exist
	/// and truncating it if it does.
	fn write(&self, path: &AbsolutePath, contents: &[u8]) -> anyhow::Result<()>;

	/// Creates the directory and all parent directories if they don't exist.
	fn create_dir_all(&self, path: &AbsolutePath) -> anyhow::Result<()>;

	/// Deletes a file.
	fn remove_file(&self, path: &AbsolutePath) -> anyhow::Result<()>;

	/// Returns `true` if the path exists on the filesystem.
	fn exists(&self, path: &AbsolutePath) -> bool;

	/// Returns `true` if the path is a directory.
	fn is_dir(&self, path: &AbsolutePath) -> bool;

	/// Canonicalizes a path, resolving symlinks and `.`/`..` components.
	fn canonicalize(&self, path: &AbsolutePath) -> anyhow::Result<PathBuf>;

	/// Expands a glob pattern and returns matching paths.
	fn glob(&self, pattern: &str) -> anyhow::Result<Vec<PathBuf>>;
}

/// A filesystem implementation backed by the local operating system.
///
/// Delegates each operation to [`std::fs`] or [`glob::glob`]. This is the
/// production implementation used by the binary and all tests.
#[derive(Debug)]
pub struct LocalFilesystem;

impl Filesystem for LocalFilesystem {
	fn read_to_string(&self, path: &AbsolutePath) -> anyhow::Result<String> {
		std::fs::read_to_string(path.as_path())
			.with_context(|| format!("Failed to read {}", path.display()))
	}

	fn read(&self, path: &AbsolutePath) -> anyhow::Result<Vec<u8>> {
		std::fs::read(path.as_path()).with_context(|| format!("Failed to read {}", path.display()))
	}

	fn write(&self, path: &AbsolutePath, contents: &[u8]) -> anyhow::Result<()> {
		std::fs::write(path.as_path(), contents)
			.with_context(|| format!("Failed to write {}", path.display()))
	}

	fn create_dir_all(&self, path: &AbsolutePath) -> anyhow::Result<()> {
		std::fs::create_dir_all(path.as_path())
			.with_context(|| format!("Failed to create directory {}", path.display()))
	}

	fn remove_file(&self, path: &AbsolutePath) -> anyhow::Result<()> {
		std::fs::remove_file(path.as_path())
			.with_context(|| format!("Failed to remove {}", path.display()))
	}

	fn exists(&self, path: &AbsolutePath) -> bool {
		path.as_path().exists()
	}

	fn is_dir(&self, path: &AbsolutePath) -> bool {
		path.as_path().is_dir()
	}

	fn canonicalize(&self, path: &AbsolutePath) -> anyhow::Result<PathBuf> {
		std::fs::canonicalize(path.as_path())
			.with_context(|| format!("Failed to canonicalize {}", path.display()))
	}

	fn glob(&self, pattern: &str) -> anyhow::Result<Vec<PathBuf>> {
		glob::glob(pattern)
			.with_context(|| format!("Invalid glob pattern: {pattern}"))?
			.collect::<Result<Vec<_>, _>>()
			.context("Failed to read glob entry")
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn local_fs() -> LocalFilesystem {
		LocalFilesystem
	}

	#[test]
	fn read_to_string_reads_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("hello.txt");
		std::fs::write(&file, "hello world").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		assert_eq!(local_fs().read_to_string(&path).unwrap(), "hello world");
	}

	#[test]
	fn read_to_string_missing_file_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("missing.txt")).unwrap();
		let err = local_fs().read_to_string(&path).unwrap_err();
		assert!(err.to_string().contains("Failed to read"), "got: {err}");
	}

	#[test]
	fn read_reads_bytes() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("data.bin");
		std::fs::write(&file, b"\x00\x01\x02").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		assert_eq!(local_fs().read(&path).unwrap(), b"\x00\x01\x02");
	}

	#[test]
	fn read_missing_file_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("missing.bin")).unwrap();
		let err = local_fs().read(&path).unwrap_err();
		assert!(err.to_string().contains("Failed to read"), "got: {err}");
	}

	#[test]
	fn write_creates_and_writes_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("out.txt");
		let path = AbsolutePath::new(&file).unwrap();
		local_fs().write(&path, b"content").unwrap();
		assert_eq!(std::fs::read_to_string(&file).unwrap(), "content");
	}

	#[test]
	fn write_overwrites_existing_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("out.txt");
		std::fs::write(&file, "old").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		local_fs().write(&path, b"new").unwrap();
		assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
	}

	#[test]
	fn create_dir_all_creates_nested_dirs() {
		let dir = tempfile::tempdir().unwrap();
		let nested = dir.path().join("a/b/c");
		let path = AbsolutePath::new(&nested).unwrap();
		local_fs().create_dir_all(&path).unwrap();
		assert!(nested.is_dir());
	}

	#[test]
	fn remove_file_deletes_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("doomed.txt");
		std::fs::write(&file, "bye").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		local_fs().remove_file(&path).unwrap();
		assert!(!file.exists());
	}

	#[test]
	fn remove_file_missing_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("missing.txt")).unwrap();
		let err = local_fs().remove_file(&path).unwrap_err();
		assert!(err.to_string().contains("Failed to remove"), "got: {err}");
	}

	#[test]
	fn exists_returns_true_for_existing_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("here.txt");
		std::fs::write(&file, "").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		assert!(local_fs().exists(&path));
	}

	#[test]
	fn exists_returns_false_for_missing_path() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("nope")).unwrap();
		assert!(!local_fs().exists(&path));
	}

	#[test]
	fn is_dir_returns_true_for_directory() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path()).unwrap();
		assert!(local_fs().is_dir(&path));
	}

	#[test]
	fn is_dir_returns_false_for_file() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join("file.txt");
		std::fs::write(&file, "").unwrap();
		let path = AbsolutePath::new(&file).unwrap();
		assert!(!local_fs().is_dir(&path));
	}

	#[test]
	fn canonicalize_resolves_path() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path()).unwrap();
		let canonical = local_fs().canonicalize(&path).unwrap();
		assert!(canonical.is_absolute());
	}

	#[test]
	fn canonicalize_missing_path_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = AbsolutePath::new(dir.path().join("missing")).unwrap();
		let err = local_fs().canonicalize(&path).unwrap_err();
		assert!(
			err.to_string().contains("Failed to canonicalize"),
			"got: {err}"
		);
	}

	#[test]
	fn glob_matches_files() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("a.txt"), "").unwrap();
		std::fs::write(dir.path().join("b.txt"), "").unwrap();
		std::fs::write(dir.path().join("c.rs"), "").unwrap();
		let pattern = format!("{}/*.txt", dir.path().display());
		let results = local_fs().glob(&pattern).unwrap();
		assert_eq!(results.len(), 2);
	}

	#[test]
	fn glob_no_matches_returns_empty() {
		let dir = tempfile::tempdir().unwrap();
		let pattern = format!("{}/*.xyz", dir.path().display());
		let results = local_fs().glob(&pattern).unwrap();
		assert!(results.is_empty());
	}

	#[test]
	fn glob_invalid_pattern_returns_error() {
		let err = local_fs().glob("[invalid").unwrap_err();
		assert!(
			err.to_string().contains("Invalid glob pattern"),
			"got: {err}"
		);
	}
}
