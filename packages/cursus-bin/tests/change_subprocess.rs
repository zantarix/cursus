//! Subprocess tests for the `change` command that require capturing stdout/stderr.

mod common;

use cursus::model::config::PackageManager;

#[tokio::test]
async fn change_dry_run_does_not_write_changeset_file() {
	let dir = common::temp_git_repo_with_project(PackageManager::Npm).await;
	let (success, stdout, _stderr) = common::run_cursus_subprocess(
		&[
			"--dry-run",
			"--no-interactive",
			"change",
			"-t",
			"patch",
			"-m",
			"test dry run",
		],
		dir.path(),
	);

	assert!(success, "Expected success exit code");

	// The rendered changeset should be printed to stdout.
	assert!(
		stdout.contains("+++"),
		"Expected changeset frontmatter in stdout, got: {stdout}"
	);
	assert!(
		stdout.contains("test-project = \"patch\""),
		"Expected project/change-type in stdout, got: {stdout}"
	);
	assert!(
		stdout.contains("test dry run"),
		"Expected message in stdout, got: {stdout}"
	);

	// No changeset file should have been written.
	let cursus_dir = dir.path().join(".cursus");
	let md_files: Vec<_> = std::fs::read_dir(&cursus_dir)
		.unwrap()
		.filter_map(|e| e.ok())
		.filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
		.collect();
	assert!(
		md_files.is_empty(),
		"--dry-run must not write changeset files, but found: {md_files:?}"
	);
}
