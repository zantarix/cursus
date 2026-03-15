mod common;

use std::sync::Arc;

use common::{run_cursus, temp_git_repo};
use cursus::command::RealCommandRunner;
use cursus::test_logging::{init_test_logger, take_logs};

/// Runs cursus in-process with a fake GitHub token configured.
///
/// Equivalent to `run_cursus` but with a `RestGitHubClient` set up
/// using the provided token string, matching what `main.rs` does at runtime.
fn run_cursus_with_token(
	args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
	cwd: &std::path::Path,
	token: &str,
) -> anyhow::Result<std::process::ExitCode> {
	let github_client = Arc::new(cursus::github::RestGitHubClient::new(token.to_string()))
		as Arc<dyn cursus::github::client::GitHubClient>;
	let env =
		cursus::Env::new(Arc::new(RealCommandRunner) as Arc<dyn cursus::command::CommandRunner>)
			.with_github_client(github_client);
	cursus::run(args, cwd, env)
}

/// Helper: write a config file with the given TOML content under `.cursus/`.
fn write_config(dir: &std::path::Path, toml: &str) {
	let config_dir = dir.join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), toml).unwrap();
}

#[test]
fn github_config_section_loads_correctly() {
	let dir = temp_git_repo();
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[github]\nenabled = true\n",
	);
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	// If the config fails to parse, cursus would error before reaching the
	// publish step. A non-interactive publish --dry-run exercises the full
	// config load path without hitting a registry.
	let result = run_cursus(
		["cursus", "publish", "--dry-run", "--no-interactive"],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

#[test]
fn github_unknown_field_causes_parse_error() {
	let dir = temp_git_repo();
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[github]\nunknown_field = true\n",
	);

	let result = run_cursus(
		["cursus", "publish", "--dry-run", "--no-interactive"],
		dir.path(),
	);
	let err = result.unwrap_err();
	let msg = format!("{err:#}");
	assert!(
		msg.contains("unknown field"),
		"Expected 'unknown field' error, got: {msg}"
	);
}

#[test]
fn github_enabled_implies_git_enabled_integration() {
	let dir = temp_git_repo();
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[github]\nenabled = true\n",
	);
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let result = run_cursus(
		["cursus", "publish", "--dry-run", "--no-interactive"],
		dir.path(),
	);
	// The command should succeed (dry-run, no network). The key assertion is
	// that config loading did not fail, which would mean github→git derivation worked.
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

/// Dry-run with GitHub enabled prints "Would create GitHub Release" to log.
#[test]
fn publish_dry_run_with_github_shows_would_create() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo();
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[github]\nenabled = true\nowner = \"acme\"\nrepo = \"app\"\n",
	);
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();

	let result = run_cursus_with_token(
		["cursus", "publish", "--dry-run", "--no-interactive"],
		dir.path(),
		"test-token",
	);
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Would create GitHub Release for v1.0.0")),
		"Expected 'Would create GitHub Release for v1.0.0' in logs, got: {logs:?}"
	);
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Would publish release after artifact upload")),
		"Expected 'Would publish release after artifact upload' in logs, got: {logs:?}"
	);
}

/// During dry-run, the build_command is suppressed by the DryRunCommandRunner.
/// Verified by using `false` (always exits 1) as the build_command and checking success.
#[test]
fn publish_dry_run_with_github_no_build_command_executed() {
	let dir = temp_git_repo();
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[github]\nenabled = true\nowner = \"acme\"\nrepo = \"app\"\nbuild_command = \"false\"\n",
	);
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();

	let result = run_cursus_with_token(
		["cursus", "publish", "--dry-run", "--no-interactive"],
		dir.path(),
		"test-token",
	);
	// If build_command were executed, `false` (exits 1) would cause orchestration to fail.
	// In dry-run it must be skipped → success.
	assert!(
		result.is_ok(),
		"Expected success (build_command skipped in dry-run), got: {result:?}"
	);
}

/// A failing build_command causes publish to exit with failure before any packages are published.
///
/// Using `false` as the build_command (always exits 1) with a real runner verifies that
/// the build command runs before any registry publish, and a failure halts the whole operation.
#[test]
fn publish_build_command_failure_aborts_before_publishing() {
	cursus::test_logging::init_test_logger();
	let _ = cursus::test_logging::take_logs();

	let dir = temp_git_repo();
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[github]\nenabled = true\nowner = \"acme\"\nrepo = \"app\"\nbuild_command = \"false\"\n",
	);
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();

	let result = run_cursus_with_token(
		["cursus", "publish", "--no-interactive"],
		dir.path(),
		"test-token",
	);
	assert!(result.is_ok(), "Expected Ok(ExitCode), got: {result:?}");
	assert_eq!(
		result.unwrap(),
		std::process::ExitCode::FAILURE,
		"Expected ExitCode::FAILURE when build command fails"
	);

	let logs = cursus::test_logging::take_logs();
	assert!(
		logs.iter().any(|(_, m)| m.contains("Build command failed")),
		"Expected 'Build command failed' in logs, got: {logs:?}"
	);
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("Published") || m.contains("Would publish")),
		"No publish should have been attempted after build command failure, got: {logs:?}"
	);
}

/// When GitHub Releases is enabled and no token is set, the command fails immediately
/// without attempting to publish anything.
#[test]
fn publish_github_missing_token_fails() {
	let dir = temp_git_repo();
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[github]\nenabled = true\nowner = \"acme\"\nrepo = \"app\"\n",
	);
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();

	// Run without a token — Env has no github client, so it should fail before publishing.
	let result = run_cursus(["cursus", "publish", "--no-interactive"], dir.path());
	let err = result.unwrap_err();
	let msg = format!("{err:#}");
	assert!(
		msg.contains("no GitHub token"),
		"Expected token error in error message, got: {msg}"
	);
}

/// Dry-run with artifacts configured prints "Would attach: {name}" and "Would publish release"
/// for each artifact.
#[test]
fn publish_dry_run_with_artifacts_shows_would_attach() {
	init_test_logger();
	let _ = take_logs();
	let dir = temp_git_repo();
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[github]\nenabled = true\nowner = \"acme\"\nrepo = \"app\"\n[github.artifacts]\n\"linux-amd64\" = \"target/release/my-app\"\n",
	);
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();

	let result = run_cursus_with_token(
		["cursus", "publish", "--dry-run", "--no-interactive"],
		dir.path(),
		"test-token",
	);
	assert!(result.is_ok(), "Expected success, got: {result:?}");

	let logs = take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Would attach: linux-amd64")),
		"Expected 'Would attach: linux-amd64' in logs, got: {logs:?}"
	);
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("Would publish release after artifact upload")),
		"Expected 'Would publish release after artifact upload' in logs, got: {logs:?}"
	);
}
