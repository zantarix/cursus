//! Absolute path newtype to enforce the invariant at the type level.

use std::fmt;
use std::ops::Deref;
use std::path::{Path, PathBuf};

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
}
