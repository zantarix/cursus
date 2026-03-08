//! The `change` subcommand.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::bail;
use clap::Args;

use crate::command::CommandRunner;
use crate::model::changeset::{self, ChangeType, Changeset};
use crate::model::config;
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
	env: &crate::Env,
	runner: Arc<dyn CommandRunner>,
) -> anyhow::Result<ExitCode> {
	let config = config::load(git_workdir)?;
	let projects = config.load_projects(Arc::clone(&runner))?;

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

	let changeset = Changeset::new(packages, args.message.clone());

	let path = changeset.write(config.git_workdir())?;

	if args.message.is_none() {
		changeset::open_editor(&path, env, runner.as_ref())?;
	}

	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_change_args() {
		let args = ChangeArgs::default();
		assert!(args.change_type.is_none());
		assert!(args.projects.is_empty());
		assert!(args.message.is_none());
	}
}
