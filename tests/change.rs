//! Integration tests for the `change` command.

mod common;

use std::process::ExitCode;

use chronicle::config::{Config, PackageManager};
use common::{temp_git_repo, temp_git_repo_with_config, temp_git_repo_with_project};

#[test]
fn change_fails_when_no_config() {
	let dir = temp_git_repo();
	let result = chronicle::run(
		[
			"chronicle",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
		],
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
		[
			"chronicle",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
		],
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
		[
			"chronicle",
			"--no-interactive",
			"change",
			"-t",
			"major",
			"-m",
			"test",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_succeeds_with_minor() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	let result = chronicle::run(
		[
			"chronicle",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_succeeds_with_patch() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	let result = chronicle::run(
		[
			"chronicle",
			"--no-interactive",
			"change",
			"-t",
			"patch",
			"-m",
			"test",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn change_no_interactive_requires_change_type() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-m", "test"],
		dir.path(),
	);

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
			"-m",
			"test",
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
			"-m",
			"test",
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

#[test]
fn change_no_interactive_requires_message() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	let result = chronicle::run(
		["chronicle", "--no-interactive", "change", "-t", "minor"],
		dir.path(),
	);

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("--message is required"),
		"Expected '--message is required' error, got: {err}"
	);
}

#[test]
fn change_with_message_creates_changeset_file() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	let result = chronicle::run(
		[
			"chronicle",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"Added a new feature",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// Find the changeset file (should be the only .md file in .chronicle besides config)
	let chronicle_dir = dir.path().join(".chronicle");
	let md_files: Vec<_> = std::fs::read_dir(&chronicle_dir)
		.unwrap()
		.filter_map(|e| e.ok())
		.filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
		.collect();

	assert_eq!(md_files.len(), 1, "Expected exactly one changeset file");

	let content = std::fs::read_to_string(md_files[0].path()).unwrap();
	assert!(
		content.starts_with("+++\n"),
		"Should start with TOML frontmatter delimiter"
	);
	assert!(
		content.contains("test-project = \"minor\""),
		"Should contain project with change type, got: {content}"
	);
	assert!(
		content.contains("Added a new feature"),
		"Should contain the message, got: {content}"
	);
}

#[test]
fn change_with_message_and_project() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	let result = chronicle::run(
		[
			"chronicle",
			"--no-interactive",
			"change",
			"-t",
			"patch",
			"-p",
			"test-project",
			"-m",
			"Fixed a bug",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let chronicle_dir = dir.path().join(".chronicle");
	let md_files: Vec<_> = std::fs::read_dir(&chronicle_dir)
		.unwrap()
		.filter_map(|e| e.ok())
		.filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
		.collect();

	assert_eq!(md_files.len(), 1);

	let content = std::fs::read_to_string(md_files[0].path()).unwrap();
	assert!(
		content.contains("test-project = \"patch\""),
		"Should contain specific project with patch type, got: {content}"
	);
	assert!(
		content.contains("Fixed a bug"),
		"Should contain the message, got: {content}"
	);
}
