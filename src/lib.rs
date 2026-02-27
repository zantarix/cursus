//! Chronicle is a CLI tool that manages project configuration via an interactive TUI setup wizard.

#![feature(coverage_attribute)]

pub mod cli;
pub mod model;
pub mod package_manager;
pub mod tui;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

/// Finds the git working directory by walking up from the given path.
///
/// Returns `Some(path)` if a `.git` directory is found, `None` otherwise.
fn find_git_workdir(start: &Path) -> Option<PathBuf> {
	std::iter::successors(Some(start.to_path_buf()), |dir| {
		dir.parent().map(Path::to_path_buf)
	})
	.find(|dir| dir.join(".git").exists())
}

/// Main entry point for the chronicle application.
///
/// Parses CLI arguments from the provided iterator, finds the git root
/// starting from the given working directory, and dispatches to the appropriate command.
pub fn run<I, T>(args: I, cwd: &Path) -> anyhow::Result<ExitCode>
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

	let git_workdir = find_git_workdir(cwd).context("No git repository found")?;

	match cli.command {
		Some(cli::Command::Init(args)) => cli::cmd_init(&git_workdir, &args, &cli.global),
		Some(cli::Command::Change(args)) => cli::cmd_change(&git_workdir, &args, &cli.global),
		Some(cli::Command::Publish(args)) => cli::cmd_publish(&git_workdir, &args),
		Some(cli::Command::Release(args)) => cli::cmd_release(&git_workdir, &args),
		None => cli::cmd_change(&git_workdir, &cli::ChangeArgs::default(), &cli.global),
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
		assert!(find_git_workdir(dir.path()).is_none());
	}

	#[test]
	fn find_git_workdir_finds_git_in_current_dir() {
		let dir = temp_dir();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let result = find_git_workdir(dir.path());
		assert_eq!(result, Some(dir.path().to_path_buf()));
	}

	#[test]
	fn find_git_workdir_finds_git_in_parent_dir() {
		let dir = temp_dir();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let subdir = dir.path().join("subdir");
		std::fs::create_dir(&subdir).unwrap();

		let result = find_git_workdir(&subdir);
		assert_eq!(result, Some(dir.path().to_path_buf()));
	}

	#[test]
	fn find_git_workdir_finds_git_in_nested_parent() {
		let dir = temp_dir();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let nested = dir.path().join("a/b/c");
		std::fs::create_dir_all(&nested).unwrap();

		let result = find_git_workdir(&nested);
		assert_eq!(result, Some(dir.path().to_path_buf()));
	}

	#[test]
	fn find_git_workdir_stops_at_first_git() {
		let dir = temp_dir();
		// Create nested git repos
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let inner = dir.path().join("inner");
		std::fs::create_dir_all(inner.join(".git")).unwrap();

		// From inner, should find inner's .git
		let result = find_git_workdir(&inner);
		assert_eq!(result, Some(inner));
	}
}
