//! Publish command implementation.

use std::process::ExitCode;
use std::sync::Arc;

use clap::Args;

use crate::command::CommandRunner;
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
pub fn cmd_publish(
	git_workdir: &std::path::Path,
	args: &PublishArgs,
	runner: Arc<dyn CommandRunner>,
) -> anyhow::Result<ExitCode> {
	// Load configuration and enumerate projects
	let config = config::load(git_workdir)?;
	let projects = config.load_projects(runner)?;

	// Filter projects by --package flags if specified
	let selected_projects = filter_projects_by_name(&projects, &args.packages)?;

	// Build dependency graph from all projects (not just selected ones)
	// We need the full graph to correctly order the selected subset
	let graph = package_manager::build_dependency_graph(&projects)?;

	// Emit cycle warnings if cycles exist and warnings are not disabled
	if !config.global.disable_dependency_cycle_warnings {
		let cycle_groups = graph.cycle_groups();
		if !cycle_groups.is_empty() {
			for group in &cycle_groups {
				eprintln!(
					"Warning: circular dependencies detected between: {}",
					group.join(", ")
				);
			}
			eprintln!(
				"To disable this warning, set `disable_dependency_cycle_warnings = true` in the [global] section of .chronicle/config.toml"
			);
			eprintln!();
		}
	}

	// Sort all projects in leaves-first order (dependencies before dependents)
	let all_sorted_names = graph.sort_leaves_first();

	// Filter to only include selected projects, maintaining sorted order
	let selected_names_set: std::collections::HashSet<_> =
		selected_projects.iter().map(|p| p.name()).collect();
	let sorted_names: Vec<_> = all_sorted_names
		.into_iter()
		.filter(|name| selected_names_set.contains(name.as_str()))
		.collect();

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
	let (published_count, skipped_count, failed) = collect_outcomes(projects, dry_run)?;

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

/// Counts publish outcomes for each project, printing per-project results.
///
/// Returns `(published, skipped, failed)`. In dry-run mode, prints what would be
/// published without calling the registry. In live mode, delegates to `do_publish`.
fn collect_outcomes(
	projects: &[package_manager::Project],
	dry_run: bool,
) -> anyhow::Result<(usize, usize, bool)> {
	let mut published = 0usize;
	let mut skipped = 0usize;
	let mut failed = false;

	for project in projects {
		if !project.is_publishable()? {
			continue;
		}

		if dry_run {
			let version = project.version();
			let registry = project.registry_name();
			println!(
				"Would publish {}@{} to {}",
				project.name(),
				version,
				registry
			);
			published += 1;
		} else {
			match do_publish(project) {
				PublishResult::Published => published += 1,
				PublishResult::Skipped => skipped += 1,
				PublishResult::Failed => failed = true,
			}
		}
	}

	Ok((published, skipped, failed))
}

/// Executes the actual publish operation for a project, handling output and errors.
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
	use std::sync::Arc;

	use crate::command::test_support::RecordingCommandRunner;
	use crate::package_manager::Project;

	use super::*;

	#[test]
	fn default_publish_args() {
		let args = PublishArgs::default();
		assert!(!args.dry_run);
		assert!(args.packages.is_empty());
	}

	#[test]
	fn collect_outcomes_dry_run_counts_published() {
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let project = Project::new_test_with_runner("pkg-a", "packages/pkg-a", Arc::clone(&runner));
		let (published, skipped, failed) =
			collect_outcomes(&[project], true).expect("collect_outcomes failed");
		assert_eq!(published, 1);
		assert_eq!(skipped, 0);
		assert!(!failed);
	}

	#[test]
	fn collect_outcomes_non_dry_run_published_increments() {
		// RecordingCommandRunner exit_code=0 → NpmAdapter::publish returns Published
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let project = Project::new_test_with_runner("pkg-a", "packages/pkg-a", Arc::clone(&runner));
		let (published, skipped, failed) =
			collect_outcomes(&[project], false).expect("collect_outcomes failed");
		assert_eq!(published, 1);
		assert_eq!(skipped, 0);
		assert!(!failed);
	}

	#[test]
	fn collect_outcomes_non_dry_run_skipped_increments() {
		// exit_code=1 with "EPUBLISHCONFLICT" → AlreadyPublished → Skipped
		let runner = Arc::new(
			RecordingCommandRunner::new(1).with_stderr(b"npm ERR! code EPUBLISHCONFLICT".to_vec()),
		);
		let project = Project::new_test_with_runner("pkg-a", "packages/pkg-a", Arc::clone(&runner));
		let (published, skipped, failed) =
			collect_outcomes(&[project], false).expect("collect_outcomes failed");
		assert_eq!(published, 0);
		assert_eq!(skipped, 1);
		assert!(!failed);
	}

	#[test]
	fn collect_outcomes_non_dry_run_failed_sets_flag() {
		// exit_code=1 with unrecognised stderr → Failed
		let runner =
			Arc::new(RecordingCommandRunner::new(1).with_stderr(b"network error".to_vec()));
		let project = Project::new_test_with_runner("pkg-a", "packages/pkg-a", Arc::clone(&runner));
		let (published, skipped, failed) =
			collect_outcomes(&[project], false).expect("collect_outcomes failed");
		assert_eq!(published, 0);
		assert_eq!(skipped, 0);
		assert!(failed);
	}
}
