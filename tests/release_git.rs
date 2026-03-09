//! Integration tests for git lifecycle automation in the `release` command.

mod common;

use std::process::{Command, Stdio};

use chronicle::git::{GitConfig, TagFormat};
use chronicle::model::config::PackageManager;
use common::{
	git_log, git_tags, temp_git_repo_with_project, temp_real_git_repo_with_cargo_workspace,
	temp_real_git_repo_with_config,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Creates a git-enabled config with default settings (commit-only, Auto format).
fn git_enabled_config() -> GitConfig {
	GitConfig {
		enabled: Some(true),
		..Default::default()
	}
}

/// Creates a changeset file in the `.chronicle` directory.
fn write_changeset(dir: &std::path::Path, filename: &str, content: &str) {
	let chronicle_dir = dir.join(".chronicle");
	std::fs::create_dir_all(&chronicle_dir).unwrap();
	std::fs::write(chronicle_dir.join(filename), content).unwrap();
}

/// Stages all files and creates a commit with the given message.
fn git_commit_all(dir: &std::path::Path, message: &str) {
	let output = Command::new("git")
		.args(["add", "."])
		.current_dir(dir)
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"git add failed:\n{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let output = Command::new("git")
		.args(["commit", "-m", message])
		.current_dir(dir)
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"git commit failed:\n{}",
		String::from_utf8_lossy(&output.stderr)
	);
}

/// Writes a single-package Cargo setup into the given directory and commits it.
fn setup_single_cargo_package(dir: &std::path::Path, name: &str, version: &str) {
	std::fs::write(
		dir.join("Cargo.toml"),
		format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
	)
	.unwrap();
	std::fs::create_dir_all(dir.join("src")).unwrap();
	std::fs::write(dir.join("src/lib.rs"), "").unwrap();
	git_commit_all(dir, "chore: add package");
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn release_git_disabled_by_default() {
	// Without [git] enabled, a release should succeed without touching git state.
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	write_changeset(
		dir.path(),
		"change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nA feature\n",
	);

	// Uses a fake .git dir, not a real repo — verifies nothing panics when enabled=false.
	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());
}

#[test]
fn release_git_creates_commit() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "0.1.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"minor\"\n+++\n\nA feature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	let log = git_log(dir.path());
	assert!(
		log.iter().any(|msg| msg.contains("chore(release):")),
		"Expected a release commit, got log: {log:?}"
	);
	assert!(
		log[0].contains("my-pkg@0.2.0"),
		"Latest commit should mention my-pkg@0.2.0, got: {}",
		log[0]
	);
}

#[test]
fn release_git_does_not_create_tags() {
	// Tags are now created during publish, not release.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nA fix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	// Release no longer creates tags — publish does.
	assert!(
		git_tags(dir.path()).is_empty(),
		"Release should not create tags (tags are created on publish), got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_git_tag_format_config_no_tags_at_release() {
	// Tag format only affects publish step now; release just commits.
	let config = GitConfig {
		enabled: Some(true),
		tag_format: TagFormat::Prefixed,
		..Default::default()
	};
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, config);
	setup_single_cargo_package(dir.path(), "solo", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nsolo = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	assert!(
		git_tags(dir.path()).is_empty(),
		"Release should not create tags regardless of tag_format, got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_git_multi_package_creates_single_commit() {
	// When multiple packages are released simultaneously, a single commit is created.
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);
	write_changeset(
		dir.path(),
		"change.md",
		"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nFix and feature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	let log = git_log(dir.path());
	assert!(
		log[0].contains("pkg-a") && log[0].contains("pkg-b"),
		"Release commit should mention both packages, got: {}",
		log[0]
	);
	// No tags at release time
	assert!(
		git_tags(dir.path()).is_empty(),
		"Release should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_no_git_flag_skips_git() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "release", "--no-git"],
		dir.path(),
	);
	assert!(result.is_ok());

	let log = git_log(dir.path());
	assert!(
		!log.iter().any(|msg| msg.contains("chore(release):")),
		"--no-git should skip git operations, got log: {log:?}"
	);
	assert!(
		git_tags(dir.path()).is_empty(),
		"--no-git should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_git_stages_only_chronicle_files() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	// Create an unrelated unstaged file in the working tree
	std::fs::write(dir.path().join("unrelated.txt"), "do not commit me").unwrap();

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	// The release commit should not contain the unrelated file
	let output = Command::new("git")
		.args(["show", "--name-only", "--format=", "HEAD"])
		.current_dir(dir.path())
		.output()
		.unwrap();
	let changed_files = String::from_utf8(output.stdout).unwrap();
	assert!(
		!changed_files.contains("unrelated.txt"),
		"Unrelated file should not be in the release commit, got: {changed_files}"
	);
}

#[test]
fn release_git_filesystem_changes_persist_after_lifecycle() {
	// Version bumps (filesystem) happen before git ops; a successful git lifecycle
	// should leave the bumped version in place.
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("1.0.1"),
		"Version should be bumped to 1.0.1, got: {cargo_toml}"
	);
}

#[test]
fn release_dry_run_with_git_enabled_does_not_create_commit_or_tags() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(
		["chronicle", "--no-interactive", "release", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok());

	let log = git_log(dir.path());
	assert!(
		!log.iter().any(|msg| msg.contains("chore(release):")),
		"Dry run should not create a commit, got log: {log:?}"
	);
	assert!(
		git_tags(dir.path()).is_empty(),
		"Dry run should not create tags, got: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_git_extra_files_are_staged() {
	// An extra file listed in [git].extra_files should be included in the release
	// commit even though Chronicle didn't write it directly.
	let config = GitConfig {
		enabled: Some(true),
		extra_files: vec!["custom.lock".to_string()],
		..Default::default()
	};
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, config);
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	// Create the extra file and commit it so it's tracked
	std::fs::write(dir.path().join("custom.lock"), "initial").unwrap();
	git_commit_all(dir.path(), "chore: add custom.lock");

	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	// Simulate the custom lock command modifying the file before chronicle runs
	std::fs::write(dir.path().join("custom.lock"), "updated").unwrap();

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	// Verify custom.lock was included in the release commit
	let output = Command::new("git")
		.args(["show", "--name-only", "--format=", "HEAD"])
		.current_dir(dir.path())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.output()
		.unwrap();
	let changed_files = String::from_utf8(output.stdout).unwrap();
	assert!(
		changed_files.contains("custom.lock"),
		"custom.lock should be in the release commit, got: {changed_files}"
	);
}

#[test]
fn release_git_config_old_run_until_field_fails_to_load() {
	// Old configs with run_until must produce a clear parse error.
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let config_dir = dir.path().join(".chronicle");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[git]\nenabled = true\nrun_until = \"push\"\n",
	)
	.unwrap();
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-pkg\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_err(), "Expected error for old run_until field");
}
