//! Shared test helpers for integration tests.

#![allow(dead_code)]

use chronicle::model::config::{Config, PackageManager};
use chronicle::package_manager::{CargoConfig, NpmConfig};
use tempfile::TempDir;

/// Creates a temporary directory with a `.git` folder to simulate a git repository.
pub fn temp_git_repo() -> TempDir {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	dir
}

/// Creates a temporary git repository with a Chronicle config file.
pub fn temp_git_repo_with_config(pm: PackageManager) -> TempDir {
	let dir = temp_git_repo();
	let config = match pm {
		PackageManager::Npm => Config::new(dir.path()).with_npm(NpmConfig::default()),
		PackageManager::Cargo => Config::new(dir.path()).with_cargo(CargoConfig::default()),
	};
	config.save().unwrap();
	dir
}

/// Creates a temporary git repository with a config and matching package manifest.
pub fn temp_git_repo_with_project(pm: PackageManager) -> TempDir {
	let dir = temp_git_repo();
	let config = match pm {
		PackageManager::Npm => Config::new(dir.path()).with_npm(NpmConfig::default()),
		PackageManager::Cargo => Config::new(dir.path()).with_cargo(CargoConfig::default()),
	};
	config.save().unwrap();
	match pm {
		PackageManager::Npm => {
			std::fs::write(
				dir.path().join("package.json"),
				r#"{"name": "test-project", "version": "0.1.0"}"#,
			)
			.unwrap();
		}
		PackageManager::Cargo => {
			std::fs::write(
				dir.path().join("Cargo.toml"),
				"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
			)
			.unwrap();
			// Create src/lib.rs so cargo can generate a valid Cargo.lock
			std::fs::create_dir_all(dir.path().join("src")).unwrap();
			std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
		}
	}
	dir
}

/// Creates a temporary git repository with a config and package manifest in a subfolder.
pub fn temp_git_repo_with_project_in_subfolder(pm: PackageManager, subfolder: &str) -> TempDir {
	let dir = temp_git_repo();
	let mut config = match pm {
		PackageManager::Npm => Config::new(dir.path()).with_npm(NpmConfig::default()),
		PackageManager::Cargo => Config::new(dir.path()).with_cargo(CargoConfig::default()),
	};
	match pm {
		PackageManager::Npm => config.npm.path = Some(subfolder.to_string()),
		PackageManager::Cargo => config.cargo.path = Some(subfolder.to_string()),
	}
	config.save().unwrap();
	let sub_path = dir.path().join(subfolder);
	std::fs::create_dir_all(&sub_path).unwrap();
	match pm {
		PackageManager::Npm => {
			std::fs::write(
				sub_path.join("package.json"),
				r#"{"name": "test-project", "version": "0.1.0"}"#,
			)
			.unwrap();
		}
		PackageManager::Cargo => {
			std::fs::write(
				sub_path.join("Cargo.toml"),
				"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
			)
			.unwrap();
		}
	}
	dir
}
