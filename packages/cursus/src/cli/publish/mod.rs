//! Publish command implementation.

mod forge_releases;
mod tags;
#[cfg(test)]
mod tests_common;

use std::process::ExitCode;

use anyhow::bail;
use clap::Args;
use log::{error, info, warn};

use crate::git::Git;
use crate::model::config::Config;
use crate::package_manager::{self, DependencyGraph, PublishOutcome, filter_projects_by_name};
use crate::path::AbsolutePath;

use forge_releases::{
	log_dry_run_forge_releases, orchestrate_forge_releases, run_forge_build_command,
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

/// Feature flags governing publish behavior.
#[derive(Debug)]
pub(crate) struct PublishFlags {
	pub(crate) dry_run: bool,
	pub(crate) git_enabled: bool,
	pub(crate) forge_enabled: bool,
	pub(crate) no_git: bool,
	pub(crate) is_multi_package: bool,
}

/// Outcome of git tag and GitHub Release operations.
#[derive(Debug)]
pub(crate) struct GitReleaseOutcome {
	pub(crate) tags_created: usize,
	pub(crate) tags_skipped: usize,
	pub(crate) tags_push_failed: usize,
	pub(crate) releases_created: usize,
	/// Releases skipped because a published release already exists for the tag.
	pub(crate) releases_already_present: usize,
	pub(crate) forge_failed: bool,
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
async fn sort_projects_by_dependency(
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
pub(crate) fn add_transitive_dependents(
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
/// Returns a [`GitReleaseOutcome`] with tag and GitHub Release counts.
async fn run_git_release_operations(
	git: &dyn Git,
	config: &Config,
	env: &crate::Env,
	published_packages: &[PublishedPackage],
	flags: &PublishFlags,
) -> anyhow::Result<GitReleaseOutcome> {
	let (tags_created, tags_skipped, tags_push_failed) = maybe_create_tags(
		published_packages,
		config,
		git,
		flags.dry_run,
		flags.git_enabled,
		flags.is_multi_package,
	)
	.await?;
	let (releases_created, releases_already_present, forge_failed) =
		maybe_orchestrate_forge_releases(
			git,
			config,
			env,
			published_packages,
			flags.dry_run,
			flags.no_git,
			flags.is_multi_package,
		)
		.await?;
	Ok(GitReleaseOutcome {
		tags_created,
		tags_skipped,
		tags_push_failed,
		releases_created,
		releases_already_present,
		forge_failed,
	})
}

/// Creates git tags for published packages (or logs dry-run intent) and returns counts.
///
/// Returns `(tags_created, tags_skipped, tags_push_failed)`.
async fn maybe_create_tags(
	published_packages: &[PublishedPackage],
	config: &Config,
	git: &dyn Git,
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
	create_and_push_tags(published_packages, config, git, is_multi_package).await
}

/// Orchestrates GitHub Releases when enabled, or logs dry-run intent.
///
/// Returns `(releases_created, releases_already_present, any_failed)`.
async fn maybe_orchestrate_forge_releases(
	git: &dyn Git,
	config: &Config,
	env: &crate::Env,
	published_packages: &[PublishedPackage],
	dry_run: bool,
	no_git: bool,
	is_multi_package: bool,
) -> anyhow::Result<(usize, usize, bool)> {
	if !config.forge_enabled() || no_git {
		return Ok((0, 0, false));
	}
	if dry_run {
		log_dry_run_forge_releases(
			published_packages,
			config,
			is_multi_package,
			env.code_forge_name(),
		);
		return Ok((0, 0, false));
	}
	let client = env
		.code_forge_client()
		.map_err(|reason| anyhow::anyhow!("Forge client not available: {reason}"))?;
	orchestrate_forge_releases(
		git,
		config,
		client,
		published_packages,
		is_multi_package,
		env.fs(),
	)
	.await
}

/// Runs pre-publish GitHub checks: validates token presence and runs the build command.
///
/// Returns `Ok(true)` if the build command failed (caller should return `ExitCode::FAILURE`),
/// `Ok(false)` if checks pass or GitHub is not enabled, or `Err` if no token was found.
async fn run_pre_publish_forge_checks(
	env: &crate::Env,
	config: &Config,
	git: &dyn Git,
	no_git: bool,
	dry_run: bool,
) -> anyhow::Result<bool> {
	if !config.forge_enabled() || no_git {
		return Ok(false);
	}
	if !dry_run && let Err(reason) = env.code_forge_client() {
		bail!("Forge releases are enabled but the code forge client is unavailable: {reason}");
	}
	run_forge_build_command(env, config, git).await
}

/// Execute the publish command.
pub(crate) async fn cmd_publish(
	args: &PublishArgs,
	dry_run: bool,
	env: &crate::Env,
	config: Config,
) -> anyhow::Result<ExitCode> {
	let git = env.git();
	let projects = config.load_projects(env).await?;
	let selected_projects = filter_projects_by_name(&projects, &args.packages)?;
	let (sorted_projects, graph) = sort_projects_by_dependency(
		&projects,
		selected_projects,
		config.global.disable_dependency_cycle_warnings,
	)
	.await?;
	if run_pre_publish_forge_checks(env, &config, git, args.no_git, dry_run).await? {
		return Ok(ExitCode::FAILURE);
	}
	let flags = PublishFlags {
		dry_run,
		git_enabled: config.git.enabled() && !args.no_git,
		forge_enabled: config.forge_enabled(),
		no_git: args.no_git,
		is_multi_package: projects.len() > 1,
	};
	let publish = publish_projects(&sorted_projects, &graph, dry_run, env.fs(), &config).await?;
	let outcome = run_git_release_operations(git, &config, env, &publish.published, &flags).await?;
	log_publish_summary(&publish, &flags, &outcome);

	let code = if publish.failed || outcome.forge_failed || outcome.tags_push_failed > 0 {
		ExitCode::FAILURE
	} else {
		ExitCode::SUCCESS
	};
	Ok(code)
}

/// Mutable state accumulated while iterating over projects in `publish_projects`.
pub(crate) struct PublishState {
	pub(crate) published: Vec<PublishedPackage>,
	pub(crate) skipped_count: usize,
	pub(crate) dep_skipped_count: usize,
	pub(crate) unprepared_count: usize,
	pub(crate) private_tagged_count: usize,
	pub(crate) failed: bool,
	pub(crate) blocked: std::collections::HashSet<String>,
}

impl PublishState {
	pub(crate) fn new() -> Self {
		Self {
			published: Vec::new(),
			skipped_count: 0,
			dep_skipped_count: 0,
			unprepared_count: 0,
			private_tagged_count: 0,
			failed: false,
			blocked: std::collections::HashSet::new(),
		}
	}

	/// Adds a private package to the published list for tag-only publishing (ADR-043).
	///
	/// The package is added to `published` so that `run_git_release_operations` creates
	/// its tag and GitHub Release. No registry publish is performed.
	///
	/// Dry-run tag logging is handled by `maybe_create_tags`, which uses the configured
	/// tag format and iterates all `published` packages uniformly.
	pub(crate) fn record_private_tagged(&mut self, project: &package_manager::Project) {
		self.published.push(PublishedPackage {
			name: project.name().to_string(),
			version: project.version().clone(),
			project_path: project.path().clone(),
		});
		self.private_tagged_count += 1;
	}

	/// Records the outcome of publishing a single project.
	///
	/// In dry-run mode, logs what would be published and pushes to `published`.
	/// In real mode, both `Published` and `Skipped` (already-on-registry) outcomes push onto
	/// `published` so that downstream tag and GitHub Release stages can recover from prior partial
	/// failures (ADR-055). Only `Failed` skips the package from `published`.
	pub(crate) async fn record_outcome(
		&mut self,
		project: &package_manager::Project,
		graph: &DependencyGraph,
		dry_run: bool,
	) {
		if dry_run {
			let version = project.version();
			let registry = project.registry_name().await;
			info!(
				"Would publish {}@{} to {}",
				project.name(),
				version,
				registry
			);
			self.published.push(PublishedPackage {
				name: project.name().to_string(),
				version: version.clone(),
				project_path: project.path().clone(),
			});
		} else {
			match do_publish(project).await {
				PublishResult::Published => self.published.push(PublishedPackage {
					name: project.name().to_string(),
					version: project.version().clone(),
					project_path: project.path().clone(),
				}),
				// Package version is already on the registry (published in a prior run).
				// Still add it to `published` so tag and GitHub Release creation are
				// retried — both are already idempotent and handle the pre-existing state.
				PublishResult::Skipped => {
					self.published.push(PublishedPackage {
						name: project.name().to_string(),
						version: project.version().clone(),
						project_path: project.path().clone(),
					});
					self.skipped_count += 1;
				}
				PublishResult::Failed => {
					self.failed = true;
					add_transitive_dependents(graph, project.name(), &mut self.blocked);
				}
			}
		}
	}
}

/// Publishes the given projects to their registries, tracking outcomes.
///
/// Projects should be pre-sorted in dependency order (leaves first).
/// Private packages (marked with `private: true` in npm or `publish = false` in Cargo)
/// are silently skipped unless listed in `[git].publish_private_packages`, in which case they
/// receive git tags and GitHub Releases but are not published to any registry.
/// Releasable packages without a `CHANGELOG.md` (never prepared) are warned about and skipped
/// without blocking their dependents.
///
/// When a package fails to publish, all of its transitive dependents are skipped
/// (with a warning) to avoid confusing errors from publishing against a registry
/// that is missing a dependency.
///
/// # Arguments
///
/// * `projects` - Projects to publish, pre-sorted in dependency order.
/// * `graph` - The dependency graph used to determine which packages to skip on failure.
/// * `dry_run` - If true, only print what would be published without actually publishing.
/// * `fs` - Filesystem abstraction used to check for `CHANGELOG.md`.
/// * `config` - Repository configuration, used to determine releasable projects via
///   [`package_manager::Project::is_releasable_under`].
async fn publish_projects(
	projects: &[package_manager::Project],
	graph: &DependencyGraph,
	dry_run: bool,
	fs: &dyn crate::filesystem::Filesystem,
	config: &Config,
) -> anyhow::Result<PublishState> {
	let mut state = PublishState::new();

	for project in projects {
		if state.blocked.contains(project.name()) {
			warn!(
				"Skipping {} because a dependency failed to publish",
				project.name()
			);
			state.dep_skipped_count += 1;
			continue;
		}
		// Silently skip projects that are neither publishable nor listed for tag-only release.
		if !project.is_releasable_under(config) {
			continue;
		}
		// Skip projects that have never been prepared (no CHANGELOG.md).
		// Not added to `blocked` — dependents may be independently publishable.
		if !project.is_prepared_for_release(fs).await? {
			warn!(
				"Skipping {}: no CHANGELOG.md found (run 'cursus prepare' first, with an appropriate changeset)",
				project.name()
			);
			state.unprepared_count += 1;
			continue;
		}
		if project.is_publishable() {
			state.record_outcome(project, graph, dry_run).await;
		} else {
			// Non-publishable but releasable: tag-only release (ADR-043).
			state.record_private_tagged(project);
		}
	}

	Ok(state)
}

/// Logs the GitHub Releases portion of the publish summary.
///
/// `registry_published` excludes private-tagged and registry-skipped packages.
/// `skipped_count` covers packages already on the registry (whose releases are still
/// attempted). `failed_count` is derived from the combined total minus created and
/// already-present releases.
pub(crate) fn log_forge_releases_summary(
	registry_published: usize,
	private_tagged_count: usize,
	skipped_count: usize,
	suffix_note: &str,
	releases_created: usize,
	releases_already_present: usize,
	forge_failed: bool,
) {
	let private_note = if private_tagged_count > 0 {
		format!(", {private_tagged_count} private (tag only)")
	} else {
		String::new()
	};
	let already_note = if releases_already_present > 0 {
		format!(", {releases_already_present} already present")
	} else {
		String::new()
	};
	// GitHub Releases are attempted for registry-published, registry-skipped, and
	// private-tagged packages — the full `state.published` slice.
	let total_releasable = registry_published + private_tagged_count + skipped_count;
	match (releases_created, forge_failed) {
		(created, false) => info!(
			"Summary: {registry_published} published{private_note}, {skipped_count} skipped, \
			 {created} GitHub Release{} created{already_note}{suffix_note}",
			if created == 1 { "" } else { "s" },
		),
		(created, true) => {
			let failed_count = total_releasable.saturating_sub(created + releases_already_present);
			info!(
				"Summary: {registry_published} published{private_note}, {skipped_count} skipped, \
				 {created} GitHub Release{} created{already_note}, {failed_count} GitHub Release{} \
				 failed{suffix_note}",
				if created == 1 { "" } else { "s" },
				if failed_count == 1 { "" } else { "s" },
			);
		}
	}
}

/// Logs the first line of the publish summary (published/skipped/GitHub counts).
pub(crate) fn log_summary_line(
	state: &PublishState,
	flags: &PublishFlags,
	outcome: &GitReleaseOutcome,
) {
	let dep_skipped_note = if state.dep_skipped_count > 0 {
		format!(", {} skipped (dependency failed)", state.dep_skipped_count)
	} else {
		String::new()
	};
	let unprepared_note = if state.unprepared_count > 0 {
		format!(", {} skipped (not yet prepared)", state.unprepared_count)
	} else {
		String::new()
	};
	let private_note = if state.private_tagged_count > 0 {
		format!(", {} private (tag only)", state.private_tagged_count)
	} else {
		String::new()
	};
	let registry_published =
		state.published.len() - state.private_tagged_count - state.skipped_count;
	if flags.dry_run {
		let tag_note = if flags.git_enabled && !state.published.is_empty() {
			format!(", {} would be tagged", state.published.len())
		} else {
			String::new()
		};
		info!(
			"Summary: {registry_published} would be published{private_note}, {} would be skipped{tag_note}{unprepared_note}",
			state.skipped_count
		);
		warn!(
			"Dry-run assumes all packages need publishing and will succeed; actual results may \
			 differ if some packages are already published or if publish failures occur"
		);
	} else if flags.forge_enabled && !flags.no_git {
		let suffix_note = format!("{dep_skipped_note}{unprepared_note}");
		log_forge_releases_summary(
			registry_published,
			state.private_tagged_count,
			state.skipped_count,
			&suffix_note,
			outcome.releases_created,
			outcome.releases_already_present,
			outcome.forge_failed,
		);
	} else {
		info!(
			"Summary: {registry_published} published{private_note}, {} skipped{dep_skipped_note}{unprepared_note}",
			state.skipped_count
		);
	}
}

/// Logs the publish summary after all publish operations have completed.
pub(crate) fn log_publish_summary(
	state: &PublishState,
	flags: &PublishFlags,
	outcome: &GitReleaseOutcome,
) {
	info!("");
	log_summary_line(state, flags, outcome);
	if !flags.dry_run && flags.git_enabled && (outcome.tags_created > 0 || outcome.tags_skipped > 0)
	{
		info!(
			"{} tag{} created, {} skipped",
			outcome.tags_created,
			if outcome.tags_created == 1 { "" } else { "s" },
			outcome.tags_skipped
		);
	}
	if !flags.dry_run && flags.git_enabled && outcome.tags_push_failed > 0 {
		info!(
			"{} tag push{} failed; run again to retry",
			outcome.tags_push_failed,
			if outcome.tags_push_failed == 1 {
				""
			} else {
				"es"
			}
		);
	}
}

/// Counts publish outcomes for each project, printing per-project results.
///
/// Executes the actual publish operation for a project, handling output and errors.
async fn do_publish(project: &package_manager::Project) -> PublishResult {
	let version = project.version();
	let registry = project.registry_name().await;

	match project.publish().await {
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
mod tests;
