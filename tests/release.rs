//! Integration tests for the `release` command.

mod common;

use std::process::ExitCode;

use chronicle::config::PackageManager;
use common::{temp_git_repo, temp_git_repo_with_project};

/// Helper to create a changeset file in the .chronicle directory.
fn write_changeset(dir: &std::path::Path, filename: &str, content: &str) {
	let chronicle_dir = dir.join(".chronicle");
	std::fs::create_dir_all(&chronicle_dir).unwrap();
	std::fs::write(chronicle_dir.join(filename), content).unwrap();
}

#[test]
fn release_fails_when_no_config() {
	let dir = temp_git_repo();
	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("No configuration found"),
		"Expected 'No configuration found' error, got: {err}"
	);
}

#[test]
fn release_with_no_changesets_is_noop() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn release_with_single_changeset_cargo() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nAdded a feature\n",
	);

	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// Verify version was bumped
	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("version = \"0.2.0\""),
		"Expected version 0.2.0, got: {cargo_toml}"
	);

	// Verify changeset was deleted
	assert!(
		!dir.path().join(".chronicle/test-change.md").exists(),
		"Changeset file should be deleted"
	);

	// Verify changelog was created
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
	assert!(
		changelog.contains("## 0.2.0"),
		"Changelog should contain version header, got: {changelog}"
	);
	assert!(
		changelog.contains("Added a feature"),
		"Changelog should contain the message, got: {changelog}"
	);
}

#[test]
fn release_with_single_changeset_npm() {
	let dir = temp_git_repo_with_project(PackageManager::Npm);
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"patch\"\n+++\n\nFixed a bug\n",
	);

	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// Verify version was bumped
	let pkg_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		pkg_json.contains("\"0.1.1\""),
		"Expected version 0.1.1, got: {pkg_json}"
	);

	// Verify changeset was deleted
	assert!(!dir.path().join(".chronicle/test-change.md").exists());
}

#[test]
fn release_aggregates_to_highest_change_type() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"change-1.md",
		"+++\ntest-project = \"patch\"\n+++\n\nBug fix\n",
	);
	write_changeset(
		dir.path(),
		"change-2.md",
		"+++\ntest-project = \"minor\"\n+++\n\nNew feature\n",
	);

	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// Minor wins over patch
	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("version = \"0.2.0\""),
		"Expected version 0.2.0 (minor bump), got: {cargo_toml}"
	);

	// Both changesets should be deleted
	assert!(!dir.path().join(".chronicle/change-1.md").exists());
	assert!(!dir.path().join(".chronicle/change-2.md").exists());
}

#[test]
fn release_dry_run_does_not_modify_files() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"major\"\n+++\n\nBreaking change\n",
	);

	let original_cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();

	let result = chronicle::run(
		["chronicle", "--no-interactive", "release", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// Cargo.toml should not be modified
	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert_eq!(
		cargo_toml, original_cargo,
		"Cargo.toml should not change in dry-run"
	);

	// Changeset should still exist
	assert!(
		dir.path().join(".chronicle/test-change.md").exists(),
		"Changeset should not be deleted in dry-run"
	);

	// Changelog should not be created
	assert!(
		!dir.path().join("CHANGELOG.md").exists(),
		"Changelog should not be created in dry-run"
	);
}

#[test]
fn release_major_bump_resets_minor_and_patch() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	// First set the version to something non-trivial
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"test-project\"\nversion = \"1.5.9\"\nedition = \"2024\"\n",
	)
	.unwrap();
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"major\"\n+++\n\nBreaking\n",
	);

	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("version = \"2.0.0\""),
		"Expected version 2.0.0, got: {cargo_toml}"
	);
}

#[test]
fn release_idempotent_no_changesets_after_release() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nFeature\n",
	);

	// First release
	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	// Second release (no changesets)
	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[test]
fn release_changelog_has_proper_sections() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"change-1.md",
		"+++\ntest-project = \"major\"\n+++\n\nBreaking API change\n",
	);
	write_changeset(
		dir.path(),
		"change-2.md",
		"+++\ntest-project = \"minor\"\n+++\n\nNew feature\n",
	);
	write_changeset(
		dir.path(),
		"change-3.md",
		"+++\ntest-project = \"patch\"\n+++\n\nBug fix\n",
	);

	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();

	// Major bump aggregates all, version should be 1.0.0
	assert!(
		changelog.contains("## 1.0.0"),
		"Should have major bumped version, got: {changelog}"
	);
	assert!(
		changelog.contains("### Breaking Changes"),
		"Should have breaking changes section, got: {changelog}"
	);
	assert!(
		changelog.contains("### Features"),
		"Should have features section, got: {changelog}"
	);
	assert!(
		changelog.contains("### Bug Fixes"),
		"Should have bug fixes section, got: {changelog}"
	);
}

#[test]
fn release_successive_releases_prepend_to_changelog() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);

	// First release: minor bump 0.1.0 -> 0.2.0
	write_changeset(
		dir.path(),
		"change-1.md",
		"+++\ntest-project = \"minor\"\n+++\n\nFirst feature\n",
	);
	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	// Second release: patch bump 0.2.0 -> 0.2.1
	write_changeset(
		dir.path(),
		"change-2.md",
		"+++\ntest-project = \"patch\"\n+++\n\nA bug fix\n",
	);
	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();

	// Both versions should be present
	assert!(
		changelog.contains("## 0.2.1"),
		"Should contain newer version, got: {changelog}"
	);
	assert!(
		changelog.contains("## 0.2.0"),
		"Should contain older version, got: {changelog}"
	);

	// Newer entry must appear before older entry
	let pos_new = changelog.find("## 0.2.1").unwrap();
	let pos_old = changelog.find("## 0.2.0").unwrap();
	assert!(
		pos_new < pos_old,
		"Newer version should appear before older version in changelog, got: {changelog}"
	);
}

#[test]
fn release_unknown_package_in_changeset_fails() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\nnonexistent-package = \"minor\"\n+++\n\nSome change\n",
	);

	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("not found in projects"),
		"Expected 'not found in projects' error, got: {err}"
	);
}

#[test]
fn release_package_flag_filters_packages() {
	let dir = temp_git_repo();
	// Create a cargo workspace with two members
	let config = chronicle::config::Config::with_package_manager(PackageManager::Cargo);
	chronicle::config::create(dir.path(), &config).unwrap();
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[workspace]\nmembers = [\"pkg-a\", \"pkg-b\"]\n",
	)
	.unwrap();
	std::fs::create_dir_all(dir.path().join("pkg-a/src")).unwrap();
	std::fs::write(
		dir.path().join("pkg-a/Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
	)
	.unwrap();
	std::fs::write(dir.path().join("pkg-a/src/lib.rs"), "").unwrap();
	std::fs::create_dir_all(dir.path().join("pkg-b/src")).unwrap();
	std::fs::write(
		dir.path().join("pkg-b/Cargo.toml"),
		"[package]\nname = \"pkg-b\"\nversion = \"0.2.0\"\nedition = \"2024\"\n",
	)
	.unwrap();
	std::fs::write(dir.path().join("pkg-b/src/lib.rs"), "").unwrap();

	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n",
	);

	let result = chronicle::run(
		[
			"chronicle",
			"--no-interactive",
			"release",
			"--package",
			"pkg-a",
		],
		dir.path(),
	);
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// pkg-a should be bumped
	let pkg_a_toml = std::fs::read_to_string(dir.path().join("pkg-a/Cargo.toml")).unwrap();
	assert!(
		pkg_a_toml.contains("version = \"0.1.1\""),
		"Expected pkg-a version 0.1.1, got: {pkg_a_toml}"
	);

	// pkg-b should NOT be bumped
	let pkg_b_toml = std::fs::read_to_string(dir.path().join("pkg-b/Cargo.toml")).unwrap();
	assert!(
		pkg_b_toml.contains("version = \"0.2.0\""),
		"Expected pkg-b version 0.2.0 (unchanged), got: {pkg_b_toml}"
	);

	// Changeset should be deleted (consumed by pkg-a release)
	assert!(!dir.path().join(".chronicle/test-change.md").exists());
}

#[test]
fn release_unknown_package_flag_fails() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nSome change\n",
	);

	let result = chronicle::run(
		[
			"chronicle",
			"--no-interactive",
			"release",
			"--package",
			"nonexistent",
		],
		dir.path(),
	);
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("Unknown package: nonexistent"),
		"Expected 'Unknown package: nonexistent' error, got: {err}"
	);
}

#[test]
fn release_updates_cargo_lock_file() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nAdded a feature\n",
	);

	// Remove Cargo.lock if it exists
	let lock_file = dir.path().join("Cargo.lock");
	if lock_file.exists() {
		std::fs::remove_file(&lock_file).unwrap();
	}

	let result = chronicle::run(["chronicle", "--no-interactive", "release"], dir.path());
	if let Err(ref err) = result {
		eprintln!("Release failed: {:#}", err);
	}
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);

	// Verify Cargo.lock was created/updated
	assert!(
		lock_file.exists(),
		"Cargo.lock should be created after release"
	);

	// Verify the lock file contains the new version
	let lock_content = std::fs::read_to_string(&lock_file).unwrap();
	assert!(
		lock_content.contains("0.2.0"),
		"Cargo.lock should contain the new version, got: {lock_content}"
	);
}
