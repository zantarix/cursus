//! Publish command implementation.

use std::process::ExitCode;

use clap::Args;

use crate::model::config;
use crate::package_manager::{self, PublishOutcome, filter_projects_by_name};

/// Result of attempting to publish a package.
enum PublishResult {
	/// Package was successfully published.
	Published,
	/// Package was already published (skipped).
	Skipped,
	/// Publish failed.
	Failed,
}

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

	publish_projects(&sorted_projects, args.dry_run)
}

/// Publishes the given projects to their registries, tracking outcomes.
///
/// Projects should be pre-sorted in dependency order (leaves first).
/// Private packages (marked with `private: true` in npm or `publish = false` in Cargo)
/// are silently skipped.
///
/// # Arguments
///
/// * `projects` - Projects to publish, pre-sorted in dependency order.
/// * `dry_run` - If true, only print what would be published without actually publishing.
fn publish_projects(
	projects: &[package_manager::Project],
	dry_run: bool,
) -> anyhow::Result<ExitCode> {
	let mut published_count = 0;
	let mut skipped_count = 0;
	let mut failed = false;

	for project in projects {
		// Check if the project is publishable (not private)
		let is_publishable = project.is_publishable()?;
		if !is_publishable {
			// Silently skip private packages
			continue;
		}

		if dry_run {
			// Dry run: just print what would be published
			let version = project.version();
			let registry = project.registry_name();
			println!(
				"Would publish {}@{} to {}",
				project.name(),
				version,
				registry
			);
		} else {
			// Real publish: delegate to do_publish which handles everything
			match do_publish(project) {
				PublishResult::Published => published_count += 1,
				PublishResult::Skipped => skipped_count += 1,
				PublishResult::Failed => failed = true,
			}
		}
	}

	println!();
	if dry_run {
		println!(
			"Summary: {} would be published, {} would be skipped",
			published_count, skipped_count
		);
	} else {
		println!(
			"Summary: {} published, {} skipped",
			published_count, skipped_count
		);
	}

	if failed {
		Ok(ExitCode::FAILURE)
	} else {
		Ok(ExitCode::SUCCESS)
	}
}

/// Executes the actual publish operation for a project, handling output and errors.
///
/// This is marked with `#[coverage(off)]` because it shells out to package managers.
#[coverage(off)]
fn do_publish(project: &package_manager::Project) -> PublishResult {
	let version = project.version();
	let registry = project.registry_name();

	match project.publish() {
		Ok(PublishOutcome::Published) => {
			println!("Published {}@{} to {}", project.name(), version, registry);
			PublishResult::Published
		}
		Ok(PublishOutcome::AlreadyPublished) => {
			println!(
				"Skipped {}@{} (already published to {})",
				project.name(),
				version,
				registry
			);
			PublishResult::Skipped
		}
		Err(e) => {
			eprintln!("Failed to publish {}@{}: {}", project.name(), version, e);
			PublishResult::Failed
		}
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
