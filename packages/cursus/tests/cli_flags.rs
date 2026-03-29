//! Integration tests for CLI flag acceptance (library-side).

mod common;

use cursus::model::config::PackageManager;

#[tokio::test]
async fn verbose_flag_accepted() {
	let dir = common::temp_git_repo_with_config(PackageManager::Cargo).await;
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
	)
	.unwrap();
	let result = common::run_cursus(
		[
			"cursus",
			"--verbose",
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
	assert!(result.is_ok(), "cursus should accept --verbose flag");
}

#[tokio::test]
async fn silent_flag_accepted() {
	let dir = common::temp_git_repo_with_config(PackageManager::Cargo).await;
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
	)
	.unwrap();
	let result = common::run_cursus(
		[
			"cursus",
			"--silent",
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
	assert!(result.is_ok(), "cursus should accept --silent flag");
}
