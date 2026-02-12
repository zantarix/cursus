//! Shared test helpers for integration tests.

use chronicle::config::{self, Config};
use tempfile::TempDir;

/// Creates a temporary directory with a `.git` folder to simulate a git repository.
pub fn temp_git_repo() -> TempDir {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	dir
}

/// Creates a temporary git repository with a Chronicle config file.
pub fn temp_git_repo_with_config(config: &Config) -> TempDir {
	let dir = temp_git_repo();
	config::create(dir.path(), config).unwrap();
	dir
}
