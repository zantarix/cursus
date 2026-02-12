use std::path::Path;
use std::process::ExitCode;

use anyhow::bail;
use clap::{ArgAction, Args, Parser, Subcommand};

use crate::config::{self, Config, PackageManager};
use crate::tui::init;

/// Global arguments that apply to all subcommands.
#[derive(Args, Debug, Clone)]
pub struct GlobalArgs {
	/// Enable interactive mode (default)
	#[arg(long, global = true, default_value_t = true, action = ArgAction::SetTrue, overrides_with = "no_interactive")]
	pub interactive: bool,

	/// Disable interactive prompts
	#[arg(long, global = true, action = ArgAction::SetTrue, overrides_with = "interactive")]
	pub no_interactive: bool,
}

impl Default for GlobalArgs {
	fn default() -> Self {
		Self {
			interactive: true,
			no_interactive: false,
		}
	}
}

/// Command-line interface for chronicle.
#[derive(Parser)]
#[command(name = "chronicle", about = "Release management")]
pub struct Cli {
	#[command(flatten)]
	pub global: GlobalArgs,

	#[command(subcommand)]
	pub command: Option<Command>,
}

/// Available subcommands.
#[derive(Subcommand)]
pub enum Command {
	/// Generate output from the current configuration
	Change,
	/// Initialize a new chronicle configuration using the setup wizard
	Init(InitArgs),
}

/// Arguments for the `init` subcommand.
#[derive(Args)]
pub struct InitArgs {
	/// Package manager to use (required in non-interactive mode)
	#[arg(short, long)]
	pub package_manager: Option<PackageManager>,
}

/// Runs the `init` subcommand.
pub fn cmd_init(
	git_workdir: &Path,
	args: &InitArgs,
	global: &GlobalArgs,
) -> anyhow::Result<ExitCode> {
	if config::exists(git_workdir) {
		bail!("Configuration already exists.");
	}

	let config = if global.no_interactive {
		// Non-interactive mode: require all arguments
		let Some(pm) = args.package_manager else {
			bail!("--package-manager is required in non-interactive mode");
		};
		Config {
			package_manager: pm,
		}
	} else {
		// Interactive mode (default): run TUI, skipping steps for pre-filled options
		let options = init::InitOptions {
			package_manager: args.package_manager,
		};
		match init::setup(git_workdir, &options)? {
			Some(config) => config,
			None => return Ok(ExitCode::from(2)),
		}
	};

	let path = config::create(git_workdir, &config)?;
	println!("Created {}", path.display());
	Ok(ExitCode::SUCCESS)
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
