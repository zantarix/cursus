//! Integration tests for git lifecycle automation in the `release` command.

mod common;

use std::process::{Command, Stdio};

use chronicle::git::{GitConfig, GitStep, TagFormat};
use chronicle::model::config::PackageManager;
use common::{
	add_local_remote, git_log, git_tag_exists, git_tags, temp_git_repo_with_project,
	temp_real_git_repo_with_cargo_workspace, temp_real_git_repo_with_config,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Creates a git-enabled config with default settings (Tag step, Auto format).
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
fn release_git_creates_tags() {
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

	// Single package with Auto format → v{version}
	assert!(
		git_tag_exists(dir.path(), "v1.0.1"),
		"Expected tag v1.0.1, got tags: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_git_tag_format_auto_single() {
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, git_enabled_config());
	setup_single_cargo_package(dir.path(), "solo", "2.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nsolo = \"minor\"\n+++\n\nFeature\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	assert!(
		git_tag_exists(dir.path(), "v2.1.0"),
		"Single-package auto format should use v{{version}}, tags: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_git_tag_format_auto_multi() {
	let dir = temp_real_git_repo_with_cargo_workspace(
		&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")],
		git_enabled_config(),
	);
	write_changeset(
		dir.path(),
		"change.md",
		"+++\npkg-a = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	// Multi-package workspace with Auto → prefixed
	assert!(
		git_tag_exists(dir.path(), "pkg-a@1.0.1"),
		"Multi-package auto format should use pkg@version, tags: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_git_tag_format_prefixed() {
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
		git_tag_exists(dir.path(), "solo@1.0.1"),
		"Prefixed format should use pkg@version even for single package, tags: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_git_tag_format_simple() {
	let config = GitConfig {
		enabled: Some(true),
		tag_format: TagFormat::Simple,
		..Default::default()
	};
	let dir =
		temp_real_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "2.0.0")], config);
	write_changeset(
		dir.path(),
		"change.md",
		"+++\npkg-a = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	// Simple format: always v{version} even in a monorepo
	assert!(
		git_tag_exists(dir.path(), "v1.0.1"),
		"Simple format should use v{{version}} even in multi-package, tags: {:?}",
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
fn release_git_run_until_commit() {
	let config = GitConfig {
		enabled: Some(true),
		run_until: GitStep::Commit,
		..Default::default()
	};
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, config);
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok());

	let log = git_log(dir.path());
	assert!(
		log.iter().any(|msg| msg.contains("chore(release):")),
		"Commit should be created, got log: {log:?}"
	);
	assert!(
		git_tags(dir.path()).is_empty(),
		"run_until=commit should not create tags, got: {:?}",
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
fn release_git_run_until_push() {
	let config = GitConfig {
		enabled: Some(true),
		run_until: GitStep::Push,
		..Default::default()
	};
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, config);
	// Wire up a local bare repo as "origin" so push has a real remote to push to
	let _remote = add_local_remote(dir.path());

	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	// Push initial state to remote so follow-tags push has a tracking branch
	let output = Command::new("git")
		.args(["push", "-u", "origin", "HEAD"])
		.current_dir(dir.path())
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"initial push failed:\n{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	assert!(result.is_ok(), "release failed: {result:?}");

	// Commit and tags should exist locally
	let log = git_log(dir.path());
	assert!(
		log.iter().any(|msg| msg.contains("chore(release):")),
		"Expected release commit, got: {log:?}"
	);
	assert!(
		git_tag_exists(dir.path(), "v1.0.1"),
		"Expected tag v1.0.1, got tags: {:?}",
		git_tags(dir.path())
	);
}

#[test]
fn release_git_failure_preserves_filesystem_changes() {
	// When git push fails (no remote configured), version bumps and
	// changelogs written before git ran must remain on disk.
	let config = GitConfig {
		enabled: Some(true),
		run_until: GitStep::Push,
		..Default::default()
	};
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, config);
	// No remote added → git push will fail
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nFix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");

	let result = common::run_chronicle(["chronicle", "--no-interactive", "release"], dir.path());
	// Push fails because there is no remote — the overall release should error
	assert!(result.is_err(), "Expected error due to missing remote");

	// Despite the git failure, the version bump must still be on disk
	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("1.0.1"),
		"Version should be bumped to 1.0.1 even after git failure, got: {cargo_toml}"
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
fn release_git_multi_package_creates_all_tags() {
	// When multiple packages are released simultaneously, a tag must be created
	// for each one — not just the first.
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

	let tags = git_tags(dir.path());
	// Multi-package workspace with Auto format → pkg@version
	assert!(
		tags.contains(&"pkg-a@1.0.1".to_string()),
		"Expected tag pkg-a@1.0.1, got: {tags:?}"
	);
	assert!(
		tags.contains(&"pkg-b@2.1.0".to_string()),
		"Expected tag pkg-b@2.1.0, got: {tags:?}"
	);
}
