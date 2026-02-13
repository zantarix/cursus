//! Integration tests for the `change` command.

mod common;

use std::process::ExitCode;

use chronicle::config::{Config, PackageManager};
use common::{temp_git_repo, temp_git_repo_with_config};

#[test]
fn change_fails_when_no_config() {
	let dir = temp_git_repo();
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-t", "minor"],
		dir.path(),
	);

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("No configuration found"),
		"Expected 'No configuration found' error, got: {err}"
	);
}

#[test]
fn change_succeeds_with_major() {
	let config = Config::with_package_manager(PackageManager::Npm);
	let dir = temp_git_repo_with_config(&config);
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-t", "major"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_succeeds_with_minor() {
	let config = Config::with_package_manager(PackageManager::Npm);
	let dir = temp_git_repo_with_config(&config);
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-t", "minor"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_succeeds_with_patch() {
	let config = Config::with_package_manager(PackageManager::Cargo);
	let dir = temp_git_repo_with_config(&config);
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-t", "patch"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_no_interactive_requires_change_type() {
	let config = Config::with_package_manager(PackageManager::Npm);
	let dir = temp_git_repo_with_config(&config);
	let result = chronicle::run(["chronicle", "--no-interactive", "change"], dir.path());

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("--change-type is required"),
		"Expected '--change-type is required' error, got: {err}"
	);
}

#[test]
fn change_is_default_command() {
	// Running without a subcommand should behave like `change`,
	// which fails when no config exists
	let dir = temp_git_repo();
	let result = chronicle::run(["chronicle", "--no-interactive"], dir.path());

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("No configuration found"),
		"Expected 'No configuration found' error (same as change command), got: {err}"
	);
}
