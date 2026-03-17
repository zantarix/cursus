//! Absolute path newtype to enforce the invariant at the type level.

use std::fmt;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::Context as _;

/// A path that is guaranteed to be absolute.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AbsolutePath(PathBuf);

impl AbsolutePath {
	/// Creates a new `AbsolutePath`, returning an error if the path is not absolute.
	pub fn new(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
		let path = path.into();
		if !path.is_absolute() {
			anyhow::bail!("path must be absolute: {}", path.display());
		}
		Ok(Self(path))
	}

	/// Returns a reference to the underlying `Path`.
	pub fn as_path(&self) -> &Path {
		self.0.as_path()
	}

	/// Consumes this `AbsolutePath` and returns the underlying `PathBuf`.
	pub fn into_path_buf(self) -> PathBuf {
		self.0
	}

	/// Joins `sub` onto this path, canonicalizes the result, and verifies it
	/// remains within this path. Returns an error if the resolved path escapes
	/// the base directory (e.g. via `..` components or symlinks).
	pub fn subpath(&self, sub: impl AsRef<Path>) -> anyhow::Result<AbsolutePath> {
		let joined = self.0.join(sub);
		let canonical_base = std::fs::canonicalize(&self.0)
			.with_context(|| format!("failed to canonicalize base path: {}", self.0.display()))?;
		let canonical_joined = std::fs::canonicalize(&joined).with_context(|| {
			format!(
				"path does not exist or cannot be resolved: {}",
				joined.display()
			)
		})?;
		if !canonical_joined.starts_with(&canonical_base) {
			anyhow::bail!("path escapes base directory: {}", joined.display());
		}
		AbsolutePath::new(canonical_joined)
	}

	/// Expands a glob pattern relative to this path, returning only results
	/// that resolve within this directory. Paths that escape (via `..` or
	/// symlinks) are rejected with an error.
	pub fn safe_glob(&self, pattern: &str) -> anyhow::Result<Vec<AbsolutePath>> {
		let full_pattern = self.0.join(pattern);
		let pattern_str = full_pattern
			.to_str()
			.context("Invalid UTF-8 in glob pattern")?;
		let canonical_base = std::fs::canonicalize(&self.0)
			.with_context(|| format!("failed to canonicalize base path: {}", self.0.display()))?;

		let mut results = Vec::new();
		for entry in
			glob::glob(pattern_str).with_context(|| format!("Invalid glob pattern: {pattern}"))?
		{
			let path = entry
				.with_context(|| format!("Failed to read glob entry for pattern: {pattern}"))?;
			let canonical = std::fs::canonicalize(&path).with_context(|| {
				format!("failed to canonicalize glob result: {}", path.display())
			})?;
			if !canonical.starts_with(&canonical_base) {
				anyhow::bail!("glob result escapes base directory: {}", path.display());
			}
			results.push(AbsolutePath::new(canonical)?);
		}
		Ok(results)
	}
}

impl Deref for AbsolutePath {
	type Target = Path;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl AsRef<Path> for AbsolutePath {
	fn as_ref(&self) -> &Path {
		self.0.as_path()
	}
}

impl fmt::Display for AbsolutePath {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.display().fmt(f)
	}
}

impl From<AbsolutePath> for PathBuf {
	fn from(path: AbsolutePath) -> Self {
		path.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn new_succeeds_with_absolute_path() {
		let p = AbsolutePath::new("/foo/bar").unwrap();
		assert_eq!(p.as_path(), Path::new("/foo/bar"));
	}

	#[test]
	fn new_fails_with_relative_path() {
		let result = AbsolutePath::new("foo/bar");
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("must be absolute"));
	}

	#[test]
	fn deref_and_join_work() {
		let p = AbsolutePath::new("/foo").unwrap();
		let joined = p.join("bar");
		assert_eq!(joined, Path::new("/foo/bar"));
	}

	#[test]
	fn display_works() {
		let p = AbsolutePath::new("/foo/bar").unwrap();
		assert_eq!(format!("{p}"), "/foo/bar");
	}

	#[test]
	fn into_path_buf_works() {
		let p = AbsolutePath::new("/foo/bar").unwrap();
		let pb: PathBuf = p.into_path_buf();
		assert_eq!(pb, PathBuf::from("/foo/bar"));
	}

	#[test]
	fn from_trait_gives_path_buf() {
		let pb: PathBuf = AbsolutePath::new("/foo").unwrap().into();
		assert_eq!(pb, PathBuf::from("/foo"));
	}

	// ── subpath ───────────────────────────────────────────────────────────────

	#[test]
	fn subpath_valid_child_succeeds() {
		let dir = tempfile::tempdir().unwrap();
		let child = dir.path().join("child");
		std::fs::create_dir(&child).unwrap();
		let base = AbsolutePath::new(dir.path()).unwrap();
		let result = base.subpath("child").unwrap();
		assert!(result.starts_with(dir.path()));
	}

	#[test]
	fn subpath_dotdot_escape_is_rejected() {
		let outer = tempfile::tempdir().unwrap();
		let repo = outer.path().join("repo");
		std::fs::create_dir(&repo).unwrap();
		let escape = outer.path().join("secret");
		std::fs::create_dir(&escape).unwrap();
		let base = AbsolutePath::new(&repo).unwrap();
		let result = base.subpath("../secret");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(msg.contains("escapes base directory"), "got: {msg}");
	}

	#[cfg(unix)]
	#[test]
	fn subpath_symlink_escape_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		let link = dir.path().join("escape");
		std::os::unix::fs::symlink("/tmp", &link).unwrap();
		let base = AbsolutePath::new(dir.path()).unwrap();
		let result = base.subpath("escape");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(msg.contains("escapes base directory"), "got: {msg}");
	}

	#[test]
	fn subpath_dot_resolves_to_base() {
		let dir = tempfile::tempdir().unwrap();
		let base = AbsolutePath::new(dir.path()).unwrap();
		let result = base.subpath(".").unwrap();
		assert_eq!(*result, *base);
	}

	// ── safe_glob ─────────────────────────────────────────────────────────────

	#[test]
	fn safe_glob_empty_result_is_ok() {
		let dir = tempfile::tempdir().unwrap();
		let base = AbsolutePath::new(dir.path()).unwrap();
		let results = base.safe_glob("no-match-*").unwrap();
		assert!(results.is_empty());
	}

	#[test]
	fn safe_glob_matching_subdirs_succeeds() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("pkg-a")).unwrap();
		std::fs::create_dir(dir.path().join("pkg-b")).unwrap();
		let base = AbsolutePath::new(dir.path()).unwrap();
		let results = base.safe_glob("pkg-*").unwrap();
		assert_eq!(results.len(), 2);
		assert!(results.iter().all(|p| p.starts_with(dir.path())));
	}

	#[test]
	fn safe_glob_dotdot_pattern_escape_is_rejected() {
		let outer = tempfile::tempdir().unwrap();
		let repo = outer.path().join("repo");
		std::fs::create_dir(&repo).unwrap();
		let escape = outer.path().join("secret");
		std::fs::create_dir(&escape).unwrap();
		let base = AbsolutePath::new(&repo).unwrap();
		let result = base.safe_glob("../secret");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(msg.contains("escapes base directory"), "got: {msg}");
	}

	#[cfg(unix)]
	#[test]
	fn safe_glob_symlink_escape_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		let link = dir.path().join("escape");
		std::os::unix::fs::symlink("/tmp", &link).unwrap();
		let base = AbsolutePath::new(dir.path()).unwrap();
		let result = base.safe_glob("escape");
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(msg.contains("escapes base directory"), "got: {msg}");
	}
}
