//! Shared test helpers for binary crate subprocess tests.
//!
//! `test_env`, `temp_git_repo`, `temp_git_repo_with_config`, `temp_git_repo_with_project`,
//! and `write_changeset` are intentionally duplicated from `packages/cursus/tests/common/mod.rs`.
//! Keep them in sync when modifying either copy.

// Each tests/*.rs binary compiles this module independently; helpers used in one
// binary but not another appear as dead code in that binary's compilation unit.
#![allow(dead_code)]

use std::process::Command;

use cursus::model::config::{CargoConfig, Config, NpmConfig, PackageManager};

use tempfile::TempDir;

/// Creates a minimal `Env` with a real command runner, local filesystem, and git for the given dir.
pub(super) fn test_env(dir: &std::path::Path) -> cursus::Env {
	let runner = std::sync::Arc::new(cursus::command::RealCommandRunner)
		as std::sync::Arc<dyn cursus::command::CommandRunner>;
	let path = cursus::path::AbsolutePath::new(dir).unwrap();
	cursus::Env::new(
		std::sync::Arc::clone(&runner),
		std::sync::Arc::new(cursus::filesystem::LocalFilesystem),
		std::sync::Arc::new(cursus::git::GitWorkdir::new(runner, path)),
	)
}

/// Creates a temporary directory with a `.git` folder to simulate a git repository.
pub(super) fn temp_git_repo() -> TempDir {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	dir
}

/// Creates a temporary git repository with a Cursus config file.
pub(super) async fn temp_git_repo_with_config(pm: PackageManager) -> TempDir {
	let dir = temp_git_repo();
	let env = test_env(dir.path());
	let config = match pm {
		PackageManager::Npm => Config::new().with_npm(NpmConfig::enabled()),
		PackageManager::Cargo => Config::new().with_cargo(CargoConfig::enabled()),
	};
	config.save(env.fs(), env.git().path()).await.unwrap();
	dir
}

/// Creates a temporary git repository with a config and matching package manifest.
pub(super) async fn temp_git_repo_with_project(pm: PackageManager) -> TempDir {
	let dir = temp_git_repo();
	let env = test_env(dir.path());
	let config = match pm {
		PackageManager::Npm => Config::new().with_npm(NpmConfig::enabled()),
		PackageManager::Cargo => Config::new().with_cargo(CargoConfig::enabled()),
	};
	config.save(env.fs(), env.git().path()).await.unwrap();
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

/// Creates a changeset file in the `.cursus` directory.
///
/// The `content` should be a valid changeset with TOML frontmatter, e.g.:
/// `"+++\npkg-name = \"minor\"\n+++\n\nDescription\n"`.
pub(super) fn write_changeset(dir: &std::path::Path, filename: &str, content: &str) {
	let cursus_dir = dir.join(".cursus");
	std::fs::create_dir_all(&cursus_dir).unwrap();
	std::fs::write(cursus_dir.join(filename), content).unwrap();
}

/// Runs cursus as a real subprocess, capturing stdout and stderr.
///
/// Returns `(success, stdout, stderr)`. Use this instead of the library's `run_cursus`
/// when the command is expected to produce clap-generated output (e.g. `--help`, `--version`,
/// or invalid flags/subcommands) so that the output is captured rather than leaked to the
/// test runner's terminal.
pub(super) fn run_cursus_subprocess(
	args: &[&str],
	cwd: &std::path::Path,
) -> (bool, String, String) {
	let bin = env!("CARGO_BIN_EXE_cursus");
	let output = Command::new(bin)
		.args(args)
		.current_dir(cwd)
		.output()
		.expect("Failed to spawn cursus subprocess");
	let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
	let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
	(output.status.success(), stdout, stderr)
}

/// Runs the cursus binary inside a specific Nix dev shell.
///
/// Each shell provides a controlled, minimal set of tools matching a typical user
/// installation of a particular package manager (e.g. only npm, only pnpm, yarn
/// classic as `yarn`, yarn berry as `yarn`). This lets integration tests exercise
/// the auto-detection and version-branching logic in a realistic environment.
///
/// The pre-compiled cursus binary (`CARGO_BIN_EXE_cursus`) is used so no Rust
/// toolchain is required inside the shell. The shell is resolved against the
/// workspace flake using its absolute path.
///
/// Nix is a required development dependency of this project, so tests using
/// this helper run as part of the normal `cargo test` suite.
#[cfg(feature = "nix-tests")]
pub(super) fn run_cursus_in_nix_shell(
	shell_attr: &str,
	args: &[&str],
	cwd: &std::path::Path,
) -> (bool, String, String) {
	let bin = env!("CARGO_BIN_EXE_cursus");
	let flake_root = env!("CURSUS_WORKSPACE_ROOT");
	let flake_ref = format!("{flake_root}#{shell_attr}");
	let mut nix_args = vec!["develop", flake_ref.as_str(), "--command", bin];
	nix_args.extend_from_slice(args);
	let output = Command::new("nix")
		.args(&nix_args)
		.current_dir(cwd)
		.output()
		.expect("Failed to spawn `nix develop` — is nix installed?");
	let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
	let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
	(output.status.success(), stdout, stderr)
}
