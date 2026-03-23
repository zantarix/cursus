//! Integration tests for the `[global].ignore` package filtering feature.

mod common;

use common::temp_git_repo_with_cargo_workspace;
use cursus::model::config::{CargoConfig, Config, GlobalConfig};

/// Creates a workspace with the ignore list set, saves config, and runs `change`.
///
/// Returns an error if cursus itself fails, or the project list if it succeeds.
fn run_change_with_ignore(
	members: &[(&str, &str)],
	ignore_patterns: Vec<String>,
	project: Option<&str>,
) -> anyhow::Result<std::process::ExitCode> {
	// Build a git repo with the workspace packages.
	let dir = temp_git_repo_with_cargo_workspace(members);

	// Overwrite the config with the ignore list.
	let mut global = GlobalConfig::default();
	global.ignore = ignore_patterns;
	let config = Config::new(&common::test_env(dir.path()))
		.with_global(global)
		.with_cargo(CargoConfig::enabled());
	config.save().unwrap();

	let mut args = vec![
		"cursus",
		"--no-interactive",
		"change",
		"-t",
		"minor",
		"-m",
		"test",
	];
	if let Some(p) = project {
		args.extend(["-p", p]);
	}

	common::run_cursus(args, dir.path())
}

#[test]
fn ignored_package_is_excluded_from_change_targets() {
	// Two packages: "app" and "internal-tool". Ignore the latter.
	// Running `change` with `-p app` should succeed (app is still visible).
	let result = run_change_with_ignore(
		&[("app", "0.1.0"), ("internal-tool", "0.1.0")],
		vec!["internal-tool".to_string()],
		Some("app"),
	);
	assert!(result.is_ok(), "Expected success, got: {result:?}");
}

#[test]
fn ignored_package_cannot_be_targeted_by_change() {
	// "internal-tool" is ignored, so targeting it with `-p internal-tool` must fail
	// because it will not appear in the enumerated project list.
	let result = run_change_with_ignore(
		&[("app", "0.1.0"), ("internal-tool", "0.1.0")],
		vec!["internal-tool".to_string()],
		Some("internal-tool"),
	);
	assert!(
		result.is_err(),
		"Expected an error when targeting an ignored package"
	);
}

#[test]
fn glob_pattern_excludes_multiple_packages() {
	// "example-basic" and "example-advanced" should both be filtered out by "example-*".
	// Only "core" remains.
	let result = run_change_with_ignore(
		&[
			("core", "0.1.0"),
			("example-basic", "0.1.0"),
			("example-advanced", "0.1.0"),
		],
		vec!["example-*".to_string()],
		Some("core"),
	);
	assert!(
		result.is_ok(),
		"Expected success targeting 'core': {result:?}"
	);
}

#[test]
fn ignoring_all_packages_gives_informative_error() {
	// When every project is filtered out, the error must mention [global].ignore
	// rather than the generic "No projects found" message.
	let result = run_change_with_ignore(&[("app", "0.1.0")], vec!["app".to_string()], None);
	assert!(
		result.is_err(),
		"Expected an error when all packages are ignored"
	);
	let err = result.unwrap_err().to_string();
	assert!(
		err.contains("excluded by [global].ignore"),
		"Expected informative error about ignore patterns, got: {err}"
	);
}
