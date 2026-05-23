use std::path::{Path, PathBuf};

use crate::filesystem::LocalFilesystem;
use crate::path::AbsolutePath;

fn fs() -> LocalFilesystem {
	LocalFilesystem
}

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
fn child_produces_expected_path() {
	let p = AbsolutePath::new("/repo").unwrap();
	let child = p.child("Cargo.toml");
	assert_eq!(child.as_path(), Path::new("/repo/Cargo.toml"));
}

#[test]
fn child_with_nested_relative_path() {
	let p = AbsolutePath::new("/repo").unwrap();
	let child = p.child(".cursus/config.toml");
	assert_eq!(child.as_path(), Path::new("/repo/.cursus/config.toml"));
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

#[tokio::test]
async fn subpath_valid_child_succeeds() {
	let dir = tempfile::tempdir().unwrap();
	let child = dir.path().join("child");
	std::fs::create_dir(&child).unwrap();
	let base = AbsolutePath::new(dir.path()).unwrap();
	let result = base.subpath("child", &fs()).await.unwrap();
	assert!(result.starts_with(dir.path()));
}

#[tokio::test]
async fn subpath_dotdot_escape_is_rejected() {
	let outer = tempfile::tempdir().unwrap();
	let repo = outer.path().join("repo");
	std::fs::create_dir(&repo).unwrap();
	let escape = outer.path().join("secret");
	std::fs::create_dir(&escape).unwrap();
	let base = AbsolutePath::new(&repo).unwrap();
	let result = base.subpath("../secret", &fs()).await;
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(msg.contains("escapes base directory"), "got: {msg}");
}

#[cfg(unix)]
#[tokio::test]
async fn subpath_symlink_escape_is_rejected() {
	let dir = tempfile::tempdir().unwrap();
	let link = dir.path().join("escape");
	std::os::unix::fs::symlink("/tmp", &link).unwrap();
	let base = AbsolutePath::new(dir.path()).unwrap();
	let result = base.subpath("escape", &fs()).await;
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(msg.contains("escapes base directory"), "got: {msg}");
}

#[tokio::test]
async fn subpath_dot_resolves_to_base() {
	let dir = tempfile::tempdir().unwrap();
	let base = AbsolutePath::new(dir.path()).unwrap();
	let result = base.subpath(".", &fs()).await.unwrap();
	assert_eq!(*result, *base);
}

// ── safe_glob ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn safe_glob_empty_result_is_ok() {
	let dir = tempfile::tempdir().unwrap();
	let base = AbsolutePath::new(dir.path()).unwrap();
	let results = base.safe_glob("no-match-*", &fs()).await.unwrap();
	assert!(results.is_empty());
}

#[tokio::test]
async fn safe_glob_matching_subdirs_succeeds() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join("pkg-a")).unwrap();
	std::fs::create_dir(dir.path().join("pkg-b")).unwrap();
	let base = AbsolutePath::new(dir.path()).unwrap();
	let results = base.safe_glob("pkg-*", &fs()).await.unwrap();
	assert_eq!(results.len(), 2);
	assert!(results.iter().all(|p| p.starts_with(dir.path())));
}

#[tokio::test]
async fn safe_glob_dotdot_pattern_escape_is_rejected() {
	let outer = tempfile::tempdir().unwrap();
	let repo = outer.path().join("repo");
	std::fs::create_dir(&repo).unwrap();
	let escape = outer.path().join("secret");
	std::fs::create_dir(&escape).unwrap();
	let base = AbsolutePath::new(&repo).unwrap();
	let result = base.safe_glob("../secret", &fs()).await;
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(msg.contains("escapes base directory"), "got: {msg}");
}

#[cfg(unix)]
#[tokio::test]
async fn safe_glob_symlink_escape_is_rejected() {
	let dir = tempfile::tempdir().unwrap();
	let link = dir.path().join("escape");
	std::os::unix::fs::symlink("/tmp", &link).unwrap();
	let base = AbsolutePath::new(dir.path()).unwrap();
	let result = base.safe_glob("escape", &fs()).await;
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(msg.contains("escapes base directory"), "got: {msg}");
}
