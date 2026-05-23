//! Absolute path newtype to enforce the invariant at the type level.

use std::fmt;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::Context as _;

use crate::filesystem::Filesystem;

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

	/// Joins a relative child onto this path, returning a new `AbsolutePath`.
	///
	/// Unlike [`subpath`][Self::subpath], this does **not** canonicalize or
	/// verify that the result stays within the base — it simply appends the
	/// component. Use this for well-known filenames (e.g. `"Cargo.toml"`,
	/// `"CHANGELOG.md"`) where escaping is not a concern.
	///
	/// **Note:** If `sub` is an absolute path, `PathBuf::join` will discard
	/// the base and return `sub` unchanged. Only pass relative components.
	pub fn child(&self, sub: impl AsRef<Path>) -> AbsolutePath {
		let joined = self.0.join(sub);
		debug_assert!(joined.is_absolute(), "child() produced a non-absolute path");
		AbsolutePath(joined)
	}

	/// Joins `sub` onto this path, canonicalizes the result, and verifies it
	/// remains within this path. Returns an error if the resolved path escapes
	/// the base directory (e.g. via `..` components or symlinks).
	pub async fn subpath(
		&self,
		sub: impl AsRef<Path>,
		fs: &dyn Filesystem,
	) -> anyhow::Result<AbsolutePath> {
		let joined = self.0.join(sub);
		let canonical_base = fs
			.canonicalize(self)
			.await
			.with_context(|| format!("failed to canonicalize base path: {}", self.0.display()))?;
		let joined_abs = AbsolutePath::new(&joined).with_context(|| {
			format!(
				"path does not exist or cannot be resolved: {}",
				joined.display()
			)
		})?;
		let canonical_joined = fs.canonicalize(&joined_abs).await.with_context(|| {
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
	pub async fn safe_glob(
		&self,
		pattern: &str,
		fs: &dyn Filesystem,
	) -> anyhow::Result<Vec<AbsolutePath>> {
		let full_pattern = self.0.join(pattern);
		let pattern_str = full_pattern
			.to_str()
			.context("Invalid UTF-8 in glob pattern")?;
		let canonical_base = fs
			.canonicalize(self)
			.await
			.with_context(|| format!("failed to canonicalize base path: {}", self.0.display()))?;

		let mut results = Vec::new();
		for path in fs.glob(pattern_str).await? {
			let abs_path = AbsolutePath::new(&path)
				.with_context(|| format!("glob result is not absolute: {}", path.display()))?;
			let canonical = fs.canonicalize(&abs_path).await.with_context(|| {
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
