//! CLI argument parsing tests (subprocess-based).

mod common;

use common::temp_git_repo;

#[tokio::test]
async fn run_fails_with_invalid_command() {
	let dir = temp_git_repo();
	let (success, _, _) =
		common::run_cursus_subprocess(&["--no-interactive", "invalid-command"], dir.path());
	assert!(!success);
}

#[tokio::test]
async fn run_fails_with_unknown_flag() {
	let dir = temp_git_repo();
	let (success, _, _) =
		common::run_cursus_subprocess(&["--no-interactive", "--unknown-flag"], dir.path());
	assert!(!success);
}

#[tokio::test]
async fn run_succeeds_with_help_flag() {
	let dir = temp_git_repo();
	let (success, _, _) = common::run_cursus_subprocess(&["--help"], dir.path());
	assert!(success);
}

#[tokio::test]
async fn run_succeeds_with_version_flag() {
	let dir = temp_git_repo();
	let (success, _, _) = common::run_cursus_subprocess(&["--version"], dir.path());
	assert!(success);
}

#[tokio::test]
async fn verbose_and_silent_flags_conflict() {
	let dir = temp_git_repo();
	let (success, _, _) =
		common::run_cursus_subprocess(&["-v", "-s", "--no-interactive"], dir.path());
	assert!(!success, "cursus should fail when -v and -s are combined");
}
