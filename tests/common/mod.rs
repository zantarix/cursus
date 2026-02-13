//! Shared test helpers for integration tests.

#![allow(dead_code)]

use chronicle::config::{self, Config, PackageManager};
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

/// Creates a temporary git repository with a config and matching package manifest.
pub fn temp_git_repo_with_project(pm: PackageManager) -> TempDir {
	let config = Config::with_package_manager(pm);
	let dir = temp_git_repo_with_config(&config);
	match pm {
		PackageManager::Npm => {
			std::fs::write(
				dir.path().join("package.json"),
				r#"{"name": "test-project"}"#,
			)
			.unwrap();
		}
		PackageManager::Cargo => {
			std::fs::write(
				dir.path().join("Cargo.toml"),
				"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
			)
			.unwrap();
		}
	}
	dir
}
