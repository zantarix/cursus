//! Chronicle is a CLI tool that manages project configuration via an interactive TUI setup wizard.

#![feature(coverage_attribute)]
#![deny(clippy::too_many_lines)]

pub mod cli;
pub mod command;
pub(crate) mod conventional_commit;
pub(crate) mod env;
pub mod git;
pub mod github;
pub mod model;
pub mod package_manager;
#[cfg(feature = "test-support")]
pub mod path;
#[cfg(not(feature = "test-support"))]
pub(crate) mod path;
pub mod tui;
pub mod utils;

#[cfg(any(test, feature = "test-support"))]
pub mod test_logging;

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

pub use env::Env;

use crate::path::AbsolutePath;

/// Finds the git working directory by walking up from the given path.
///
/// Returns `Some(path)` if a `.git` directory is found, `None` otherwise.
fn find_git_workdir(start: &AbsolutePath) -> Option<AbsolutePath> {
	std::iter::successors(Some(start.to_path_buf()), |dir| {
		dir.parent().map(Path::to_path_buf)
	})
	.find(|dir| dir.join(".git").exists())
	.and_then(|p| AbsolutePath::new(p).ok())
}

/// Main entry point for the chronicle application.
///
/// Parses CLI arguments from the provided iterator, then delegates to
/// [`run_with`]. Use [`run_with`] directly when the arguments have already
/// been parsed (e.g., to initialise logging from the flags before running).
pub fn run<I, T>(args: I, cwd: &Path, env: Env) -> anyhow::Result<ExitCode>
where
	I: IntoIterator<Item = T>,
	T: Into<OsString> + Clone,
{
	let cli = match cli::Cli::try_parse_from(args) {
		Ok(cli) => cli,
		Err(e) => {
			// clap returns errors for help/version requests too
			// Use clap's error printing to handle them correctly
			e.print().context("Failed to print clap error")?;
			let exit_code = if e.use_stderr() {
				ExitCode::FAILURE
			} else {
				ExitCode::SUCCESS
			};
			return Ok(exit_code);
		}
	};
	run_with(cli, cwd, env)
}

/// Dispatches a pre-parsed CLI to the appropriate subcommand.
///
/// Prefer this over [`run`] when the caller has already parsed the arguments
/// (for example, to read the verbose/silent flags and initialise logging before
/// any library code runs).
///
/// When `--dry-run` is set, the environment's command runner is automatically
/// wrapped in a [`crate::command::DryRunCommandRunner`] so that mutating
/// subprocess calls (git commits, cargo publish, etc.) are suppressed across
/// all code paths — both the binary and integration tests.
pub fn run_with(cli: cli::Cli, cwd: &Path, env: Env) -> anyhow::Result<ExitCode> {
	let cwd_abs = AbsolutePath::new(cwd).context("current working directory is not absolute")?;
	let git_workdir = find_git_workdir(&cwd_abs).context("No git repository found")?;

	// Wrap the runner with DryRunCommandRunner when --dry-run is active so that
	// all mutating subprocess calls are silently suppressed.
	let dry_run = cli.global.dry_run;
	let env = if dry_run {
		env.with_dry_run_runner()
	} else {
		env
	};

	let git = git::GitWorkdir::new(&env, git_workdir.clone());

	match cli.command {
		Some(cli::Command::Init(args)) => cli::cmd_init(&git_workdir, &args, &cli.global, &env),
		command => {
			let config = model::config::load(&git_workdir, &env)?;
			match command {
				Some(cli::Command::Change(args)) => {
					cli::cmd_change(&git, &args, &cli.global, config)
				}
				Some(cli::Command::Prepare(args)) => cli::cmd_prepare(&git, &args, dry_run, config),
				Some(cli::Command::Publish(args)) => cli::cmd_publish(&git, &args, dry_run, config),
				Some(cli::Command::Ci(args)) => cli::cmd_ci(&git, &args, dry_run, config),
				None => cli::cmd_change(&git, &cli::ChangeArgs::default(), &cli.global, config),
				Some(cli::Command::Init(_)) => {
					// The outer match arm already handles Init; this arm cannot be reached.
					anyhow::bail!(
						"Unexpected Init command in inner dispatch - this is a bug, please report it."
					)
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	#[test]
	fn find_git_workdir_returns_none_when_no_git() {
		let dir = temp_dir();
		assert!(find_git_workdir(&AbsolutePath::new(dir.path()).unwrap()).is_none());
	}

	#[test]
	fn find_git_workdir_finds_git_in_current_dir() {
		let dir = temp_dir();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let result = find_git_workdir(&AbsolutePath::new(dir.path()).unwrap());
		assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
	}

	#[test]
	fn find_git_workdir_finds_git_in_parent_dir() {
		let dir = temp_dir();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let subdir = dir.path().join("subdir");
		std::fs::create_dir(&subdir).unwrap();

		let result = find_git_workdir(&AbsolutePath::new(&subdir).unwrap());
		assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
	}

	#[test]
	fn find_git_workdir_finds_git_in_nested_parent() {
		let dir = temp_dir();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let nested = dir.path().join("a/b/c");
		std::fs::create_dir_all(&nested).unwrap();

		let result = find_git_workdir(&AbsolutePath::new(&nested).unwrap());
		assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
	}

	#[test]
	fn find_git_workdir_stops_at_first_git() {
		let dir = temp_dir();
		// Create nested git repos
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let inner = dir.path().join("inner");
		std::fs::create_dir_all(inner.join(".git")).unwrap();

		// From inner, should find inner's .git
		let result = find_git_workdir(&AbsolutePath::new(&inner).unwrap());
		assert_eq!(result, Some(AbsolutePath::new(inner).unwrap()));
	}
}
