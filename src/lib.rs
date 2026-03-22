//! Cursus is a CLI tool that manages project configuration via an interactive TUI setup wizard.

#![feature(coverage_attribute)]

pub mod cli;
pub mod command;
pub mod conventional_commit;
pub(crate) mod env;
pub mod filesystem;
pub mod git;
pub mod github;
pub mod locale;
pub mod model;
pub mod package_manager;
pub mod path;
pub(crate) mod shell;
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

pub use env::Env;

use crate::path::AbsolutePath;

/// Finds the git working directory by walking up from the given path.
///
/// Returns `Some(path)` if a `.git` directory is found, `None` otherwise.
pub fn find_git_workdir(
	start: &AbsolutePath,
	fs: &dyn filesystem::Filesystem,
) -> Option<AbsolutePath> {
	std::iter::successors(Some(start.to_path_buf()), |dir| {
		dir.parent().map(Path::to_path_buf)
	})
	.find(|dir| AbsolutePath::new(dir.join(".git")).is_ok_and(|p| fs.exists(&p)))
	.and_then(|p| AbsolutePath::new(p).ok())
}

/// Main entry point for the cursus application.
///
/// Parses CLI arguments from the provided iterator, performs git discovery
/// and dry-run setup, then delegates to [`run_with`].
///
/// Use [`run_with`] directly when the caller has already parsed the arguments
/// and set up git on the environment (e.g., from `main` or when providing a
/// custom [`git::Git`] implementation).
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

	// Apply dry-run wrapping BEFORE creating GitWorkdir so it gets the
	// wrapped CommandRunner (per ADR-035 construction order).
	let dry_run = cli.global.dry_run;
	let env = if dry_run {
		env.with_dry_run_runner()
	} else {
		env
	};

	// Git discovery and GitWorkdir creation.
	let cwd_abs = AbsolutePath::new(cwd).context("current working directory is not absolute")?;
	let git_workdir = find_git_workdir(&cwd_abs, env.fs()).context("No git repository found")?;
	let git_impl = Arc::new(git::GitWorkdir::new(&env, git_workdir));
	let env = env.with_git(git_impl);

	run_with(cli, env)
}

/// Dispatches a pre-parsed CLI to the appropriate subcommand.
///
/// The environment must have git already configured via
/// [`Env::with_git`] before calling this function. Use [`run`] for the
/// convenience entry point that handles git discovery automatically.
pub fn run_with(cli: cli::Cli, env: Env) -> anyhow::Result<ExitCode> {
	// Set the process-global locale from the environment before any output.
	locale::set_locale(env.locale());

	let dry_run = cli.global.dry_run;
	let git = env.git().context("No git implementation set on Env")?;
	let git_workdir = git.path().clone();

	match cli.command {
		Some(cli::Command::Init(args)) => cli::cmd_init(&git_workdir, &args, &cli.global, &env),
		Some(cli::Command::Verify(args)) => cli::cmd_verify(git, &args),
		command => {
			let config = model::config::load(&git_workdir, &env)?;
			match command {
				Some(cli::Command::Change(args)) => {
					cli::cmd_change(git, &args, &cli.global, config)
				}
				Some(cli::Command::Prepare(args)) => cli::cmd_prepare(git, &args, dry_run, config),
				Some(cli::Command::Publish(args)) => cli::cmd_publish(git, &args, dry_run, config),
				Some(cli::Command::Ci(args)) => cli::cmd_ci(git, &args, dry_run, config),
				None => cli::cmd_change(git, &cli::ChangeArgs::default(), &cli.global, config),
				Some(cli::Command::Init(_)) | Some(cli::Command::Verify(_)) => {
					// The outer match arms already handle Init and Verify; these arms cannot be reached.
					anyhow::bail!(
						"Unexpected Init/Verify command in inner dispatch - this is a bug, please report it."
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
		let fs = crate::filesystem::LocalFilesystem;
		assert!(find_git_workdir(&AbsolutePath::new(dir.path()).unwrap(), &fs).is_none());
	}

	#[test]
	fn find_git_workdir_finds_git_in_current_dir() {
		let dir = temp_dir();
		let fs = crate::filesystem::LocalFilesystem;
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let result = find_git_workdir(&AbsolutePath::new(dir.path()).unwrap(), &fs);
		assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
	}

	#[test]
	fn find_git_workdir_finds_git_in_parent_dir() {
		let dir = temp_dir();
		let fs = crate::filesystem::LocalFilesystem;
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let subdir = dir.path().join("subdir");
		std::fs::create_dir(&subdir).unwrap();

		let result = find_git_workdir(&AbsolutePath::new(&subdir).unwrap(), &fs);
		assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
	}

	#[test]
	fn find_git_workdir_finds_git_in_nested_parent() {
		let dir = temp_dir();
		let fs = crate::filesystem::LocalFilesystem;
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let nested = dir.path().join("a/b/c");
		std::fs::create_dir_all(&nested).unwrap();

		let result = find_git_workdir(&AbsolutePath::new(&nested).unwrap(), &fs);
		assert_eq!(result, Some(AbsolutePath::new(dir.path()).unwrap()));
	}

	#[test]
	fn find_git_workdir_stops_at_first_git() {
		let dir = temp_dir();
		let fs = crate::filesystem::LocalFilesystem;
		// Create nested git repos
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let inner = dir.path().join("inner");
		std::fs::create_dir_all(inner.join(".git")).unwrap();

		// From inner, should find inner's .git
		let result = find_git_workdir(&AbsolutePath::new(&inner).unwrap(), &fs);
		assert_eq!(result, Some(AbsolutePath::new(inner).unwrap()));
	}
}
