//! CLI argument parsing tests

mod common;

use common::temp_git_repo;

#[test]
fn run_fails_with_invalid_command() {
	let dir = temp_git_repo();
	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "invalid-command"],
		dir.path(),
	);
	assert!(result.is_ok_and(|code| code == std::process::ExitCode::FAILURE));
}

#[test]
fn run_fails_with_unknown_flag() {
	let dir = temp_git_repo();
	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "--unknown-flag"],
		dir.path(),
	);
	assert!(result.is_ok_and(|code| code == std::process::ExitCode::FAILURE));
}

#[test]
fn run_succeeds_with_help_flag() {
	let dir = temp_git_repo();
	let result = common::run_chronicle(["chronicle", "--help"], dir.path());
	assert!(result.is_ok_and(|code| code == std::process::ExitCode::SUCCESS));
}

#[test]
fn run_succeeds_with_version_flag() {
	let dir = temp_git_repo();
	let result = common::run_chronicle(["chronicle", "--version"], dir.path());
	assert!(result.is_ok_and(|code| code == std::process::ExitCode::SUCCESS));
}
