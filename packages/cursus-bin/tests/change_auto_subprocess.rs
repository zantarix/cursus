//! Subprocess tests for `cursus change --auto` clap argument conflicts.

mod common;

#[tokio::test]
async fn change_auto_conflicts_with_change_type() {
	let (success, _stdout, stderr) = common::run_cursus_subprocess(
		&["--no-interactive", "change", "--auto", "-t", "minor"],
		std::env::temp_dir().as_path(),
	);
	assert!(!success);
	assert!(
		stderr.contains("auto") || stderr.contains("change-type") || stderr.contains("conflict"),
		"Expected conflict error, got: {stderr}"
	);
}

#[tokio::test]
async fn change_auto_conflicts_with_message() {
	let (success, _stdout, stderr) = common::run_cursus_subprocess(
		&["--no-interactive", "change", "--auto", "-m", "hello"],
		std::env::temp_dir().as_path(),
	);
	assert!(!success);
	assert!(
		stderr.contains("auto") || stderr.contains("message") || stderr.contains("conflict"),
		"Expected conflict error, got: {stderr}"
	);
}
