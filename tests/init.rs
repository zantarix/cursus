//! Integration tests for the `init` command.

mod common;

use chronicle::config::{self, Config, PackageManager};
use common::{temp_git_repo, temp_git_repo_with_config};

#[test]
fn init_fails_when_config_already_exists() {
	let config = Config {
		package_manager: PackageManager::Npm,
	};
	let dir = temp_git_repo_with_config(&config);
	let result = chronicle::run(["chronicle", "init"], dir.path());

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("Configuration already exists"),
		"Expected 'Configuration already exists' error, got: {err}"
	);
}

#[test]
fn init_fails_when_config_exists_with_cargo() {
	let config = Config {
		package_manager: PackageManager::Cargo,
	};
	let dir = temp_git_repo_with_config(&config);
	let result = chronicle::run(["chronicle", "init"], dir.path());

	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("already exists"));
}

// CLI argument parsing tests

#[test]
fn run_fails_with_invalid_command() {
	let dir = temp_git_repo();
	let result = chronicle::run(["chronicle", "invalid-command"], dir.path());
	assert!(result.is_err());
}

#[test]
fn run_fails_with_unknown_flag() {
	let dir = temp_git_repo();
	let result = chronicle::run(["chronicle", "--unknown-flag"], dir.path());
	assert!(result.is_err());
}

#[test]
fn run_fails_when_not_in_git_repo() {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	let result = chronicle::run(["chronicle", "init"], dir.path());

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("No git repository found"),
		"Expected 'No git repository found' error, got: {err}"
	);
}

// Config file verification tests

#[test]
fn config_file_is_created_in_correct_location() {
	let dir = temp_git_repo();
	let config = Config {
		package_manager: PackageManager::Npm,
	};

	let path = config::create(dir.path(), &config).unwrap();

	assert_eq!(path, dir.path().join(".chronicle/config.toml"));
	assert!(path.exists());
}

#[test]
fn config_file_contains_correct_content() {
	let dir = temp_git_repo();
	let config = Config {
		package_manager: PackageManager::Cargo,
	};

	config::create(dir.path(), &config).unwrap();
	let loaded = config::load(dir.path()).unwrap();

	assert_eq!(loaded.package_manager, PackageManager::Cargo);
}
