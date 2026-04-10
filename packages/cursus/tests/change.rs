//! Integration tests for the `change` command.

mod common;

use std::process::ExitCode;
use std::sync::Arc;

use common::{
	temp_git_repo, temp_git_repo_with_config, temp_git_repo_with_project,
	temp_git_repo_with_project_in_subfolder,
};
use cursus::command::RealCommandRunner;
use cursus::filesystem::LocalFilesystem;
use cursus::model::config::PackageManager;

#[tokio::test]
async fn change_fails_when_no_config() {
	let dir = temp_git_repo();
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("No configuration found"),
		"Expected 'No configuration found' error, got: {err}"
	);
}

#[tokio::test]
async fn change_fails_when_no_projects_found() {
	let dir = temp_git_repo_with_config(PackageManager::Npm).await;
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("No projects found"),
		"Expected 'No projects found' error, got: {err}"
	);
}

#[tokio::test]
async fn change_succeeds_with_major() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"major",
			"-m",
			"test",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// Verify the changeset file was created with the correct change type.
	let changeset_files: Vec<_> = std::fs::read_dir(dir.path().join(".cursus"))
		.expect("Expected .cursus/ directory to exist")
		.filter_map(|e| e.ok())
		.filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
		.collect();
	assert!(
		!changeset_files.is_empty(),
		"Expected a changeset file in .cursus/"
	);
	let content = std::fs::read_to_string(changeset_files[0].path()).unwrap();
	assert!(
		content.contains("major"),
		"Changeset should record a major change, got: {content}"
	);
}

#[tokio::test]
async fn change_succeeds_with_minor() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// Verify the changeset file was created with the correct change type.
	let changeset_files: Vec<_> = std::fs::read_dir(dir.path().join(".cursus"))
		.expect("Expected .cursus/ directory to exist")
		.filter_map(|e| e.ok())
		.filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
		.collect();
	assert!(
		!changeset_files.is_empty(),
		"Expected a changeset file in .cursus/"
	);
	let content = std::fs::read_to_string(changeset_files[0].path()).unwrap();
	assert!(
		content.contains("minor"),
		"Changeset should record a minor change, got: {content}"
	);
}

#[tokio::test]
async fn change_succeeds_with_patch() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"patch",
			"-m",
			"test",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[tokio::test]
async fn change_no_interactive_requires_change_type() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "-m", "test"],
		dir.path(),
	)
	.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("--change-type is required"),
		"Expected '--change-type is required' error, got: {err}"
	);
}

#[tokio::test]
async fn change_is_default_command() {
	// Running without a subcommand should behave like `change`,
	// which fails when no config exists
	let dir = temp_git_repo();
	let result = common::run_cursus(["cursus", "--no-interactive"], dir.path()).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("No configuration found"),
		"Expected 'No configuration found' error (same as change command), got: {err}"
	);
}

#[tokio::test]
async fn change_with_project_flag_selects_specific_project() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	let result = common::run_cursus(
		[
			"cursus",
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
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[tokio::test]
async fn change_with_unknown_project_fails() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	let result = common::run_cursus(
		[
			"cursus",
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
	)
	.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("Unknown project: nonexistent"),
		"Expected 'Unknown project' error, got: {err}"
	);
}

#[tokio::test]
async fn change_no_interactive_requires_message() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	let result = common::run_cursus(
		["cursus", "--no-interactive", "change", "-t", "minor"],
		dir.path(),
	)
	.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("--message is required"),
		"Expected '--message is required' error, got: {err}"
	);
}

#[tokio::test]
async fn change_with_message_creates_changeset_file() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"Added a new feature",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// Find the changeset file (should be the only .md file in .cursus besides config)
	let cursus_dir = dir.path().join(".cursus");
	let md_files: Vec<_> = std::fs::read_dir(&cursus_dir)
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

#[tokio::test]
async fn change_with_message_and_project() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	let result = common::run_cursus(
		[
			"cursus",
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
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let cursus_dir = dir.path().join(".cursus");
	let md_files: Vec<_> = std::fs::read_dir(&cursus_dir)
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

#[tokio::test]
async fn change_succeeds_with_npm_project_in_subfolder() {
	let dir = temp_git_repo_with_project_in_subfolder(PackageManager::Npm, "frontend").await;
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test subfolder",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[tokio::test]
async fn change_succeeds_with_cargo_project_in_subfolder() {
	let dir = temp_git_repo_with_project_in_subfolder(PackageManager::Cargo, "backend").await;
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"patch",
			"-m",
			"test subfolder",
		],
		dir.path(),
	)
	.await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[tokio::test]
async fn change_interactive_with_message_does_not_open_editor() {
	// When --change-type is supplied, change::run() returns immediately without
	// entering the TUI. This means we can reach the open-editor guard in
	// cmd_change while in interactive mode (no --no-interactive) with a message
	// already provided.
	//
	// The condition `if args.message.is_none()` must NOT open the editor when a
	// message is present. The Env contains a nonexistent VISUAL binary, so any
	// call to open_editor returns an error — this catches mutations that would
	// cause the editor to be opened unnecessarily.
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	let runner = Arc::new(RealCommandRunner) as Arc<dyn cursus::command::CommandRunner>;
	let path = cursus::path::AbsolutePath::new(dir.path()).unwrap();
	let git = Arc::new(cursus::git::GitWorkdir::new(Arc::clone(&runner), path));
	let env = cursus::Env::new(runner, Arc::new(LocalFilesystem), git)
		.with_editor("__cursus_test_nonexistent_editor__".to_string());
	let cli = clap::Parser::parse_from(["cursus", "change", "-t", "minor", "-m", "bump"]);
	let config = cursus::model::config::load(env.fs(), env.git().path())
		.await
		.unwrap();
	let result = cursus::run(cli, env, config).await;
	assert_eq!(result.expect("Expected success"), ExitCode::SUCCESS);
}
