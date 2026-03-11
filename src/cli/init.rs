//! The `init` subcommand.

use std::process::ExitCode;

use anyhow::bail;
use clap::Args;
use log::info;

use crate::model::config::{CargoConfig, Config, NpmConfig, PackageManager, exists};
use crate::path::AbsolutePath;
use crate::tui::init;

use super::GlobalArgs;

/// Arguments for the `init` subcommand.
#[derive(Args)]
pub struct InitArgs {
	/// Package manager to use (required in non-interactive mode)
	#[arg(short, long)]
	pub package_manager: Option<PackageManager>,
}

/// Runs the `init` subcommand.
pub(crate) fn cmd_init(
	git_workdir: &AbsolutePath,
	args: &InitArgs,
	global: &GlobalArgs,
) -> anyhow::Result<ExitCode> {
	if exists(git_workdir) {
		bail!("Configuration already exists.");
	}

	let config = if global.no_interactive {
		// Non-interactive mode: require all arguments
		let Some(pm) = args.package_manager else {
			bail!("--package-manager is required in non-interactive mode");
		};
		match pm {
			PackageManager::Npm => Config::new(git_workdir).with_npm(NpmConfig::enabled()),
			PackageManager::Cargo => Config::new(git_workdir).with_cargo(CargoConfig::enabled()),
		}
	} else {
		// Interactive mode (default): run TUI, skipping steps for pre-filled options
		let options = init::InitOptions {
			package_manager: args.package_manager,
		};
		match init::run(git_workdir, &options)? {
			Some(config) => config,
			None => return Ok(ExitCode::from(2)),
		}
	};

	let path = config.save()?;
	info!("Created {}", path.display());
	Ok(ExitCode::SUCCESS)
}
