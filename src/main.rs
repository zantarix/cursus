mod config;
mod tui;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;

use crate::tui::init::{SetupChoice, prompt_setup};

fn find_git_root(start: &Path) -> Option<PathBuf> {
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
	let cwd = std::env::current_dir().context("Failed to get current working directory")?;
	let git_root = find_git_root(&cwd).context("No git repository found")?;

	if !config::exists(&git_root) {
		match prompt_setup()? {
			SetupChoice::Yes => {
				let path = config::create(&git_root)?;
				println!("Created {}", path.display());
			}
			SetupChoice::No => {
				return Ok(ExitCode::from(2));
			}
		}
	}

	let _config = config::load(&git_root)?;
	println!("{}", git_root.display());

	Ok(ExitCode::SUCCESS)
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
