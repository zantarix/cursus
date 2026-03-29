//! Integration tests for the `init` command.

mod common;

use common::{run_cursus, temp_git_repo};

#[tokio::test]
async fn init_fails_when_not_in_git_repo() {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	let result = run_cursus(["cursus", "--no-interactive", "init"], dir.path()).await;
	let err = result.unwrap_err();
	let msg = format!("{err:#}");
	assert!(
		msg.contains("No git repository found"),
		"Expected 'No git repository found' in error message, got: {msg}"
	);
}

#[tokio::test]
async fn init_no_interactive_returns_error() {
	let dir = temp_git_repo();
	let result = run_cursus(["cursus", "--no-interactive", "init"], dir.path()).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("interactive-only"),
		"Expected 'interactive-only' error, got: {err}"
	);
}
