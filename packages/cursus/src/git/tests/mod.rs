mod ref_format;
mod signed_commit;

use crate::filesystem::LocalFilesystem;
use crate::git::*;
use crate::path::AbsolutePath;

fn fs() -> LocalFilesystem {
	LocalFilesystem
}

#[tokio::test]
async fn returns_none_when_no_git() {
	let dir = tempfile::tempdir().unwrap();
	assert!(
		find_workdir(&AbsolutePath::new(dir.path()).unwrap(), &fs())
			.await
			.is_none()
	);
}

#[tokio::test]
async fn finds_git_in_current_dir() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let result = find_workdir(&AbsolutePath::new(dir.path()).unwrap(), &fs()).await;
	assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
}

#[tokio::test]
async fn finds_git_in_parent_dir() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let subdir = dir.path().join("subdir");
	std::fs::create_dir(&subdir).unwrap();
	let result = find_workdir(&AbsolutePath::new(&subdir).unwrap(), &fs()).await;
	assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
}

#[tokio::test]
async fn finds_git_in_nested_parent() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let nested = dir.path().join("a/b/c");
	std::fs::create_dir_all(&nested).unwrap();
	let result = find_workdir(&AbsolutePath::new(&nested).unwrap(), &fs()).await;
	assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
}

#[tokio::test]
async fn stops_at_first_git() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let inner = dir.path().join("inner");
	std::fs::create_dir_all(inner.join(".git")).unwrap();
	let result = find_workdir(&AbsolutePath::new(&inner).unwrap(), &fs()).await;
	assert_eq!(result, Some(AbsolutePath::new(inner).unwrap()));
}
