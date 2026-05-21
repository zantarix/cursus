use crate::filesystem::*;
use crate::path::AbsolutePath;

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
