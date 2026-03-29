//! Nix dev shell integration tests for the `prepare` command's lock file updates.
//!
//! Each test runs cursus inside a minimal Nix dev shell that provides only the
//! package manager under test. This exercises the auto-detection and
//! version-branching logic in a realistic isolated environment.

mod common;

use common::{temp_git_repo_with_project, write_changeset};
use cursus::model::config::PackageManager;

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
