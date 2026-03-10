//! Chronicle is a CLI tool that manages project configuration via an interactive TUI setup wizard.

#![feature(coverage_attribute)]

pub mod cli;
pub mod command;
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
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;

use crate::command::CommandRunner;
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

/// Environment variables used by Chronicle.
///
/// Populated from the process environment at the binary boundary and threaded
/// into the library so that internal functions never read `std::env` directly.
#[derive(Debug, Clone, Default)]
pub struct Env {
	/// Value of the `VISUAL` environment variable.
	pub visual: Option<String>,
	/// Value of the `EDITOR` environment variable.
	pub editor: Option<String>,
}

/// Main entry point for the chronicle application.
///
/// Parses CLI arguments from the provided iterator, then delegates to
/// [`run_with`]. Use [`run_with`] directly when the arguments have already
/// been parsed (e.g., to initialise logging from the flags before running).
pub fn run<I, T>(
	args: I,
	cwd: &Path,
	env: Env,
	runner: Arc<dyn CommandRunner>,
	github_client: Option<Arc<dyn github::client::GitHubClient>>,
) -> anyhow::Result<ExitCode>
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
	run_with(cli, cwd, env, runner, github_client)
}

/// Dispatches a pre-parsed CLI to the appropriate subcommand.
///
/// Prefer this over [`run`] when the caller has already parsed the arguments
/// (for example, to read the verbose/silent flags and initialise logging before
/// any library code runs).
pub fn run_with(
	cli: cli::Cli,
	cwd: &Path,
	env: Env,
	runner: Arc<dyn CommandRunner>,
	github_client: Option<Arc<dyn github::client::GitHubClient>>,
) -> anyhow::Result<ExitCode> {
	let cwd_abs = AbsolutePath::new(cwd).context("current working directory is not absolute")?;
	let git_workdir = find_git_workdir(&cwd_abs).context("No git repository found")?;
	let git = git::GitWorkdir::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		git_workdir.clone(),
	);

	match cli.command {
		Some(cli::Command::Init(args)) => cli::cmd_init(&git_workdir, &args, &cli.global),
		command => {
			let config = model::config::load(&git_workdir)?;
			match command {
				Some(cli::Command::Change(args)) => {
					cli::cmd_change(&git, &args, &cli.global, &env, config, Arc::clone(&runner))
				}
				Some(cli::Command::Prepare(args)) => {
					cli::cmd_prepare(&git, &args, config, Arc::clone(&runner), github_client)
				}
				Some(cli::Command::Publish(args)) => {
					cli::cmd_publish(&git, &args, config, Arc::clone(&runner), github_client)
				}
				Some(cli::Command::Ci(args)) => {
					cli::cmd_ci(&git, &args, config, Arc::clone(&runner), github_client)
				}
				None => cli::cmd_change(
					&git,
					&cli::ChangeArgs::default(),
					&cli.global,
					&env,
					config,
					Arc::clone(&runner),
				),
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
