//! CLI argument parsing tests

mod common;

use common::temp_git_repo;

#[test]
fn run_fails_with_invalid_command() {
	let dir = temp_git_repo();
	let result = chronicle::run(
		["chronicle", "--no-interactive", "invalid-command"],
		dir.path(),
	);
	assert!(result.is_err());
}

#[test]
fn run_fails_with_unknown_flag() {
	let dir = temp_git_repo();
	let result = chronicle::run(
		["chronicle", "--no-interactive", "--unknown-flag"],
		dir.path(),
	);
	assert!(result.is_err());
}
