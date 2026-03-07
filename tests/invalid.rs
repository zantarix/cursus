//! CLI argument parsing tests

mod common;

use common::temp_git_repo;

#[test]
fn run_fails_with_invalid_command() {
	let dir = temp_git_repo();
	let (success, _, _) =
		common::run_chronicle_subprocess(&["--no-interactive", "invalid-command"], dir.path());
	assert!(!success);
}

#[test]
fn run_fails_with_unknown_flag() {
	let dir = temp_git_repo();
	let (success, _, _) =
		common::run_chronicle_subprocess(&["--no-interactive", "--unknown-flag"], dir.path());
	assert!(!success);
}

#[test]
fn run_succeeds_with_help_flag() {
	let dir = temp_git_repo();
	let (success, _, _) = common::run_chronicle_subprocess(&["--help"], dir.path());
	assert!(success);
}

#[test]
fn run_succeeds_with_version_flag() {
	let dir = temp_git_repo();
	let (success, _, _) = common::run_chronicle_subprocess(&["--version"], dir.path());
	assert!(success);
}
