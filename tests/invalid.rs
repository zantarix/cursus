//! CLI argument parsing tests

mod common;

use common::temp_git_repo;

#[test]
fn run_fails_with_invalid_command() {
	let dir = temp_git_repo();
	let (success, _, _) =
		common::run_cursus_subprocess(&["--no-interactive", "invalid-command"], dir.path());
	assert!(!success);
}

#[test]
fn run_fails_with_unknown_flag() {
	let dir = temp_git_repo();
	let (success, _, _) =
		common::run_cursus_subprocess(&["--no-interactive", "--unknown-flag"], dir.path());
	assert!(!success);
}

#[test]
fn run_succeeds_with_help_flag() {
	let dir = temp_git_repo();
	let (success, _, _) = common::run_cursus_subprocess(&["--help"], dir.path());
	assert!(success);
}

#[test]
fn run_succeeds_with_version_flag() {
	let dir = temp_git_repo();
	let (success, _, _) = common::run_cursus_subprocess(&["--version"], dir.path());
	assert!(success);
}

#[test]
fn verbose_and_silent_flags_conflict() {
	let dir = temp_git_repo();
	let (success, _, _) =
		common::run_cursus_subprocess(&["-v", "-s", "--no-interactive"], dir.path());
	assert!(!success, "cursus should fail when -v and -s are combined");
}

#[test]
fn verbose_flag_accepted() {
	let dir = common::temp_git_repo_with_config(cursus::model::config::PackageManager::Cargo);
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
	);
	assert!(result.is_ok(), "cursus should accept --verbose flag");
}

#[test]
fn silent_flag_accepted() {
	let dir = common::temp_git_repo_with_config(cursus::model::config::PackageManager::Cargo);
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
	);
	assert!(result.is_ok(), "cursus should accept --silent flag");
}
