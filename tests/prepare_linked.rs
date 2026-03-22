//! Integration tests for linked-version behaviour in the `prepare` command.

mod common;

use std::process::ExitCode;

use common::{temp_git_repo_with_cargo_workspace, write_changeset};
use cursus::filesystem::LocalFilesystem;
use cursus::model::config::{LinkedVersionGroup, LinkedVersionsConfig};
use cursus::path::AbsolutePath;
use cursus::test_logging::{init_test_logger, take_logs};

/// Adds a `[linked-versions]` block to the Cursus config saved in `dir`.
fn add_linked_versions_to_config(dir: &std::path::Path, lv: LinkedVersionsConfig) {
	let abs = AbsolutePath::new(dir).unwrap();
	let mut config = cursus::model::config::load(&abs, &make_env()).unwrap();
	config.linked_versions = lv;
	config.save().unwrap();
}

fn make_env() -> cursus::Env {
	cursus::Env::new(
		std::sync::Arc::new(cursus::command::RealCommandRunner)
			as std::sync::Arc<dyn cursus::command::CommandRunner>,
		std::sync::Arc::new(cursus::filesystem::LocalFilesystem),
	)
}

fn read_pkg_version(dir: &std::path::Path, pkg: &str) -> String {
	let cargo_toml = std::fs::read_to_string(dir.join(format!("{pkg}/Cargo.toml"))).unwrap();
	for line in cargo_toml.lines() {
		if let Some(v) = line.strip_prefix("version = \"") {
			return v.trim_end_matches('"').to_string();
		}
	}
	panic!("version not found in {pkg}/Cargo.toml");
}

fn global_config() -> LinkedVersionsConfig {
	LinkedVersionsConfig {
		enabled: Some(true),
		groups: vec![],
	}
}

fn group_config(packages: Vec<Vec<&str>>) -> LinkedVersionsConfig {
	LinkedVersionsConfig {
		enabled: None,
		groups: packages
			.into_iter()
			.map(|pkgs| LinkedVersionGroup {
				packages: pkgs.into_iter().map(str::to_string).collect(),
			})
			.collect(),
	}
}

// ── Global linking ─────────────────────────────────────────────────────────

#[test]
fn global_linking_bumps_all_to_max() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_cargo_workspace(&[
		("pkg-a", "1.0.0"),
		("pkg-b", "1.0.0"),
		("pkg-c", "1.0.0"),
	]);
	// Only pkg-a has a changeset (minor bump → 1.1.0).
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"minor\"\n+++\n\nA feature\n",
	);
	add_linked_versions_to_config(dir.path(), global_config());

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// All packages should be at 1.1.0 (pulled up by global linking).
	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "1.1.0");
	assert_eq!(read_pkg_version(dir.path(), "pkg-b"), "1.1.0");
	assert_eq!(read_pkg_version(dir.path(), "pkg-c"), "1.1.0");
}

#[test]
fn global_linking_with_package_filter_errors() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "1.0.0")]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"patch\"\n+++\n\nA fix\n",
	);
	add_linked_versions_to_config(dir.path(), global_config());

	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--package",
			"pkg-a",
		],
		dir.path(),
	);
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("global linked-versions")
	);
}

// ── Group-based linking ─────────────────────────────────────────────────────

#[test]
fn group_linking_bumps_only_group_members() {
	let dir = temp_git_repo_with_cargo_workspace(&[
		("pkg-a", "1.0.0"),
		("pkg-b", "1.0.0"),
		("standalone", "2.0.0"),
	]);
	// pkg-a gets a minor changeset; pkg-b and standalone have none.
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"minor\"\n+++\n\nA feature\n",
	);
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["pkg-a", "pkg-b"]]));

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());

	// pkg-a and pkg-b are linked, so both go to 1.1.0.
	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "1.1.0");
	assert_eq!(read_pkg_version(dir.path(), "pkg-b"), "1.1.0");
	// standalone is not in the group; it has no changeset, so it stays at 2.0.0.
	assert_eq!(read_pkg_version(dir.path(), "standalone"), "2.0.0");
}

#[test]
fn max_version_wins_with_diverged_versions() {
	let dir = temp_git_repo_with_cargo_workspace(&[
		("pkg-a", "2.1.0"),
		("pkg-b", "2.0.0"), // already diverged
	]);
	// A patch changeset on pkg-a bumps it to 2.1.1.
	// Group max among (2.1.1, 2.0.0) = 2.1.1 → pkg-b gets pulled to 2.1.1.
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"patch\"\n+++\n\nA fix\n",
	);
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["pkg-a", "pkg-b"]]));

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());

	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "2.1.1");
	assert_eq!(read_pkg_version(dir.path(), "pkg-b"), "2.1.1");
}

/// Regression: changeset on the lower-versioned package must not be swallowed.
///
/// A@2.3.4 (no changeset) + B@1.2.3 (patch changeset) → both at 2.3.5.
/// The algorithm must apply B's patch bump *to the group max current version*
/// (2.3.4), not to B's own current version (which would give 1.2.4).
#[test]
fn changeset_on_lower_version_package_advances_whole_group() {
	let dir = temp_git_repo_with_cargo_workspace(&[
		("pkg-a", "2.3.4"), // higher current version, no changeset
		("pkg-b", "1.2.3"), // lower current version, has a patch changeset
	]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-b = \"patch\"\n+++\n\nA fix\n",
	);
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["pkg-a", "pkg-b"]]));

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	// Both packages must end up at 2.3.5 (max current 2.3.4 + patch).
	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "2.3.5");
	assert_eq!(read_pkg_version(dir.path(), "pkg-b"), "2.3.5");
}

#[test]
fn scoped_prepare_partial_group_overlap_errors() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "1.0.0")]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"patch\"\n+++\n\nA fix\n",
	);
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["pkg-a", "pkg-b"]]));

	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--package",
			"pkg-a",
		],
		dir.path(),
	);
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("partially overlaps"),
		"Expected 'partially overlaps' error, got: {msg}"
	);
	assert!(
		msg.contains("pkg-b"),
		"Expected missing pkg-b listed in error"
	);
}

#[test]
fn scoped_prepare_full_group_in_scope_succeeds() {
	let dir = temp_git_repo_with_cargo_workspace(&[
		("pkg-a", "1.0.0"),
		("pkg-b", "1.0.0"),
		("standalone", "1.0.0"),
	]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"patch\"\n+++\n\nA fix\n",
	);
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["pkg-a", "pkg-b"]]));

	// Including both pkg-a and pkg-b (the full group) is valid.
	let result = common::run_cursus(
		[
			"cursus",
			"--no-interactive",
			"prepare",
			"--package",
			"pkg-a",
			"--package",
			"pkg-b",
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected success: {result:?}");
	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "1.0.1");
	assert_eq!(read_pkg_version(dir.path(), "pkg-b"), "1.0.1");
	// standalone is excluded from scope, so it stays.
	assert_eq!(read_pkg_version(dir.path(), "standalone"), "1.0.0");
}

#[test]
fn linked_packages_get_sync_changelog_entry() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "1.0.0")]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"minor\"\n+++\n\nFeature for A\n",
	);
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["pkg-a", "pkg-b"]]));

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());

	let changelog_b = std::fs::read_to_string(dir.path().join("pkg-b/CHANGELOG.md")).unwrap();
	assert!(
		changelog_b.contains("version sync"),
		"pkg-b changelog should mention version sync, got: {changelog_b}"
	);
	assert!(
		changelog_b.contains("1.1.0"),
		"pkg-b changelog should contain the new version, got: {changelog_b}"
	);
}

#[test]
fn disabled_linking_does_not_sync() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "1.0.0")]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"minor\"\n+++\n\nA feature\n",
	);
	// enabled = false disables linking even though groups are non-empty.
	add_linked_versions_to_config(
		dir.path(),
		LinkedVersionsConfig {
			enabled: Some(false),
			groups: vec![LinkedVersionGroup {
				packages: vec!["pkg-a".to_string(), "pkg-b".to_string()],
			}],
		},
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());

	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "1.1.0");
	// pkg-b is not bumped because linking is disabled.
	assert_eq!(read_pkg_version(dir.path(), "pkg-b"), "1.0.0");
}

#[test]
fn empty_packages_array_errors() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0")]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"patch\"\n+++\n\nA fix\n",
	);
	add_linked_versions_to_config(
		dir.path(),
		LinkedVersionsConfig {
			enabled: None,
			groups: vec![LinkedVersionGroup { packages: vec![] }],
		},
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("empty 'packages' array")
	);
}

#[test]
fn package_in_multiple_groups_errors() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "1.0.0")]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"patch\"\n+++\n\nA fix\n",
	);
	// pkg-a matches both groups (by name in group 1 and wildcard in group 2).
	add_linked_versions_to_config(
		dir.path(),
		group_config(vec![vec!["pkg-a", "pkg-b"], vec!["pkg-*"]]),
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("multiple linked-versions groups")
	);
}

#[test]
fn pattern_matching_no_packages_warns_but_succeeds() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0")]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"patch\"\n+++\n\nA fix\n",
	);
	// The pattern "nonexistent-*" matches nothing — should warn, not fail.
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["nonexistent-*"]]));

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(level, m)| *level == log::Level::Warn && m.contains("matches no packages")),
		"Expected warning about pattern matching no packages, got: {logs:?}"
	);
	// pkg-a is not in any group (the only group had no matches), so it gets bumped normally.
	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "1.0.1");
}

#[test]
fn dry_run_with_linked_versions_does_not_write() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "1.0.0")]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"minor\"\n+++\n\nA feature\n",
	);
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["pkg-a", "pkg-b"]]));

	let result = common::run_cursus(
		["cursus", "--no-interactive", "--dry-run", "prepare"],
		dir.path(),
	);
	assert!(result.is_ok());

	// No versions should be changed in dry-run mode.
	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "1.0.0");
	assert_eq!(read_pkg_version(dir.path(), "pkg-b"), "1.0.0");
}

/// Highest change type across group members wins.
///
/// pkg-a has a minor changeset and pkg-b has a major changeset; the whole group
/// should advance by a major bump applied to the max current version.
#[test]
fn highest_change_type_across_group_members_wins() {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "1.0.0")]);
	// Two changesets: one minor, one major.
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\npkg-a = \"minor\"\n+++\n\nA feature\n",
	);
	write_changeset(
		dir.path(),
		"cs2.md",
		"+++\npkg-b = \"major\"\n+++\n\nA breaking change\n",
	);
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["pkg-a", "pkg-b"]]));

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	// max current = 1.0.0; highest_ct = Major → final = 2.0.0
	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "2.0.0");
	assert_eq!(read_pkg_version(dir.path(), "pkg-b"), "2.0.0");
}

#[test]
fn glob_pattern_matches_prefix() {
	let dir = temp_git_repo_with_cargo_workspace(&[
		("sdk-core", "1.0.0"),
		("sdk-utils", "1.0.0"),
		("other", "1.0.0"),
	]);
	write_changeset(
		dir.path(),
		"cs1.md",
		"+++\nsdk-core = \"minor\"\n+++\n\nCore update\n",
	);
	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["sdk-*"]]));

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());

	assert_eq!(read_pkg_version(dir.path(), "sdk-core"), "1.1.0");
	assert_eq!(read_pkg_version(dir.path(), "sdk-utils"), "1.1.0");
	assert_eq!(read_pkg_version(dir.path(), "other"), "1.0.0");
}

// ── Linked group + dependency propagation interaction ───────────────────────

#[test]
fn propagation_into_linked_group_bumps_all_group_members() {
	// Scenario: linked group [pkg-a@0.2.5, pkg-b@0.2.5], pkg-c@1.2.3.
	// pkg-a depends on pkg-c. Changeset bumps only pkg-c (minor).
	// Expected: pkg-c → 1.3.0, pkg-a → 0.2.6 (propagated patch), pkg-b → 0.2.6 (linked with pkg-a).
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo_with_cargo_workspace(&[
		("pkg-a", "0.2.5"),
		("pkg-b", "0.2.5"),
		("pkg-c", "1.2.3"),
	]);
	// pkg-a depends on pkg-c
	std::fs::write(
		dir.path().join("pkg-a/Cargo.toml"),
		"[package]\nname = \"pkg-a\"\nversion = \"0.2.5\"\nedition = \"2024\"\n\n[dependencies]\npkg-c = { path = \"../pkg-c\", version = \"1.2.3\" }\n",
	)
	.unwrap();

	add_linked_versions_to_config(dir.path(), group_config(vec![vec!["pkg-a", "pkg-b"]]));

	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-c = \"minor\"\n+++\n\nNew feature in pkg-c\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "prepare failed: {result:?}");
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_pkg_version(dir.path(), "pkg-c"), "1.3.0");
	assert_eq!(read_pkg_version(dir.path(), "pkg-a"), "0.2.6");
	// pkg-b is in the same linked group as pkg-a and must be co-bumped.
	assert_eq!(read_pkg_version(dir.path(), "pkg-b"), "0.2.6");
}
