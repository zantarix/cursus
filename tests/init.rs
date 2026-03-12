//! Integration tests for the `init` command.

mod common;

use common::{run_chronicle, temp_git_repo};

#[test]
fn init_fails_when_not_in_git_repo() {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	let result = run_chronicle(["chronicle", "--no-interactive", "init"], dir.path());
	let err = result.unwrap_err();
	let msg = format!("{err:#}");
	assert!(
		msg.contains("No git repository found"),
		"Expected 'No git repository found' in error message, got: {msg}"
	);
}

#[test]
fn init_no_interactive_returns_error() {
	let dir = temp_git_repo();
	let result = run_chronicle(["chronicle", "--no-interactive", "init"], dir.path());

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("interactive-only"),
		"Expected 'interactive-only' error, got: {err}"
	);
}
