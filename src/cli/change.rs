//! The `change` subcommand.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::bail;
use clap::Args;

use crate::config::{self, PackageManager};
use crate::package_manager::{self, CargoAdapter, NpmAdapter, PackageManagerAdapter};
use crate::tui::change;

use super::GlobalArgs;

/// Arguments for the `change` subcommand.
#[derive(Args, Default)]
pub struct ChangeArgs {
	/// Type of change: major, minor, or patch (required in non-interactive mode)
	#[arg(short = 't', long)]
	pub change_type: Option<change::ChangeType>,
}

/// Runs the `change` subcommand.
pub fn cmd_change(
	git_workdir: &Path,
	args: &ChangeArgs,
	global: &GlobalArgs,
) -> anyhow::Result<ExitCode> {
	if !config::exists(git_workdir) {
		bail!("No configuration found. Run 'chronicle init' to create one.");
	}
	let config = config::load(git_workdir)?;

	let adapters: Vec<Arc<dyn PackageManagerAdapter>> = config
		.enabled_package_managers()
		.map(|pm| -> Arc<dyn PackageManagerAdapter> {
			match pm {
				PackageManager::Npm => Arc::new(NpmAdapter::new(config.npm.clone())),
				PackageManager::Cargo => Arc::new(CargoAdapter::new(config.cargo.clone())),
			}
		})
		.collect();

	let projects = package_manager::enumerate_projects(adapters, git_workdir)?;

	if projects.is_empty() {
		bail!("No projects found. Check that your package manager configuration is correct.");
	}

	for project in &projects {
		println!("{}: {}", project.name(), project.path().display());
	}

	let change_type = if global.no_interactive {
		let Some(ct) = args.change_type else {
			bail!("--change-type is required in non-interactive mode");
		};
		ct
	} else {
		let options = change::ChangeOptions {
			change_type: args.change_type,
		};
		match change::run(&options)? {
			Some(ct) => ct,
			None => return Ok(ExitCode::from(2)),
		}
	};

	println!("{change_type:?}");
	Ok(ExitCode::SUCCESS)
}
