//! Integration tests for the `prepare` command.

mod common;

use std::process::ExitCode;

use common::{
	temp_git_repo, temp_git_repo_with_cargo_workspace, temp_git_repo_with_project, write_changeset,
};
use cursus::model::config::PackageManager;
use cursus::test_logging::{init_test_logger, take_logs};

#[tokio::test]
async fn prepare_fails_when_no_config() {
	let dir = temp_git_repo();
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("No configuration found"),
		"Expected 'No configuration found' error, got: {err}"
	);
}

#[tokio::test]
async fn prepare_with_no_changesets_is_noop() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info
				&& m.contains("No pending changesets found")),
		"Expected info 'No pending changesets found' log, got: {logs:?}"
	);
}

#[tokio::test]
async fn prepare_with_single_changeset_cargo() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nAdded a feature\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let logs = take_logs();
	assert!(
		logs.iter().any(|(level, m)| *level == log::Level::Info
			&& m.contains("test-project: 0.1.0 -> 0.2.0 (minor)")),
		"Expected info version bump summary log, got: {logs:?}"
	);

	// Verify version was bumped
	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("version = \"0.2.0\""),
		"Expected version 0.2.0, got: {cargo_toml}"
	);

	// Verify changeset was deleted
	assert!(
		!dir.path().join(".cursus/test-change.md").exists(),
		"Changeset file should be deleted"
	);

	// Verify changelog was created
	let today = cursus::utils::today_iso_date();
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
	assert!(
		changelog.contains(&format!("## 0.2.0 - {today}")),
		"Changelog should contain version header with date, got: {changelog}"
	);
	assert!(
		changelog.contains("Added a feature"),
		"Changelog should contain the message, got: {changelog}"
	);
}

#[tokio::test]
async fn prepare_with_single_changeset_npm() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"patch\"\n+++\n\nFixed a bug\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let logs = take_logs();
	assert!(
		logs.iter().any(|(level, m)| *level == log::Level::Info
			&& m.contains("test-project: 0.1.0 -> 0.1.1 (patch)")),
		"Expected info version bump summary log, got: {logs:?}"
	);

	// Verify version was bumped
	let pkg_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		pkg_json.contains("\"0.1.1\""),
		"Expected version 0.1.1, got: {pkg_json}"
	);

	// Verify changeset was deleted
	assert!(!dir.path().join(".cursus/test-change.md").exists());
}

#[tokio::test]
async fn prepare_aggregates_to_highest_change_type() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
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

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// Minor wins over patch
	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("version = \"0.2.0\""),
		"Expected version 0.2.0 (minor bump), got: {cargo_toml}"
	);

	// Both changesets should be deleted
	assert!(!dir.path().join(".cursus/change-1.md").exists());
	assert!(!dir.path().join(".cursus/change-2.md").exists());
}

#[tokio::test]
async fn prepare_dry_run_does_not_modify_files() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"major\"\n+++\n\nBreaking change\n",
	);

	let original_cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();

	let result = common::run_cursus(
		["cursus", "--no-interactive", "prepare", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Info && m.contains("Would consume changeset")),
		"Expected info 'Would consume changeset' log in dry-run, got: {logs:?}"
	);

	// Cargo.toml should not be modified
	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert_eq!(
		cargo_toml, original_cargo,
		"Cargo.toml should not change in dry-run"
	);

	// Changeset should still exist
	assert!(
		dir.path().join(".cursus/test-change.md").exists(),
		"Changeset should not be deleted in dry-run"
	);

	// Changelog should not be created
	assert!(
		!dir.path().join("CHANGELOG.md").exists(),
		"Changelog should not be created in dry-run"
	);
}

#[tokio::test]
async fn prepare_major_bump_resets_minor_and_patch() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
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

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());

	let cargo_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		cargo_toml.contains("version = \"2.0.0\""),
		"Expected version 2.0.0, got: {cargo_toml}"
	);
}

#[tokio::test]
async fn prepare_idempotent_no_changesets_after_release() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nFeature\n",
	);

	// First release
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());

	// Second release (no changesets)
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);
}

#[tokio::test]
async fn prepare_changelog_has_proper_sections() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
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

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());

	let today = cursus::utils::today_iso_date();
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();

	// Major bump aggregates all, version should be 1.0.0
	assert!(
		changelog.contains(&format!("## 1.0.0 - {today}")),
		"Should have major bumped version with date, got: {changelog}"
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

#[tokio::test]
async fn prepare_successive_releases_prepend_to_changelog() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;

	// First release: minor bump 0.1.0 -> 0.2.0
	write_changeset(
		dir.path(),
		"change-1.md",
		"+++\ntest-project = \"minor\"\n+++\n\nFirst feature\n",
	);
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());

	// Second release: patch bump 0.2.0 -> 0.2.1
	write_changeset(
		dir.path(),
		"change-2.md",
		"+++\ntest-project = \"patch\"\n+++\n\nA bug fix\n",
	);
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok());

	let today = cursus::utils::today_iso_date();
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();

	// Both versions should be present
	assert!(
		changelog.contains(&format!("## 0.2.1 - {today}")),
		"Should contain newer version with date, got: {changelog}"
	);
	assert!(
		changelog.contains(&format!("## 0.2.0 - {today}")),
		"Should contain older version with date, got: {changelog}"
	);

	// Newer entry must appear before older entry
	let pos_new = changelog.find(&format!("## 0.2.1 - {today}")).unwrap();
	let pos_old = changelog.find(&format!("## 0.2.0 - {today}")).unwrap();
	assert!(
		pos_new < pos_old,
		"Newer version should appear before older version in changelog, got: {changelog}"
	);
}

#[tokio::test]
async fn prepare_unknown_package_in_changeset_fails() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\nnonexistent-package = \"minor\"\n+++\n\nSome change\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("not found in projects"),
		"Expected 'not found in projects' error, got: {err}"
	);
}

#[tokio::test]
async fn prepare_package_flag_filters_packages() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "0.1.0"), ("pkg-b", "0.2.0")]).await;

	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n",
	);

	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--package",
			"pkg-a",
		],
		dir.path(),
	)
	.await;
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

	// Changeset should be partially consumed — rewritten with only pkg-b remaining
	assert!(
		dir.path().join(".cursus/test-change.md").exists(),
		"Changeset should still exist (partially consumed)"
	);
	let rewritten = std::fs::read_to_string(dir.path().join(".cursus/test-change.md")).unwrap();
	insta::assert_snapshot!(rewritten);
}

#[tokio::test]
async fn prepare_scoped_fully_consumed_changeset_is_deleted() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "0.1.0")]).await;

	// Changeset only references pkg-a
	write_changeset(
		dir.path(),
		"only-a.md",
		"+++\npkg-a = \"patch\"\n+++\n\nFix in pkg-a\n",
	);

	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--package",
			"pkg-a",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok());

	// Fully consumed changeset must be deleted
	assert!(
		!dir.path().join(".cursus/only-a.md").exists(),
		"Fully consumed changeset should be deleted"
	);
}

#[tokio::test]
async fn prepare_scoped_sequential_releases() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "0.1.0"), ("pkg-b", "0.2.0")]).await;

	// Single changeset covering both packages
	write_changeset(
		dir.path(),
		"shared.md",
		"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nShared change\n",
	);

	// First release: only pkg-a
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--package",
			"pkg-a",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "First release failed: {result:?}");

	// Changeset should be rewritten with only pkg-b
	assert!(
		dir.path().join(".cursus/shared.md").exists(),
		"Changeset should still exist after first release"
	);
	let intermediate = std::fs::read_to_string(dir.path().join(".cursus/shared.md")).unwrap();
	insta::assert_snapshot!(intermediate);

	let pkg_a_toml = std::fs::read_to_string(dir.path().join("pkg-a/Cargo.toml")).unwrap();
	assert!(
		pkg_a_toml.contains("version = \"0.1.1\""),
		"pkg-a should be at 0.1.1, got: {pkg_a_toml}"
	);

	// Second release: only pkg-b
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--package",
			"pkg-b",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Second release failed: {result:?}");

	// Changeset now fully consumed — should be deleted
	assert!(
		!dir.path().join(".cursus/shared.md").exists(),
		"Changeset should be deleted after second release"
	);
	let pkg_b_toml = std::fs::read_to_string(dir.path().join("pkg-b/Cargo.toml")).unwrap();
	assert!(
		pkg_b_toml.contains("version = \"0.3.0\""),
		"pkg-b should be at 0.3.0, got: {pkg_b_toml}"
	);
}

#[tokio::test]
async fn prepare_scoped_unrelated_changeset_untouched() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "0.1.0"), ("pkg-b", "0.2.0")]).await;

	// One changeset for pkg-a, one for pkg-b
	let pkg_b_content = "+++\npkg-b = \"minor\"\n+++\n\nPkg-b only change\n";
	write_changeset(
		dir.path(),
		"only-a.md",
		"+++\npkg-a = \"patch\"\n+++\n\nPkg-a only change\n",
	);
	write_changeset(dir.path(), "only-b.md", pkg_b_content);

	// Release only pkg-a
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--package",
			"pkg-a",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok());

	// pkg-a changeset deleted
	assert!(
		!dir.path().join(".cursus/only-a.md").exists(),
		"pkg-a changeset should be deleted"
	);

	// pkg-b changeset untouched
	assert!(
		dir.path().join(".cursus/only-b.md").exists(),
		"pkg-b changeset should be untouched"
	);
	let b_content = std::fs::read_to_string(dir.path().join(".cursus/only-b.md")).unwrap();
	assert_eq!(
		b_content, pkg_b_content,
		"pkg-b changeset should be unchanged"
	);
}

#[tokio::test]
async fn prepare_unknown_package_flag_fails() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"minor\"\n+++\n\nSome change\n",
	);

	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--package",
			"nonexistent",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("Unknown package: nonexistent"),
		"Expected 'Unknown package: nonexistent' error, got: {err}"
	);
}

#[tokio::test]
async fn prepare_updates_cargo_lock_file() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
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

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
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

// The following four tests each run cursus inside a minimal Nix dev shell that provides
// only the package manager under test. This exercises the auto-detection and
// version-branching logic in a realistic isolated environment.

#[cfg(feature = "nix-tests")]
#[tokio::test]
async fn prepare_updates_npm_lock_file() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	std::fs::write(
		dir.path().join("package-lock.json"),
		r#"{"name":"test-project","version":"0.1.0","lockfileVersion":3,"requires":true,"packages":{"":{"name":"test-project","version":"0.1.0"}}}"#,
	)
	.unwrap();
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"patch\"\n+++\n\nFixed a bug\n",
	);

	let (success, _stdout, stderr) =
		common::run_cursus_in_nix_shell("test-npm", &["--no-interactive", "prepare"], dir.path());
	assert!(success, "prepare failed:\n{stderr}");

	let pkg_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		pkg_json.contains("\"0.1.1\""),
		"Expected version 0.1.1, got: {pkg_json}"
	);
	assert!(
		dir.path().join("package-lock.json").exists(),
		"package-lock.json should still exist after prepare"
	);
}

#[cfg(feature = "nix-tests")]
#[tokio::test]
async fn prepare_updates_pnpm_lock_file() {
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	std::fs::write(
		dir.path().join("pnpm-lock.yaml"),
		"lockfileVersion: '9.0'\nsettings:\n  autoInstallPeers: true\n  excludeLinksFromLockfile: false\n",
	)
	.unwrap();
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"patch\"\n+++\n\nFixed a bug\n",
	);

	let (success, _stdout, stderr) =
		common::run_cursus_in_nix_shell("test-pnpm", &["--no-interactive", "prepare"], dir.path());
	assert!(success, "prepare failed:\n{stderr}");

	let pkg_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		pkg_json.contains("\"0.1.1\""),
		"Expected version 0.1.1, got: {pkg_json}"
	);
	assert!(
		dir.path().join("pnpm-lock.yaml").exists(),
		"pnpm-lock.yaml should still exist after prepare"
	);
}

#[cfg(feature = "nix-tests")]
#[tokio::test]
async fn prepare_updates_yarn_classic_lock_file() {
	// In test-yarn-classic, `yarn` is Yarn Classic (1.x). The implementation detects
	// this via `yarn --version` and uses `--ignore-scripts` instead of `--mode
	// update-lockfile` (which Classic silently ignores, leaving scripts running).
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	std::fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"patch\"\n+++\n\nFixed a bug\n",
	);

	let (success, _stdout, stderr) = common::run_cursus_in_nix_shell(
		"test-yarn-classic",
		&["--no-interactive", "prepare"],
		dir.path(),
	);
	assert!(success, "prepare failed:\n{stderr}");

	let pkg_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		pkg_json.contains("\"0.1.1\""),
		"Expected version 0.1.1, got: {pkg_json}"
	);
	assert!(
		dir.path().join("yarn.lock").exists(),
		"yarn.lock should still exist after prepare"
	);
}

#[cfg(feature = "nix-tests")]
#[tokio::test]
async fn prepare_updates_yarn_berry_lock_file() {
	// In test-yarn-berry, `yarn` is Yarn Berry (v4) installed directly from
	// pkgs.yarn-berry — no wrapper needed. The implementation detects the major
	// version and uses `--mode update-lockfile`, which skips scripts automatically.
	let dir = temp_git_repo_with_project(PackageManager::Npm).await;
	std::fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();
	write_changeset(
		dir.path(),
		"test-change.md",
		"+++\ntest-project = \"patch\"\n+++\n\nFixed a bug\n",
	);

	let (success, _stdout, stderr) = common::run_cursus_in_nix_shell(
		"test-yarn-berry",
		&["--no-interactive", "prepare"],
		dir.path(),
	);
	assert!(success, "prepare failed:\n{stderr}");

	let pkg_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
	assert!(
		pkg_json.contains("\"0.1.1\""),
		"Expected version 0.1.1, got: {pkg_json}"
	);
	assert!(
		dir.path().join("yarn.lock").exists(),
		"yarn.lock should still exist after prepare"
	);
}

#[tokio::test]
async fn prepare_updates_cargo_intra_workspace_dep_version() {
	// pkg-a depends on pkg-b via a path dep; when pkg-b is bumped, pkg-a's Cargo.toml should be updated
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "0.1.0"), ("pkg-b", "0.2.0")]).await;

	// Write pkg-a with a path + version dep on pkg-b (path lets Cargo resolve it locally)
	std::fs::write(
		dir.path().join("pkg-a/Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\npkg-b = { path = \"../pkg-b\", version = \"0.2.0\" }\n",
	)
	.unwrap();

	write_changeset(
		dir.path(),
		"bump-b.md",
		"+++\npkg-b = \"minor\"\n+++\n\nAdded feature to pkg-b\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "release failed: {:?}", result.unwrap_err());

	// Verify pkg-b version was bumped
	let pkg_b_toml = std::fs::read_to_string(dir.path().join("pkg-b/Cargo.toml")).unwrap();
	assert!(
		pkg_b_toml.contains("version = \"0.3.0\""),
		"Expected pkg-b version 0.3.0, got: {pkg_b_toml}"
	);

	// Verify pkg-a's dependency on pkg-b was updated (version field in table entry)
	let pkg_a_toml = std::fs::read_to_string(dir.path().join("pkg-a/Cargo.toml")).unwrap();
	assert!(
		pkg_a_toml.contains("version = \"0.3.0\""),
		"Expected pkg-a to reference pkg-b 0.3.0, got: {pkg_a_toml}"
	);
	assert!(
		pkg_a_toml.contains("path = \"../pkg-b\""),
		"Expected path dep to be preserved, got: {pkg_a_toml}"
	);
}

#[tokio::test]
async fn prepare_updates_cargo_workspace_dep_in_root() {
	// Root Cargo.toml has [workspace.dependencies]; pkg-a uses it via workspace = true.
	// When pkg-b is bumped, the root [workspace.dependencies] entry should be updated.
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "0.1.0"), ("pkg-b", "0.2.0")]).await;

	// Root Cargo.toml with workspace.dependencies (path dep so Cargo finds it locally)
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[workspace]\nmembers = [\"pkg-a\", \"pkg-b\"]\n\n[workspace.dependencies]\npkg-b = { path = \"pkg-b\", version = \"0.2.0\" }\n",
	)
	.unwrap();

	// pkg-a inherits pkg-b from workspace, which puts "pkg-b" in its dependency_names
	std::fs::write(
		dir.path().join("pkg-a/Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\npkg-b = { workspace = true }\n",
	)
	.unwrap();

	write_changeset(
		dir.path(),
		"bump-b.md",
		"+++\npkg-b = \"minor\"\n+++\n\nAdded feature\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "release failed: {:?}", result.unwrap_err());

	let root_toml = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
	assert!(
		root_toml.contains("version = \"0.3.0\""),
		"Expected root [workspace.dependencies] pkg-b version 0.3.0, got: {root_toml}"
	);
	assert!(
		root_toml.contains("path = \"pkg-b\""),
		"Expected path to be preserved in workspace.dependencies, got: {root_toml}"
	);
}

#[tokio::test]
async fn prepare_dry_run_shows_dep_updates_without_modifying_files() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "0.1.0"), ("pkg-b", "0.2.0")]).await;

	std::fs::write(
		dir.path().join("pkg-a/Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\npkg-b = { path = \"../pkg-b\", version = \"0.2.0\" }\n",
	)
	.unwrap();

	write_changeset(
		dir.path(),
		"bump-b.md",
		"+++\npkg-b = \"patch\"\n+++\n\nBug fix\n",
	);

	let original_a = std::fs::read_to_string(dir.path().join("pkg-a/Cargo.toml")).unwrap();

	let result = common::run_cursus(
		["cursus", "--no-interactive", "prepare", "--dry-run"],
		dir.path(),
	)
	.await;
	assert!(result.is_ok());

	// pkg-a/Cargo.toml should not be modified in dry-run
	let after_a = std::fs::read_to_string(dir.path().join("pkg-a/Cargo.toml")).unwrap();
	assert_eq!(original_a, after_a, "dry-run must not modify dep files");
}

/// `prepare` issues a warning when `--branch` is given with the push strategy.
#[tokio::test]
async fn prepare_branch_arg_with_push_strategy_warns() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	write_changeset(
		dir.path(),
		"change.md",
		"+++\ntest-project = \"patch\"\n+++\n\nFix\n",
	);

	// Push strategy is the default (no [git] section). Passing --branch should warn.
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--branch",
			"custom-branch",
		],
		dir.path(),
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Warn && m.contains("no effect")),
		"Expected a 'no effect' warning when --branch is given with push strategy, got: {logs:?}"
	);
}

/// `prepare` without `--branch` and push strategy must NOT issue the "no effect" warning.
///
/// This guards against mutations that weaken `&&` to `||` in the branch-warning predicate,
/// which would incorrectly warn whenever push strategy is active even without a `--branch` arg.
#[tokio::test]
async fn prepare_no_branch_arg_with_push_strategy_no_warning() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_project(PackageManager::Cargo).await;
	write_changeset(
		dir.path(),
		"change.md",
		"+++\ntest-project = \"patch\"\n+++\n\nFix\n",
	);

	// No --branch arg, push strategy (default) → warning must NOT appear.
	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let logs = take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("no effect") && m.contains("branch")),
		"Should not warn about --branch when it was not passed, got: {logs:?}"
	);
}

#[tokio::test]
async fn prepare_updates_npm_intra_workspace_dep_version() {
	// pkg-a depends on pkg-b; when pkg-b is bumped, pkg-a's package.json should be updated
	let dir = temp_git_repo();
	let config = cursus::model::config::Config::new(&common::test_env(dir.path()))
		.with_npm(cursus::model::config::NpmConfig::enabled());
	config.save().await.unwrap();

	// Root package.json with workspace config
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "0.0.0", "private": true, "workspaces": ["pkg-a", "pkg-b"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("pkg-a")).unwrap();
	std::fs::write(
		dir.path().join("pkg-a/package.json"),
		r#"{"name": "pkg-a", "version": "0.1.0", "dependencies": {"pkg-b": "^0.2.0"}}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("pkg-b")).unwrap();
	std::fs::write(
		dir.path().join("pkg-b/package.json"),
		r#"{"name": "pkg-b", "version": "0.2.0"}"#,
	)
	.unwrap();

	write_changeset(
		dir.path(),
		"bump-b.md",
		"+++\npkg-b = \"minor\"\n+++\n\nAdded feature to pkg-b\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "release failed: {:?}", result.unwrap_err());

	// Verify pkg-b version was bumped
	let pkg_b_json = std::fs::read_to_string(dir.path().join("pkg-b/package.json")).unwrap();
	assert!(
		pkg_b_json.contains("\"0.3.0\""),
		"Expected pkg-b version 0.3.0, got: {pkg_b_json}"
	);

	// Verify pkg-a's dependency on pkg-b was updated (preserving ^ prefix)
	let pkg_a_json = std::fs::read_to_string(dir.path().join("pkg-a/package.json")).unwrap();
	assert!(
		pkg_a_json.contains("\"pkg-b\": \"^0.3.0\""),
		"Expected pkg-a to reference pkg-b ^0.3.0, got: {pkg_a_json}"
	);
}

#[tokio::test]
async fn prepare_skips_workspace_protocol_dep_version() {
	// pkg-a depends on pkg-b via workspace: protocol; that dep must NOT be rewritten
	// (ADR-012: workspace: entries auto-resolve at publish time via the package manager)
	let dir = temp_git_repo();
	let config = cursus::model::config::Config::new(&common::test_env(dir.path()))
		.with_npm(cursus::model::config::NpmConfig::enabled());
	config.save().await.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "0.0.0", "private": true, "workspaces": ["pkg-a", "pkg-b"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("pkg-a")).unwrap();
	std::fs::write(
		dir.path().join("pkg-a/package.json"),
		r#"{"name": "pkg-a", "version": "0.1.0", "dependencies": {"pkg-b": "workspace:*"}}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("pkg-b")).unwrap();
	std::fs::write(
		dir.path().join("pkg-b/package.json"),
		r#"{"name": "pkg-b", "version": "0.2.0"}"#,
	)
	.unwrap();

	write_changeset(
		dir.path(),
		"bump-b.md",
		"+++\npkg-b = \"minor\"\n+++\n\nAdded feature to pkg-b\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path()).await;
	assert!(result.is_ok(), "prepare failed: {:?}", result.unwrap_err());

	// pkg-b version should be bumped normally
	let pkg_b_json = std::fs::read_to_string(dir.path().join("pkg-b/package.json")).unwrap();
	assert!(
		pkg_b_json.contains("\"0.3.0\""),
		"Expected pkg-b version 0.3.0, got: {pkg_b_json}"
	);

	// pkg-a's workspace: dep must remain untouched
	let pkg_a_json = std::fs::read_to_string(dir.path().join("pkg-a/package.json")).unwrap();
	assert!(
		pkg_a_json.contains("\"pkg-b\": \"workspace:*\""),
		"Expected workspace: dep to be preserved unchanged, got: {pkg_a_json}"
	);
}
