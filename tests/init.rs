//! Integration tests for the `init` command.

mod common;

use chronicle::config::{Config, PackageManager};
use common::temp_git_repo_with_config;

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
