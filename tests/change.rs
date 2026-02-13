//! Integration tests for the `change` command.

mod common;

use std::process::ExitCode;

use chronicle::config::{Config, PackageManager};
use common::{temp_git_repo, temp_git_repo_with_config, temp_git_repo_with_project};

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
fn change_fails_when_no_projects_found() {
	let config = Config::with_package_manager(PackageManager::Npm);
	let dir = temp_git_repo_with_config(&config);
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-t", "minor"],
		dir.path(),
	);

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("No projects found"),
		"Expected 'No projects found' error, got: {err}"
	);
}

#[test]
fn change_succeeds_with_major() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-t", "major"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_succeeds_with_minor() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-t", "minor"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_succeeds_with_patch() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-t", "patch"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_no_interactive_requires_change_type() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
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

#[test]
fn change_with_project_flag_selects_specific_project() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	let result = chronicle::run(
		[
			"chronicle",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-p",
			"test-project",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_with_unknown_project_fails() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	let result = chronicle::run(
		[
			"chronicle",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-p",
			"nonexistent",
		],
		dir.path(),
	);

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("Unknown project: nonexistent"),
		"Expected 'Unknown project' error, got: {err}"
	);
}
