//! The `change` subcommand.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use anyhow::bail;
use clap::Args;

use crate::changeset::{self, ChangeType, Changeset};
use crate::config;
use crate::tui::change;

use super::GlobalArgs;

/// Arguments for the `change` subcommand.
#[derive(Args, Default)]
pub struct ChangeArgs {
	/// Type of change: major, minor, or patch (required in non-interactive mode)
	#[arg(short = 't', long)]
	pub change_type: Option<ChangeType>,

	/// Project name(s) to include (repeatable; defaults to all in non-interactive mode)
	#[arg(short = 'p', long = "project")]
	pub projects: Vec<String>,

	/// Description message for the changeset (required in non-interactive mode)
	#[arg(short = 'm', long)]
	pub message: Option<String>,
}

/// Runs the `change` subcommand.
pub fn cmd_change(
	git_workdir: &Path,
	args: &ChangeArgs,
	global: &GlobalArgs,
) -> anyhow::Result<ExitCode> {
	let (_config, projects) = config::load_projects(git_workdir)?;

	if projects.is_empty() {
		bail!("No projects found. Check that your package manager configuration is correct.");
	}

	let project_indices = if !args.projects.is_empty() {
		let indices: Vec<usize> = args
			.projects
			.iter()
			.map(|name| {
				projects
					.iter()
					.position(|p| p.name() == name)
					.ok_or_else(|| anyhow::anyhow!("Unknown project: {name}"))
			})
			.collect::<anyhow::Result<Vec<_>>>()?;
		Some(indices)
	} else {
		None
	};

	let result = if global.no_interactive {
		let Some(ct) = args.change_type else {
			bail!("--change-type is required in non-interactive mode");
		};
		if args.message.is_none() {
			bail!("--message is required in non-interactive mode");
		}
		let selected_projects = match &project_indices {
			Some(indices) => indices.iter().map(|&i| projects[i].clone()).collect(),
			None => projects.clone(),
		};
		change::ChangeResult {
			projects: selected_projects,
			change_type: ct,
		}
	} else {
		let options = change::ChangeOptions {
			change_type: args.change_type,
			projects: project_indices,
		};
		match change::run(&projects, &options)? {
			Some(r) => r,
			None => return Ok(ExitCode::from(2)),
		}
	};

	let packages: BTreeMap<String, ChangeType> = result
		.projects
		.iter()
		.map(|p| (p.name().to_string(), result.change_type))
		.collect();

	let changeset = Changeset {
		packages,
		message: args.message.clone(),
	};

	let path = changeset::write_changeset(git_workdir, &changeset)?;

	if !global.no_interactive && args.message.is_none() {
		changeset::open_editor(&path)?;
	}

	Ok(ExitCode::SUCCESS)
}
