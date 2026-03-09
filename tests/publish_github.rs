mod common;

use std::path::Path;

use common::{run_chronicle, run_chronicle_with_env, temp_git_repo};

/// Runs chronicle as a subprocess with the given environment variables, capturing stdout/stderr.
///
/// Returns `(success, stdout, stderr)`.
fn run_chronicle_subprocess_with_env(
	args: &[&str],
	cwd: &Path,
	env_vars: &[(&str, &str)],
) -> (bool, String, String) {
	let bin = env!("CARGO_BIN_EXE_chronicle");
	let mut cmd = std::process::Command::new(bin);
	cmd.args(args).current_dir(cwd);
	// Clear GitHub token vars to prevent leaking from the test runner's environment.
	cmd.env_remove("GH_TOKEN");
	cmd.env_remove("GITHUB_TOKEN");
	for (key, val) in env_vars {
		cmd.env(key, val);
	}
	let output = cmd.output().expect("Failed to spawn chronicle subprocess");
	let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
	let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
	(output.status.success(), stdout, stderr)
}

/// Helper: write a config file with the given TOML content under `.chronicle/`.
fn write_config(dir: &std::path::Path, toml: &str) {
	let config_dir = dir.join(".chronicle");
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

	// If the config fails to parse, chronicle would error before reaching the
	// publish step. A non-interactive publish --dry-run exercises the full
	// config load path without hitting a registry.
	let result = run_chronicle(
		["chronicle", "publish", "--dry-run", "--no-interactive"],
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

	let result = run_chronicle(
		["chronicle", "publish", "--dry-run", "--no-interactive"],
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

	let env = chronicle::Env {
		github_token: Some("test-token".to_string()),
		..Default::default()
	};
	let result = run_chronicle_with_env(
		["chronicle", "publish", "--dry-run", "--no-interactive"],
		dir.path(),
		env,
	);
	// The command should succeed (dry-run, no network). The key assertion is
	// that config loading did not fail, which would mean github→git derivation worked.
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
}

/// Dry-run with GitHub enabled prints "Would create GitHub Release" to stdout.
#[test]
fn publish_dry_run_with_github_shows_would_create() {
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

	let (success, stdout, stderr) = run_chronicle_subprocess_with_env(
		&["publish", "--dry-run", "--no-interactive"],
		dir.path(),
		&[("GH_TOKEN", "test-token")],
	);
	assert!(success, "Expected success, stderr: {stderr}");
	assert!(
		stdout.contains("Would create GitHub Release for v1.0.0"),
		"Expected 'Would create GitHub Release for v1.0.0' in stdout, got: {stdout}"
	);
}

/// During dry-run, the build_command is never executed.
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

	let (success, _stdout, stderr) = run_chronicle_subprocess_with_env(
		&["publish", "--dry-run", "--no-interactive"],
		dir.path(),
		&[("GH_TOKEN", "test-token")],
	);
	// If build_command were executed, `false` (exits 1) would cause orchestration to fail.
	// In dry-run it must be skipped → success.
	assert!(
		success,
		"Expected success (build_command skipped in dry-run), stderr: {stderr}"
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

	// Run without dry-run and without a token — should fail before publishing
	let (success, _stdout, stderr) = run_chronicle_subprocess_with_env(
		&["publish", "--no-interactive"],
		dir.path(),
		&[], // no GH_TOKEN
	);
	assert!(!success, "Expected failure, stderr: {stderr}");
	assert!(
		stderr.contains("no GitHub token"),
		"Expected token error in stderr, got: {stderr}"
	);
}

/// Dry-run with artifacts configured prints "Would attach: {name}" for each artifact.
#[test]
fn publish_dry_run_with_artifacts_shows_would_attach() {
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

	let (success, stdout, stderr) = run_chronicle_subprocess_with_env(
		&["publish", "--dry-run", "--no-interactive"],
		dir.path(),
		&[("GH_TOKEN", "test-token")],
	);
	assert!(success, "Expected success, stderr: {stderr}");
	assert!(
		stdout.contains("Would attach: linux-amd64"),
		"Expected 'Would attach: linux-amd64' in stdout, got: {stdout}"
	);
}
