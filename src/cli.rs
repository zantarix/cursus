use std::path::Path;
use std::process::ExitCode;

use anyhow::bail;
use clap::{Parser, Subcommand};

use crate::config;
use crate::tui::init;

/// Command-line interface for chronicle.
#[derive(Parser)]
#[command(name = "chronicle", about = "Release management")]
pub struct Cli {
	#[command(subcommand)]
	pub command: Option<Command>,
}

/// Available subcommands.
#[derive(Subcommand)]
pub enum Command {
	/// Generate output from the current configuration
	Change,
	/// Initialize a new chronicle configuration using the setup wizard
	Init,
}

/// Runs the `init` subcommand.
pub fn cmd_init(git_workdir: &Path) -> anyhow::Result<ExitCode> {
	if config::exists(git_workdir) {
		bail!("Configuration already exists.");
	}
	match init::setup(git_workdir)? {
		Some(config) => {
			let path = config::create(git_workdir, &config)?;
			println!("Created {}", path.display());
			Ok(ExitCode::SUCCESS)
		}
		None => Ok(ExitCode::from(2)),
	}
}

/// Runs the `change` subcommand.
pub fn cmd_change(git_workdir: &Path) -> anyhow::Result<ExitCode> {
	if !config::exists(git_workdir) {
		bail!("No configuration found. Run 'chronicle init' to create one.");
	}
	let _config = config::load(git_workdir)?;
	println!("{}", git_workdir.display());
	Ok(ExitCode::SUCCESS)
}
