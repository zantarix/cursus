//! Integration tests for dependency propagation bumps (ADR-023).

mod common;

use std::process::ExitCode;

use common::{temp_git_repo, temp_git_repo_with_cargo_workspace, write_changeset};
use cursus::filesystem::LocalFilesystem;
use cursus::model::config::Config;
use cursus::model::config::{DependencyBump, NpmConfig, PrepareConfig};
use cursus::path::AbsolutePath;
use cursus::test_logging::{init_test_logger, take_logs};

/// Reads the `version` field from a member's Cargo.toml.
fn read_version(dir: &std::path::Path, pkg: &str) -> String {
	let cargo_toml =
		std::fs::read_to_string(dir.join(format!("{pkg}/Cargo.toml"))).unwrap_or_else(|_| {
			std::fs::read_to_string(dir.join("Cargo.toml")).expect("Could not read Cargo.toml")
		});
	let parsed: toml::Value = toml::from_str(&cargo_toml).expect("invalid Cargo.toml");
	parsed["package"]["version"]
		.as_str()
		.unwrap_or_else(|| panic!("version not found for package {pkg}"))
		.to_string()
}

fn make_env() -> cursus::Env {
	cursus::Env::new(
		std::sync::Arc::new(cursus::command::RealCommandRunner)
			as std::sync::Arc<dyn cursus::command::CommandRunner>,
		std::sync::Arc::new(cursus::filesystem::LocalFilesystem),
	)
}

/// Saves a `[prepare]` config section to an existing cursus config.
fn set_prepare_config(dir: &std::path::Path, prepare: PrepareConfig) {
	let abs = AbsolutePath::new(dir).unwrap();
	let mut config = cursus::model::config::load(&abs, &make_env()).unwrap();
	config.prepare = prepare;
	config.with_env(common::test_env()).save().unwrap();
}

/// Creates a Cargo workspace with pkg-a at 1.0.0 and pkg-b depending on pkg-a.
///
/// Returns the temp dir. pkg-b's Cargo.toml declares a dependency on pkg-a.
fn workspace_with_dependency() -> tempfile::TempDir {
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "1.0.0")]);
	// Add a [dependencies] section to pkg-b that references pkg-a.
	std::fs::write(
		dir.path().join("pkg-b/Cargo.toml"),
		"[package]\nname = \"pkg-b\"\nversion = \"1.0.0\"\nedition = \"2024\"\n\n[dependencies]\npkg-a = { path = \"../pkg-a\", version = \"1.0.0\" }\n",
	)
	.unwrap();
	dir
}

/// Creates a workspace with three packages in a chain: C depends on B, B depends on A.
fn workspace_with_chain() -> tempfile::TempDir {
	let dir = temp_git_repo_with_cargo_workspace(&[
		("pkg-a", "1.0.0"),
		("pkg-b", "1.0.0"),
		("pkg-c", "1.0.0"),
	]);
	std::fs::write(
		dir.path().join("pkg-b/Cargo.toml"),
		"[package]\nname = \"pkg-b\"\nversion = \"1.0.0\"\nedition = \"2024\"\n\n[dependencies]\npkg-a = { path = \"../pkg-a\", version = \"1.0.0\" }\n",
	)
	.unwrap();
	std::fs::write(
		dir.path().join("pkg-c/Cargo.toml"),
		"[package]\nname = \"pkg-c\"\nversion = \"1.0.0\"\nedition = \"2024\"\n\n[dependencies]\npkg-b = { path = \"../pkg-b\", version = \"1.0.0\" }\n",
	)
	.unwrap();
	dir
}

// ── Basic propagation ──────────────────────────────────────────────────────

#[test]
fn propagation_auto_minor_bump_produces_patch_in_dependent() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	// pkg-a gets a minor bump; under "auto", pkg-b should receive a patch bump.
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"minor\"\n+++\n\nNew feature in pkg-a\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "1.1.0");
	// Under "auto", minor upstream → patch propagation.
	assert_eq!(read_version(dir.path(), "pkg-b"), "1.0.1");
}

#[test]
fn propagation_auto_major_bump_produces_major_in_dependent() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	// pkg-a gets a major bump; under "auto", pkg-b should also receive a major bump.
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"major\"\n+++\n\nBreaking change in pkg-a\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "2.0.0");
	// Under "auto", major upstream → major propagation.
	assert_eq!(read_version(dir.path(), "pkg-b"), "2.0.0");
}

#[test]
fn propagation_patch_mode_always_patches_dependent() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	set_prepare_config(
		dir.path(),
		PrepareConfig {
			dependency_bump: DependencyBump::Patch,
		},
	);
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"major\"\n+++\n\nBreaking\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "2.0.0");
	// "patch" mode: major upstream still only causes patch in dependent.
	assert_eq!(read_version(dir.path(), "pkg-b"), "1.0.1");
}

#[test]
fn propagation_match_mode_mirrors_upstream_level() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	set_prepare_config(
		dir.path(),
		PrepareConfig {
			dependency_bump: DependencyBump::Match,
		},
	);
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"minor\"\n+++\n\nFeature\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "1.1.0");
	// "match" mode: minor upstream → minor in dependent.
	assert_eq!(read_version(dir.path(), "pkg-b"), "1.1.0");
}

#[test]
fn propagation_minor_mode_always_minors_dependent() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	set_prepare_config(
		dir.path(),
		PrepareConfig {
			dependency_bump: DependencyBump::Minor,
		},
	);
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"patch\"\n+++\n\nBug fix\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "1.0.1");
	// "minor" mode: patch upstream → minor in dependent.
	assert_eq!(read_version(dir.path(), "pkg-b"), "1.1.0");
}

#[test]
fn propagation_major_mode_always_majors_dependent() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	set_prepare_config(
		dir.path(),
		PrepareConfig {
			dependency_bump: DependencyBump::Major,
		},
	);
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"patch\"\n+++\n\nBug fix\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "1.0.1");
	// "major" mode: patch upstream → major in dependent.
	assert_eq!(read_version(dir.path(), "pkg-b"), "2.0.0");
}

// ── Transitive propagation ─────────────────────────────────────────────────

#[test]
fn propagation_transitive_chain_all_bumped() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_chain();
	// A gets a minor bump. Under "auto": B gets patch, C gets patch (from B's patch).
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"minor\"\n+++\n\nFeature\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "1.1.0");
	assert_eq!(read_version(dir.path(), "pkg-b"), "1.0.1");
	assert_eq!(read_version(dir.path(), "pkg-c"), "1.0.1");
}

#[test]
fn propagation_transitive_major_propagates_through_chain() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_chain();
	// A gets a major bump. Under "auto": B gets major, C gets major (from B's major).
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"major\"\n+++\n\nBreaking\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "2.0.0");
	assert_eq!(read_version(dir.path(), "pkg-b"), "2.0.0");
	assert_eq!(read_version(dir.path(), "pkg-c"), "2.0.0");
}

// ── Existing changeset takes precedence ───────────────────────────────────

#[test]
fn propagation_does_not_downgrade_existing_higher_changeset() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	// pkg-a gets a patch bump (would propagate patch to pkg-b).
	// But pkg-b already has a major changeset — it should stay at major.
	write_changeset(
		dir.path(),
		"cs-a.md",
		"+++\npkg-a = \"patch\"\n+++\n\nBug fix\n",
	);
	write_changeset(
		dir.path(),
		"cs-b.md",
		"+++\npkg-b = \"major\"\n+++\n\nBreaking change in b\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "1.0.1");
	// pkg-b already had major; propagation should not downgrade it.
	assert_eq!(read_version(dir.path(), "pkg-b"), "2.0.0");
}

#[test]
fn propagation_upgrades_lower_existing_changeset_when_needed() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	// pkg-a gets a major bump (propagates major under "auto").
	// pkg-b already has a patch changeset — propagation should upgrade to major.
	write_changeset(
		dir.path(),
		"cs-a.md",
		"+++\npkg-a = \"major\"\n+++\n\nBreaking\n",
	);
	write_changeset(
		dir.path(),
		"cs-b.md",
		"+++\npkg-b = \"patch\"\n+++\n\nSmall fix in b\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "2.0.0");
	// pkg-b had patch but propagation from major gives major → promotes to major.
	assert_eq!(read_version(dir.path(), "pkg-b"), "2.0.0");
}

// ── Changelog contains Dependencies section ───────────────────────────────

#[test]
fn propagation_only_package_gets_dependencies_section_in_changelog() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"minor\"\n+++\n\nNew feature\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	let changelog = std::fs::read_to_string(dir.path().join("pkg-b/CHANGELOG.md")).unwrap();
	assert!(
		changelog.contains("### Dependencies"),
		"Expected '### Dependencies' section in pkg-b changelog, got:\n{changelog}"
	);
	assert!(
		changelog.contains("pkg-a"),
		"Expected pkg-a mention in pkg-b changelog, got:\n{changelog}"
	);
}

// ── No propagation when no workspace deps ────────────────────────────────

#[test]
fn no_propagation_for_packages_without_workspace_deps() {
	init_test_logger();
	let _ = take_logs();
	// Use a workspace where packages do NOT depend on each other.
	let dir = temp_git_repo_with_cargo_workspace(&[("pkg-a", "1.0.0"), ("pkg-b", "1.0.0")]);
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"major\"\n+++\n\nBreaking\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	assert_eq!(read_version(dir.path(), "pkg-a"), "2.0.0");
	// pkg-b has no dependency on pkg-a — should not be bumped.
	assert_eq!(read_version(dir.path(), "pkg-b"), "1.0.0");
}

// ── Scoped prepare generates changeset for out-of-scope dependents ────────

#[test]
fn scoped_prepare_generates_changeset_for_out_of_scope_dependent() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	// Only prepare pkg-a. pkg-b depends on pkg-a but is out of scope.
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"minor\"\n+++\n\nNew feature\n",
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
	);
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// pkg-a should be bumped.
	assert_eq!(read_version(dir.path(), "pkg-a"), "1.1.0");
	// pkg-b was out of scope and should NOT be bumped in this run.
	assert_eq!(read_version(dir.path(), "pkg-b"), "1.0.0");

	// A new changeset file should have been created for pkg-b.
	let cursus_dir = dir.path().join(".cursus");
	let changeset_files: Vec<_> = std::fs::read_dir(&cursus_dir)
		.unwrap()
		.filter_map(|e| e.ok())
		.filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
		.collect();
	assert!(
		!changeset_files.is_empty(),
		"Expected at least one changeset file for out-of-scope pkg-b, found none"
	);

	// The generated changeset should mention pkg-b.
	let has_pkg_b_changeset = changeset_files.iter().any(|entry| {
		let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
		content.contains("pkg-b")
	});
	assert!(
		has_pkg_b_changeset,
		"Expected a changeset file containing 'pkg-b'"
	);
}

// ── Dry-run ────────────────────────────────────────────────────────────────

#[test]
fn propagation_dry_run_does_not_modify_files() {
	init_test_logger();
	let _ = take_logs();
	let dir = workspace_with_dependency();
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"minor\"\n+++\n\nFeature\n",
	);

	let result = common::run_cursus(
		["cursus", "--no-interactive", "prepare", "--dry-run"],
		dir.path(),
	);
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// No versions should have changed.
	assert_eq!(read_version(dir.path(), "pkg-a"), "1.0.0");
	assert_eq!(read_version(dir.path(), "pkg-b"), "1.0.0");

	// No changeset should have been consumed.
	assert!(dir.path().join(".cursus/cs.md").exists());

	// Logs should mention propagation.
	let logs = take_logs();
	assert!(
		logs.iter().any(|(_, m)| m.contains("pkg-b")),
		"Expected log mentioning pkg-b in dry-run, got: {logs:?}"
	);
}

// ── Circular dependency (npm) ───────────────────────────────────────────────

/// Creates an npm workspace with pkg-a and pkg-b mutually depending on each other.
///
/// Unlike Cargo, npm does not reject circular workspace dependencies, so this
/// exercises the BFS cycle-termination property of the propagation algorithm.
fn npm_workspace_with_cycle() -> tempfile::TempDir {
	let dir = temp_git_repo();
	let config =
		Config::new(&AbsolutePath::new(dir.path()).unwrap()).with_npm(NpmConfig::enabled());
	config.with_env(common::test_env()).save().unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "0.0.0", "private": true, "workspaces": ["pkg-a", "pkg-b"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("pkg-a")).unwrap();
	std::fs::write(
		dir.path().join("pkg-a/package.json"),
		r#"{"name": "pkg-a", "version": "1.0.0", "dependencies": {"pkg-b": "1.0.0"}}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("pkg-b")).unwrap();
	std::fs::write(
		dir.path().join("pkg-b/package.json"),
		r#"{"name": "pkg-b", "version": "1.0.0", "dependencies": {"pkg-a": "1.0.0"}}"#,
	)
	.unwrap();

	dir
}

/// Reads the version field from a package's package.json.
fn read_npm_version(dir: &std::path::Path, pkg: &str) -> String {
	let json = std::fs::read_to_string(dir.join(format!("{pkg}/package.json"))).unwrap();
	let v: serde_json::Value = serde_json::from_str(&json).unwrap();
	v["version"].as_str().unwrap().to_string()
}

#[test]
fn propagation_npm_circular_deps_terminates_and_bumps_dependent() {
	init_test_logger();
	let _ = take_logs();
	let dir = npm_workspace_with_cycle();
	// Only pkg-a has a changeset; pkg-b should receive a propagated patch bump
	// under the default "auto" mode (minor upstream → patch propagation).
	write_changeset(
		dir.path(),
		"cs.md",
		"+++\npkg-a = \"minor\"\n+++\n\nNew feature\n",
	);

	let result = common::run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "prepare failed: {result:?}");
	assert_eq!(result.unwrap(), ExitCode::SUCCESS);

	// pkg-a has a minor changeset → 1.1.0
	assert_eq!(read_npm_version(dir.path(), "pkg-a"), "1.1.0");
	// pkg-b depends on pkg-a; auto-mode minor → patch propagation → 1.0.1
	assert_eq!(read_npm_version(dir.path(), "pkg-b"), "1.0.1");
}
