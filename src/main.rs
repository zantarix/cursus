mod cli;
mod config;
mod tui;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

fn find_git_workdir(start: &Path) -> Option<PathBuf> {
	let mut current = Some(start.to_path_buf());
	while let Some(dir) = current {
		if dir.join(".git").exists() {
			return Some(dir);
		}
		current = dir.parent().map(Path::to_path_buf);
	}
	None
}

fn run() -> anyhow::Result<ExitCode> {
	let cli = cli::Cli::parse();
	let cwd = std::env::current_dir().context("Failed to get current working directory")?;
	let git_workdir = find_git_workdir(&cwd).context("No git repository found")?;

	match cli.command {
		Some(cli::Command::Init) => cli::cmd_init(&git_workdir),
		Some(cli::Command::Change) | None => cli::cmd_change(&git_workdir),
	}
}

fn main() -> ExitCode {
	match run() {
		Ok(code) => code,
		Err(e) => {
			eprintln!("Error: {e:#}");
			ExitCode::FAILURE
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
