//! Publish command implementation.

use std::process::ExitCode;

use clap::Args;

use crate::model::config;
use crate::package_manager::{self, PublishOutcome, filter_projects_by_name};

/// Arguments for the publish subcommand.
#[derive(Args, Default)]
pub struct PublishArgs {
	/// Preview without publishing
	#[arg(long)]
	pub dry_run: bool,
	/// Only publish specific packages (repeatable)
	#[arg(short = 'p', long = "package")]
	pub packages: Vec<String>,
}

/// Execute the publish command.
pub fn cmd_publish(args: &PublishArgs, git_workdir: &std::path::Path) -> anyhow::Result<ExitCode> {
	// Load configuration and enumerate projects
	let config = config::load(git_workdir)?;
	let projects = config.load_projects()?;

	// Filter projects by --package flags if specified
	let selected_projects = filter_projects_by_name(&projects, &args.packages)?;

	// Build dependency graph from all projects (not just selected ones)
	// We need the full graph to correctly order the selected subset
	let graph = package_manager::build_dependency_graph(&projects)?;

	// Sort selected projects in leaves-first order (dependencies before dependents)
	let selected_names: Vec<String> = selected_projects
		.iter()
		.map(|p| p.name().to_string())
		.collect();
	let sorted_names = graph.sort_leaves_first(&selected_names)?;

	// Reorder selected_projects to match sorted_names
	let mut sorted_projects = Vec::new();
	for name in &sorted_names {
		if let Some(project) = selected_projects.iter().find(|p| p.name() == name) {
			sorted_projects.push(project.clone());
		}
	}

	// Dry run: just print what would be published and exit
	if args.dry_run {
		for project in &sorted_projects {
			let version = project.read_version()?;
			let registry = project.registry_name();
			println!(
				"Would publish {}@{} to {}",
				project.name(),
				version,
				registry
			);
		}
		return Ok(ExitCode::SUCCESS);
	}

	publish_projects(&sorted_projects)
}

/// Publishes the given projects to their registries, tracking outcomes.
///
/// Projects should be pre-sorted in dependency order (leaves first).
#[coverage(off)]
fn publish_projects(projects: &[package_manager::Project]) -> anyhow::Result<ExitCode> {
	let mut published_count = 0;
	let mut skipped_count = 0;
	let mut failed = false;

	for project in projects {
		let version = project.read_version()?;
		let registry = project.registry_name();

		match project.publish() {
			Ok(PublishOutcome::Published) => {
				println!("Published {}@{} to {}", project.name(), version, registry);
				published_count += 1;
			}
			Ok(PublishOutcome::AlreadyPublished) => {
				println!(
					"Skipped {}@{} (already published to {})",
					project.name(),
					version,
					registry
				);
				skipped_count += 1;
			}
			Err(e) => {
				eprintln!("Failed to publish {}@{}: {}", project.name(), version, e);
				failed = true;
			}
		}
	}

	println!();
	println!(
		"Summary: {} published, {} skipped",
		published_count, skipped_count
	);

	if failed {
		Ok(ExitCode::FAILURE)
	} else {
		Ok(ExitCode::SUCCESS)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_publish_args() {
		let args = PublishArgs::default();
		assert!(!args.dry_run);
		assert!(args.packages.is_empty());
	}
}
