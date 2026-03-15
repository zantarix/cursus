//! Publish command implementation.

mod github_releases;
mod tags;
#[cfg(test)]
mod tests_common;

use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::Args;
use log::{error, info, warn};

use crate::git;
use crate::model::config::Config;
use crate::package_manager::{self, DependencyGraph, PublishOutcome, filter_projects_by_name};
use crate::path::AbsolutePath;

use github_releases::{
	log_dry_run_github_releases, orchestrate_github_releases, run_github_build_command,
};
use tags::create_and_push_tags;

/// Result of attempting to publish a package.
pub(super) enum PublishResult {
	/// Package was successfully published.
	Published,
	/// Package was already published (skipped).
	Skipped,
	/// Publish failed.
	Failed,
}

/// Data about a successfully published package needed for GitHub Release creation.
pub(super) struct PublishedPackage {
	/// Package name.
	pub(super) name: String,
	/// Published version.
	pub(super) version: semver::Version,
	/// Absolute path to the project root.
	pub(super) project_path: AbsolutePath,
}

/// Arguments for the publish subcommand.
#[derive(Args, Default)]
pub struct PublishArgs {
	/// Only publish specific packages (repeatable)
	#[arg(short = 'p', long = "package")]
	pub packages: Vec<String>,
	/// Skip git tag creation, tag pushing, and GitHub Releases even if enabled in config
	#[arg(long)]
	pub no_git: bool,
}

/// Sorts selected projects into dependency-first order using the full project graph.
///
/// Emits cycle warnings for circular dependencies unless disabled in config.
///
/// Returns the sorted project list and the full dependency graph (for use in
/// downstream failure propagation).
fn sort_projects_by_dependency(
	projects: &[crate::package_manager::Project],
	selected_projects: Vec<crate::package_manager::Project>,
	disable_cycle_warnings: bool,
) -> anyhow::Result<(Vec<crate::package_manager::Project>, DependencyGraph)> {
	let graph = package_manager::build_dependency_graph(projects)?;
	if !disable_cycle_warnings {
		let cycle_groups = graph.cycle_groups();
		if !cycle_groups.is_empty() {
			for group in &cycle_groups {
				warn!(
					"circular dependencies detected between: {}",
					group.join(", ")
				);
			}
			warn!(
				"To disable this warning, set `disable_dependency_cycle_warnings = true` \
				 in the [global] section of .cursus/config.toml"
			);
		}
	}
	let all_sorted_names = graph.sort_leaves_first();
	let selected_names_set: std::collections::HashSet<_> =
		selected_projects.iter().map(|p| p.name()).collect();
	let sorted_names: Vec<_> = all_sorted_names
		.into_iter()
		.filter(|name| selected_names_set.contains(name.as_str()))
		.collect();
	let sorted = sorted_names
		.iter()
		.filter_map(|name| selected_projects.iter().find(|p| p.name() == name).cloned())
		.collect();
	Ok((sorted, graph))
}

/// Adds all transitive dependents of `failed_package` to `blocked` using BFS.
///
/// Handles cycles safely via the set membership check — a package is only
/// enqueued once, so cycles in the dependency graph never cause infinite loops.
///
/// # Arguments
///
/// * `graph` - The dependency graph used to look up direct dependents.
/// * `failed_package` - The package whose dependents should be blocked.
/// * `blocked` - Set of blocked package names, mutated in place.
fn add_transitive_dependents(
	graph: &DependencyGraph,
	failed_package: &str,
	blocked: &mut std::collections::HashSet<String>,
) {
	let mut queue = std::collections::VecDeque::new();
	queue.push_back(failed_package.to_string());
	while let Some(pkg) = queue.pop_front() {
		for dependent in graph.direct_dependents(&pkg) {
			if blocked.insert(dependent.clone()) {
				queue.push_back(dependent);
			}
		}
	}
}

/// Creates git tags and GitHub Releases for all published packages.
///
/// Returns `(tags_created, tags_skipped, github_created, github_failed, tags_push_failed)`.
#[allow(clippy::too_many_arguments)]
fn run_git_release_operations(
	git: &git::GitWorkdir,
	config: &Config,
	env: &crate::Env,
	published_packages: &[PublishedPackage],
	dry_run: bool,
	git_enabled: bool,
	no_git: bool,
	is_multi_package: bool,
) -> anyhow::Result<(usize, usize, usize, bool, usize)> {
	let (tags_created, tags_skipped, tags_push_failed) = maybe_create_tags(
		published_packages,
		config,
		git,
		dry_run,
		git_enabled,
		is_multi_package,
	)?;
	let (github_created, github_failed) = maybe_orchestrate_github_releases(
		git,
		config,
		env,
		published_packages,
		dry_run,
		no_git,
		is_multi_package,
	)?;
	Ok((
		tags_created,
		tags_skipped,
		github_created,
		github_failed,
		tags_push_failed,
	))
}

/// Creates git tags for published packages (or logs dry-run intent) and returns counts.
///
/// Returns `(tags_created, tags_skipped, tags_push_failed)`.
fn maybe_create_tags(
	published_packages: &[PublishedPackage],
	config: &Config,
	git: &git::GitWorkdir,
	dry_run: bool,
	git_enabled: bool,
	is_multi_package: bool,
) -> anyhow::Result<(usize, usize, usize)> {
	if !git_enabled {
		return Ok((0, 0, 0));
	}
	if dry_run {
		for pkg in published_packages {
			let tag = config
				.git
				.tag_format
				.tag(&pkg.name, &pkg.version, is_multi_package);
			info!("Would create tag {tag}");
		}
		return Ok((0, 0, 0));
	}
	create_and_push_tags(published_packages, config, git, is_multi_package)
}

/// Orchestrates GitHub Releases when enabled, or logs dry-run intent.
///
/// Returns `(releases_created, any_failed)`.
fn maybe_orchestrate_github_releases(
	git: &git::GitWorkdir,
	config: &Config,
	env: &crate::Env,
	published_packages: &[PublishedPackage],
	dry_run: bool,
	no_git: bool,
	is_multi_package: bool,
) -> anyhow::Result<(usize, bool)> {
	if !config.github.enabled || no_git {
		return Ok((0, false));
	}
	if dry_run {
		log_dry_run_github_releases(published_packages, config, is_multi_package);
		return Ok((0, false));
	}
	let client = match env.github_client() {
		Some(c) => c,
		None => bail!("GitHub client not available despite token being set"),
	};
	orchestrate_github_releases(git, config, client, published_packages, is_multi_package)
}

/// Runs pre-publish GitHub checks: validates token presence and runs the build command.
///
/// Returns `Ok(true)` if the build command failed (caller should return `ExitCode::FAILURE`),
/// `Ok(false)` if checks pass or GitHub is not enabled, or `Err` if no token was found.
fn run_pre_publish_github_checks(
	env: &crate::Env,
	config: &Config,
	git: &git::GitWorkdir,
	no_git: bool,
	dry_run: bool,
) -> anyhow::Result<bool> {
	if !config.github.enabled || no_git {
		return Ok(false);
	}
	if !dry_run && env.github_client().is_none() {
		bail!(
			"GitHub Releases is enabled but no GitHub token found. \
			 Set GH_TOKEN or GITHUB_TOKEN environment variable."
		);
	}
	run_github_build_command(env, config, git)
}

/// Execute the publish command.
pub(crate) fn cmd_publish(
	git: &git::GitWorkdir,
	args: &PublishArgs,
	dry_run: bool,
	config: Config,
) -> anyhow::Result<ExitCode> {
	let env = config.env().context("env not set")?;
	let projects = config.load_projects()?;
	let selected_projects = filter_projects_by_name(&projects, &args.packages)?;
	let (sorted_projects, graph) = sort_projects_by_dependency(
		&projects,
		selected_projects,
		config.global.disable_dependency_cycle_warnings,
	)?;
	if run_pre_publish_github_checks(env, &config, git, args.no_git, dry_run)? {
		return Ok(ExitCode::FAILURE);
	}
	let is_multi_package = projects.len() > 1;
	let (published_packages, skipped_count, dep_skipped_count, publish_failed) =
		publish_projects(&sorted_projects, &graph, dry_run)?;
	let git_enabled = config.git.enabled() && !args.no_git;
	let (tags_created, tags_skipped, github_created, github_failed, tag_push_failed) =
		run_git_release_operations(
			git,
			&config,
			env,
			&published_packages,
			dry_run,
			git_enabled,
			args.no_git,
			is_multi_package,
		)?;
	log_publish_summary(
		&published_packages,
		skipped_count,
		dep_skipped_count,
		dry_run,
		git_enabled,
		tags_created,
		tags_skipped,
		tag_push_failed,
		config.github.enabled,
		args.no_git,
		github_created,
		github_failed,
	);

	let code = if publish_failed || github_failed || tag_push_failed > 0 {
		ExitCode::FAILURE
	} else {
		ExitCode::SUCCESS
	};
	Ok(code)
}

/// Publishes the given projects to their registries, tracking outcomes.
///
/// Projects should be pre-sorted in dependency order (leaves first).
/// Private packages (marked with `private: true` in npm or `publish = false` in Cargo)
/// are silently skipped.
///
/// When a package fails to publish, all of its transitive dependents are skipped
/// (with a warning) to avoid confusing errors from publishing against a registry
/// that is missing a dependency.
///
/// Returns `(published_packages, skipped_count, dep_skipped_count, failed)`.
///
/// # Arguments
///
/// * `projects` - Projects to publish, pre-sorted in dependency order.
/// * `graph` - The dependency graph used to determine which packages to skip on failure.
/// * `dry_run` - If true, only print what would be published without actually publishing.
fn publish_projects(
	projects: &[package_manager::Project],
	graph: &DependencyGraph,
	dry_run: bool,
) -> anyhow::Result<(Vec<PublishedPackage>, usize, usize, bool)> {
	let mut published = Vec::new();
	let mut skipped_count = 0;
	let mut dep_skipped_count = 0;
	let mut failed = false;
	let mut blocked: std::collections::HashSet<String> = std::collections::HashSet::new();

	for project in projects {
		// Skip packages whose dependency failed to publish
		if blocked.contains(project.name()) {
			warn!(
				"Skipping {} because a dependency failed to publish",
				project.name()
			);
			dep_skipped_count += 1;
			continue;
		}

		// Check if the project is publishable (not private)
		let is_publishable = project.is_publishable()?;
		if !is_publishable {
			// Silently skip private packages (per ADR-007). Private packages are not
			// added to `blocked`, because their dependents may still be publishable via
			// other means. If a dependent genuinely requires this package on the registry,
			// its own publish will fail and that failure will correctly block further
			// transitive dependents at that point.
			continue;
		}

		if dry_run {
			// Dry run: just print what would be published
			let version = project.version();
			let registry = project.registry_name();
			info!(
				"Would publish {}@{} to {}",
				project.name(),
				version,
				registry
			);
			published.push(PublishedPackage {
				name: project.name().to_string(),
				version: version.clone(),
				project_path: project.path().clone(),
			});
		} else {
			// Real publish: delegate to do_publish which handles everything
			match do_publish(project) {
				PublishResult::Published => {
					published.push(PublishedPackage {
						name: project.name().to_string(),
						version: project.version().clone(),
						project_path: project.path().clone(),
					});
				}
				PublishResult::Skipped => skipped_count += 1,
				PublishResult::Failed => {
					failed = true;
					add_transitive_dependents(graph, project.name(), &mut blocked);
				}
			}
		}
	}

	Ok((published, skipped_count, dep_skipped_count, failed))
}

/// Logs the first line of the publish summary (published/skipped/GitHub counts).
#[allow(clippy::too_many_arguments)]
fn log_summary_line(
	published_packages: &[PublishedPackage],
	skipped_count: usize,
	dep_skipped_count: usize,
	dry_run: bool,
	git_enabled: bool,
	github_enabled: bool,
	no_git: bool,
	github_created: usize,
	github_failed: bool,
) {
	let dep_skipped_note = if dep_skipped_count > 0 {
		format!(", {dep_skipped_count} skipped (dependency failed)")
	} else {
		String::new()
	};
	if dry_run {
		let tag_note = if git_enabled && !published_packages.is_empty() {
			format!(", {} would be tagged", published_packages.len())
		} else {
			String::new()
		};
		info!(
			"Summary: {} would be published, {} would be skipped{tag_note}",
			published_packages.len(),
			skipped_count
		);
		warn!(
			"Dry-run assumes all packages need publishing and will succeed; actual results may differ if some packages are already published or if publish failures occur"
		);
	} else if github_enabled && !no_git {
		match (github_created, github_failed) {
			(created, false) => info!(
				"Summary: {} published, {} skipped, {} GitHub Releases created{dep_skipped_note}",
				published_packages.len(),
				skipped_count,
				created
			),
			(created, true) => {
				let failed_count = published_packages.len().saturating_sub(created);
				info!(
					"Summary: {} published, {} skipped, {} GitHub Release{} created, {} GitHub Release{} failed{dep_skipped_note}",
					published_packages.len(),
					skipped_count,
					created,
					if created == 1 { "" } else { "s" },
					failed_count,
					if failed_count == 1 { "" } else { "s" },
				);
			}
		}
	} else {
		info!(
			"Summary: {} published, {} skipped{dep_skipped_note}",
			published_packages.len(),
			skipped_count
		);
	}
}

/// Logs the publish summary after all publish operations have completed.
#[allow(clippy::too_many_arguments)]
fn log_publish_summary(
	published_packages: &[PublishedPackage],
	skipped_count: usize,
	dep_skipped_count: usize,
	dry_run: bool,
	git_enabled: bool,
	tags_created: usize,
	tags_skipped: usize,
	tags_push_failed: usize,
	github_enabled: bool,
	no_git: bool,
	github_created: usize,
	github_failed: bool,
) {
	info!("");
	log_summary_line(
		published_packages,
		skipped_count,
		dep_skipped_count,
		dry_run,
		git_enabled,
		github_enabled,
		no_git,
		github_created,
		github_failed,
	);
	if !dry_run && git_enabled && (tags_created > 0 || tags_skipped > 0) {
		info!(
			"{tags_created} tag{} created, {tags_skipped} skipped",
			if tags_created == 1 { "" } else { "s" }
		);
	}
	if !dry_run && git_enabled && tags_push_failed > 0 {
		info!(
			"{tags_push_failed} tag push{} failed; run again to retry",
			if tags_push_failed == 1 { "" } else { "es" }
		);
	}
}

/// Counts publish outcomes for each project, printing per-project results.
///
/// Executes the actual publish operation for a project, handling output and errors.
fn do_publish(project: &package_manager::Project) -> PublishResult {
	let version = project.version();
	let registry = project.registry_name();

	match project.publish() {
		Ok(PublishOutcome::Published) => {
			info!("Published {}@{} to {}", project.name(), version, registry);
			PublishResult::Published
		}
		Ok(PublishOutcome::AlreadyPublished) => {
			info!(
				"Skipped {}@{} (already published to {})",
				project.name(),
				version,
				registry
			);
			PublishResult::Skipped
		}
		Err(e) => {
			error!("Failed to publish {}@{}: {}", project.name(), version, e);
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
		assert!(args.packages.is_empty());
		assert!(!args.no_git);
	}

	fn make_graph(edges: &[(&str, &[&str])]) -> DependencyGraph {
		let adjacency = edges
			.iter()
			.map(|(k, vs)| (k.to_string(), vs.iter().map(|v| v.to_string()).collect()))
			.collect();
		DependencyGraph::from_adjacency(adjacency)
	}

	#[test]
	fn add_transitive_dependents_linear_chain() {
		// c -> b -> a: if c fails, b and a should be blocked
		let graph = make_graph(&[("a", &["b"]), ("b", &["c"]), ("c", &[])]);
		let mut blocked = std::collections::HashSet::new();
		add_transitive_dependents(&graph, "c", &mut blocked);
		assert!(blocked.contains("b"), "b depends on c");
		assert!(blocked.contains("a"), "a depends on b (transitive)");
		assert!(
			!blocked.contains("c"),
			"failed package itself not in blocked"
		);
	}

	#[test]
	fn add_transitive_dependents_diamond() {
		// d <- b <- a, d <- c <- a: if d fails, b, c and a should be blocked
		let graph = make_graph(&[("a", &["b", "c"]), ("b", &["d"]), ("c", &["d"]), ("d", &[])]);
		let mut blocked = std::collections::HashSet::new();
		add_transitive_dependents(&graph, "d", &mut blocked);
		assert!(blocked.contains("b"));
		assert!(blocked.contains("c"));
		assert!(blocked.contains("a"));
		assert!(!blocked.contains("d"));
	}

	#[test]
	fn add_transitive_dependents_cycle_terminates() {
		// a <-> b: if a fails, b should be blocked; cycle must not loop infinitely
		let graph = make_graph(&[("a", &["b"]), ("b", &["a"])]);
		let mut blocked = std::collections::HashSet::new();
		add_transitive_dependents(&graph, "a", &mut blocked);
		assert!(blocked.contains("b"));
		// Must terminate (would panic/hang otherwise)
	}

	#[test]
	fn add_transitive_dependents_independent_subtree_not_blocked() {
		// a -> b, c -> d: if b fails only a is blocked; c and d are unaffected
		let graph = make_graph(&[("a", &["b"]), ("b", &[]), ("c", &["d"]), ("d", &[])]);
		let mut blocked = std::collections::HashSet::new();
		add_transitive_dependents(&graph, "b", &mut blocked);
		assert!(blocked.contains("a"));
		assert!(!blocked.contains("c"));
		assert!(!blocked.contains("d"));
	}

	#[test]
	fn add_transitive_dependents_with_prepopulated_blocked_set() {
		// a -> b -> c: if blocked already contains "a" and we add dependents of b,
		// "a" should not be re-enqueued (returns false from insert) and the BFS still terminates.
		let graph = make_graph(&[("a", &["b"]), ("b", &["c"]), ("c", &[])]);
		let mut blocked = std::collections::HashSet::new();
		blocked.insert("a".to_string()); // pre-populated
		add_transitive_dependents(&graph, "c", &mut blocked);
		// b should be added (depends on c)
		assert!(blocked.contains("b"));
		// a was already present and should still be there
		assert!(blocked.contains("a"));
		// The already-present "a" entry must not cause re-enqueuing (BFS terminates)
	}
}
