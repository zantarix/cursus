//! Integration tests for the `init` command.

mod common;

use std::process::ExitCode;

use chronicle::model::config::{self, PackageManager};
use common::{temp_git_repo, temp_git_repo_with_config};

#[test]
fn init_fails_when_config_already_exists() {
	let dir = temp_git_repo_with_config(PackageManager::Npm);
	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "init", "-p", "npm"],
		dir.path(),
	);

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("Configuration already exists"),
		"Expected 'Configuration already exists' error, got: {err}"
	);
}

#[test]
fn init_fails_when_not_in_git_repo() {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	let (success, _, stderr) =
		common::run_chronicle_subprocess(&["--no-interactive", "init", "-p", "npm"], dir.path());

	assert!(!success);
	assert!(
		stderr.contains("No git repository found"),
		"Expected 'No git repository found' in stderr, got: {stderr}"
	);
}

#[test]
fn init_creates_config_with_npm() {
	let dir = temp_git_repo();
	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "init", "-p", "npm"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let env = chronicle::Env::new(std::sync::Arc::new(chronicle::command::RealCommandRunner)
		as std::sync::Arc<dyn chronicle::command::CommandRunner>);
	let config = config::load(
		&chronicle::path::AbsolutePath::new(dir.path()).unwrap(),
		&env,
	)
	.unwrap();
	assert!(config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn init_creates_config_with_cargo() {
	let dir = temp_git_repo();
	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "init", "-p", "cargo"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let env = chronicle::Env::new(std::sync::Arc::new(chronicle::command::RealCommandRunner)
		as std::sync::Arc<dyn chronicle::command::CommandRunner>);
	let config = config::load(
		&chronicle::path::AbsolutePath::new(dir.path()).unwrap(),
		&env,
	)
	.unwrap();
	assert!(!config.npm.enabled);
	assert!(config.cargo.enabled);
}

#[test]
fn init_fails_with_invalid_package_manager() {
	let dir = temp_git_repo();
	let (success, _, _) = common::run_chronicle_subprocess(
		&["--no-interactive", "init", "-p", "invalid"],
		dir.path(),
	);

	assert!(!success);
}

#[test]
fn init_creates_config_in_correct_location() {
	let dir = temp_git_repo();
	common::run_chronicle(
		["chronicle", "--no-interactive", "init", "-p", "cargo"],
		dir.path(),
	)
	.unwrap();

	let config_path = dir.path().join(".chronicle/config.toml");
	assert!(config_path.exists());
}

#[test]
fn init_no_interactive_requires_package_manager() {
	let dir = temp_git_repo();
	let result = common::run_chronicle(["chronicle", "--no-interactive", "init"], dir.path());

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("--package-manager is required"),
		"Expected '--package-manager is required' error, got: {err}"
	);
}

#[test]
fn init_no_interactive_flag_works_after_subcommand() {
	let dir = temp_git_repo();
	let result = common::run_chronicle(
		["chronicle", "init", "--no-interactive", "-p", "cargo"],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}
