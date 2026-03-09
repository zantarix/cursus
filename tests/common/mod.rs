//! Shared test helpers for integration tests.

// Each tests/*.rs binary compiles this module independently; helpers used in one
// binary but not another appear as dead code in that binary's compilation unit.
#![allow(dead_code)]

use std::process::Command;
use std::sync::Arc;

use chronicle::command::CommandRunner;
use chronicle::git::GitConfig;
use chronicle::model::config::{Config, PackageManager};
use chronicle::package_manager::{CargoConfig, NpmConfig};
use tempfile::TempDir;

/// Runs chronicle with a default (empty) environment and real command runner, returning the result.
///
/// This is the standard way to invoke `chronicle::run` from integration tests.
/// It passes `Env::default()` so that no real environment variables are read,
/// and uses `RealCommandRunner` so that actual shell commands (git, cargo, npm) execute.
pub fn run_chronicle(
	args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
	cwd: &std::path::Path,
) -> anyhow::Result<std::process::ExitCode> {
	let runner: Arc<dyn CommandRunner> = Arc::new(chronicle::command::RealCommandRunner);
	chronicle::run(args, cwd, chronicle::Env::default(), runner)
}

/// Runs chronicle with a custom environment and real command runner, returning the result.
///
/// Like [`run_chronicle`] but accepts a caller-supplied [`chronicle::Env`], allowing
/// tests that need specific environment variables (e.g. `github_token`) to inject them.
pub fn run_chronicle_with_env(
	args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
	cwd: &std::path::Path,
	env: chronicle::Env,
) -> anyhow::Result<std::process::ExitCode> {
	let runner: Arc<dyn CommandRunner> = Arc::new(chronicle::command::RealCommandRunner);
	chronicle::run(args, cwd, env, runner)
}

/// Runs a git command in the given directory and panics on failure.
///
/// Stdout is suppressed to keep test output clean. Stderr is captured and
/// included in the panic message so failures are diagnosable.
fn git_cmd(dir: &std::path::Path, args: &[&str]) {
	let output = Command::new("git")
		.args(args)
		.current_dir(dir)
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::piped())
		.output()
		.unwrap_or_else(|e| panic!("Failed to spawn git {args:?}: {e}"));
	assert!(
		output.status.success(),
		"git {args:?} failed with status {}:\n{}",
		output.status,
		String::from_utf8_lossy(&output.stderr)
	);
}

/// Creates a real git repository with an initial commit in a temp directory.
///
/// Configures `user.name`, `user.email`, and disables commit/tag signing so that
/// git operations succeed in any environment without a GPG key.
fn temp_real_git_repo() -> TempDir {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	git_cmd(dir.path(), &["init"]);
	git_cmd(dir.path(), &["config", "user.name", "Chronicle Test"]);
	git_cmd(
		dir.path(),
		&["config", "user.email", "test@chronicle.local"],
	);
	git_cmd(dir.path(), &["config", "commit.gpgsign", "false"]);
	git_cmd(dir.path(), &["config", "tag.gpgsign", "false"]);
	// Create an initial empty commit so the repo has a HEAD
	git_cmd(
		dir.path(),
		&["commit", "--allow-empty", "-m", "chore: initial commit"],
	);
	dir
}

/// Creates a real git repository with a Chronicle config that has git lifecycle enabled.
pub fn temp_real_git_repo_with_config(pm: PackageManager, git_config: GitConfig) -> TempDir {
	let dir = temp_real_git_repo();
	let config = match pm {
		PackageManager::Npm => Config::new(dir.path())
			.with_npm(NpmConfig::enabled())
			.with_git(git_config),
		PackageManager::Cargo => Config::new(dir.path())
			.with_cargo(CargoConfig::enabled())
			.with_git(git_config),
	};
	config.save().unwrap();
	dir
}

/// Creates a real git repository with a Cargo workspace and Chronicle config.
///
/// All files are staged and committed in the initial state.
pub fn temp_real_git_repo_with_cargo_workspace(
	members: &[(&str, &str)],
	git_config: GitConfig,
) -> TempDir {
	let dir = temp_real_git_repo();
	let config = Config::new(dir.path())
		.with_cargo(CargoConfig::enabled())
		.with_git(git_config);
	config.save().unwrap();

	let member_list = members
		.iter()
		.map(|(name, _)| format!("\"{name}\""))
		.collect::<Vec<_>>()
		.join(", ");
	std::fs::write(
		dir.path().join("Cargo.toml"),
		format!("[workspace]\nmembers = [{member_list}]\n"),
	)
	.unwrap();

	for (name, version) in members {
		let pkg_dir = dir.path().join(name);
		std::fs::create_dir_all(pkg_dir.join("src")).unwrap();
		std::fs::write(
			pkg_dir.join("Cargo.toml"),
			format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
		)
		.unwrap();
		std::fs::write(pkg_dir.join("src/lib.rs"), "").unwrap();
	}

	// Commit all setup files
	git_cmd(dir.path(), &["add", "."]);
	git_cmd(dir.path(), &["commit", "-m", "chore: set up workspace"]);

	dir
}

/// Creates a bare "remote" repo and wires it as `origin` of the given working repo.
///
/// Returns the `TempDir` holding the bare repo (must be kept alive for the test duration).
pub fn add_local_remote(working_repo: &std::path::Path) -> TempDir {
	let remote_dir = tempfile::tempdir().expect("Failed to create remote dir");
	git_cmd(remote_dir.path(), &["init", "--bare"]);
	git_cmd(
		working_repo,
		&[
			"remote",
			"add",
			"origin",
			remote_dir.path().to_str().unwrap(),
		],
	);
	remote_dir
}

/// Reads the log of all git commits in the repo (most recent first).
pub fn git_log(dir: &std::path::Path) -> Vec<String> {
	let output = Command::new("git")
		.args(["log", "--format=%s"])
		.current_dir(dir)
		.output()
		.expect("Failed to run git log");
	String::from_utf8(output.stdout)
		.expect("git log output is not UTF-8")
		.lines()
		.map(str::to_string)
		.collect()
}

/// Returns the list of git tags in the repo.
pub fn git_tags(dir: &std::path::Path) -> Vec<String> {
	let output = Command::new("git")
		.args(["tag", "--list"])
		.current_dir(dir)
		.output()
		.expect("Failed to run git tag");
	String::from_utf8(output.stdout)
		.expect("git tag output is not UTF-8")
		.lines()
		.filter(|s| !s.is_empty())
		.map(str::to_string)
		.collect()
}

/// Returns `true` if the given tag exists in the repo.
pub fn git_tag_exists(dir: &std::path::Path, tag: &str) -> bool {
	git_tags(dir).contains(&tag.to_string())
}

/// Returns the name of the current git branch.
///
/// Panics if the command fails or the HEAD is detached.
pub fn git_current_branch(dir: &std::path::Path) -> String {
	let output = Command::new("git")
		.args(["rev-parse", "--abbrev-ref", "HEAD"])
		.current_dir(dir)
		.output()
		.expect("Failed to run git rev-parse");
	let branch = String::from_utf8(output.stdout)
		.expect("git rev-parse output is not UTF-8")
		.trim()
		.to_string();
	assert!(
		branch != "HEAD",
		"git_current_branch called in detached HEAD state"
	);
	branch
}

/// Runs chronicle as a real subprocess, capturing stdout and stderr.
///
/// Returns `(success, stdout, stderr)`. Use this instead of [`run_chronicle`] when
/// the command is expected to produce clap-generated output (e.g. `--help`, `--version`,
/// or invalid flags/subcommands) so that the output is captured rather than leaked to the
/// test runner's terminal.
pub fn run_chronicle_subprocess(args: &[&str], cwd: &std::path::Path) -> (bool, String, String) {
	let bin = env!("CARGO_BIN_EXE_chronicle");
	let output = Command::new(bin)
		.args(args)
		.current_dir(cwd)
		.output()
		.expect("Failed to spawn chronicle subprocess");
	let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
	let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
	(output.status.success(), stdout, stderr)
}

/// Creates a temporary directory with a `.git` folder to simulate a git repository.
pub fn temp_git_repo() -> TempDir {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	dir
}

/// Creates a temporary git repository with a Chronicle config file.
pub fn temp_git_repo_with_config(pm: PackageManager) -> TempDir {
	let dir = temp_git_repo();
	let config = match pm {
		PackageManager::Npm => Config::new(dir.path()).with_npm(NpmConfig::enabled()),
		PackageManager::Cargo => Config::new(dir.path()).with_cargo(CargoConfig::enabled()),
	};
	config.save().unwrap();
	dir
}

/// Creates a temporary git repository with a config and matching package manifest.
pub fn temp_git_repo_with_project(pm: PackageManager) -> TempDir {
	let dir = temp_git_repo();
	let config = match pm {
		PackageManager::Npm => Config::new(dir.path()).with_npm(NpmConfig::enabled()),
		PackageManager::Cargo => Config::new(dir.path()).with_cargo(CargoConfig::enabled()),
	};
	config.save().unwrap();
	match pm {
		PackageManager::Npm => {
			std::fs::write(
				dir.path().join("package.json"),
				r#"{"name": "test-project", "version": "0.1.0"}"#,
			)
			.unwrap();
		}
		PackageManager::Cargo => {
			std::fs::write(
				dir.path().join("Cargo.toml"),
				"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
			)
			.unwrap();
			// Create src/lib.rs so cargo can generate a valid Cargo.lock
			std::fs::create_dir_all(dir.path().join("src")).unwrap();
			std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
		}
	}
	dir
}

/// Creates a temporary git repository with a Cargo workspace containing named packages.
///
/// Each entry in `members` is a `(name, version)` pair. The workspace root
/// `Cargo.toml` lists all members, and each gets its own `Cargo.toml` and
/// an empty `src/lib.rs`.
pub fn temp_git_repo_with_cargo_workspace(members: &[(&str, &str)]) -> TempDir {
	let dir = temp_git_repo();
	let config = Config::new(dir.path()).with_cargo(CargoConfig::enabled());
	config.save().unwrap();

	let member_list = members
		.iter()
		.map(|(name, _)| format!("\"{name}\""))
		.collect::<Vec<_>>()
		.join(", ");
	std::fs::write(
		dir.path().join("Cargo.toml"),
		format!("[workspace]\nmembers = [{member_list}]\n"),
	)
	.unwrap();

	for (name, version) in members {
		let pkg_dir = dir.path().join(name);
		std::fs::create_dir_all(pkg_dir.join("src")).unwrap();
		std::fs::write(
			pkg_dir.join("Cargo.toml"),
			format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
		)
		.unwrap();
		std::fs::write(pkg_dir.join("src/lib.rs"), "").unwrap();
	}

	dir
}

/// Creates a temporary git repository with a config and package manifest in a subfolder.
pub fn temp_git_repo_with_project_in_subfolder(pm: PackageManager, subfolder: &str) -> TempDir {
	let dir = temp_git_repo();
	let mut config = match pm {
		PackageManager::Npm => Config::new(dir.path()).with_npm(NpmConfig::enabled()),
		PackageManager::Cargo => Config::new(dir.path()).with_cargo(CargoConfig::enabled()),
	};
	match pm {
		PackageManager::Npm => config.npm.path = Some(subfolder.to_string()),
		PackageManager::Cargo => config.cargo.path = Some(subfolder.to_string()),
	}
	config.save().unwrap();
	let sub_path = dir.path().join(subfolder);
	std::fs::create_dir_all(&sub_path).unwrap();
	match pm {
		PackageManager::Npm => {
			std::fs::write(
				sub_path.join("package.json"),
				r#"{"name": "test-project", "version": "0.1.0"}"#,
			)
			.unwrap();
		}
		PackageManager::Cargo => {
			std::fs::write(
				sub_path.join("Cargo.toml"),
				"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
			)
			.unwrap();
		}
	}
	dir
}
