//! Publish command implementation.

mod github_releases;
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

/// Feature flags governing publish behavior.
#[derive(Debug)]
struct PublishFlags {
	dry_run: bool,
	git_enabled: bool,
	github_enabled: bool,
	no_git: bool,
	is_multi_package: bool,
}

/// Outcome of git tag and GitHub Release operations.
#[derive(Debug)]
struct GitReleaseOutcome {
	tags_created: usize,
	tags_skipped: usize,
	tags_push_failed: usize,
	github_created: usize,
	github_failed: bool,
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
	let (github_created, github_failed) = maybe_orchestrate_github_releases(
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
		github_created,
		github_failed,
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
/// Returns `(releases_created, any_failed)`.
async fn maybe_orchestrate_github_releases(
	git: &dyn Git,
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
	let client = env
		.code_forge_client()
		.map_err(|reason| anyhow::anyhow!("GitHub client not available: {reason}"))?;
	orchestrate_github_releases(
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
async fn run_pre_publish_github_checks(
	env: &crate::Env,
	config: &Config,
	git: &dyn Git,
	no_git: bool,
	dry_run: bool,
) -> anyhow::Result<bool> {
	if !config.github.enabled || no_git {
		return Ok(false);
	}
	if !dry_run && let Err(reason) = env.code_forge_client() {
		bail!("GitHub Releases is enabled but the code forge client is unavailable: {reason}");
	}
	run_github_build_command(env, config, git).await
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
	if run_pre_publish_github_checks(env, &config, git, args.no_git, dry_run).await? {
		return Ok(ExitCode::FAILURE);
	}
	let flags = PublishFlags {
		dry_run,
		git_enabled: config.git.enabled() && !args.no_git,
		github_enabled: config.github.enabled,
		no_git: args.no_git,
		is_multi_package: projects.len() > 1,
	};
	let publish = publish_projects(
		&sorted_projects,
		&graph,
		dry_run,
		env.fs(),
		config.git.publish_private_packages(),
	)
	.await?;
	let outcome = run_git_release_operations(git, &config, env, &publish.published, &flags).await?;
	log_publish_summary(&publish, &flags, &outcome);

	let code = if publish.failed || outcome.github_failed || outcome.tags_push_failed > 0 {
		ExitCode::FAILURE
	} else {
		ExitCode::SUCCESS
	};
	Ok(code)
}

/// Mutable state accumulated while iterating over projects in `publish_projects`.
struct PublishState {
	published: Vec<PublishedPackage>,
	skipped_count: usize,
	dep_skipped_count: usize,
	unprepared_count: usize,
	private_tagged_count: usize,
	failed: bool,
	blocked: std::collections::HashSet<String>,
}

impl PublishState {
	fn new() -> Self {
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
	fn record_private_tagged(&mut self, project: &package_manager::Project) {
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
	/// In real mode, delegates to `do_publish` and updates counts/blocked set on failure.
	async fn record_outcome(
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
				PublishResult::Skipped => self.skipped_count += 1,
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
/// are silently skipped unless listed in `publish_private_packages`, in which case they
/// receive git tags and GitHub Releases but are not published to any registry.
/// Public packages without a `CHANGELOG.md` (never prepared) are warned about and skipped
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
/// * `publish_private_packages` - Names of private packages that should receive git tags
///   and GitHub Releases (but not registry publish). Per ADR-043.
async fn publish_projects(
	projects: &[package_manager::Project],
	graph: &DependencyGraph,
	dry_run: bool,
	fs: &dyn crate::filesystem::Filesystem,
	publish_private_packages: &[String],
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
		// Private packages: silently skip unless opted into tag-only publishing (ADR-043).
		if !project.is_publishable() {
			let is_listed = publish_private_packages.iter().any(|p| p == project.name());
			if is_listed && !fs.exists(&project.path().child("CHANGELOG.md")).await? {
				warn!(
					"Skipping {}: no CHANGELOG.md found (run 'cursus prepare' first, with an appropriate changeset)",
					project.name()
				);
				state.unprepared_count += 1;
			} else if is_listed {
				state.record_private_tagged(project);
			}
			continue;
		}
		// Skip public packages that have never been prepared (no CHANGELOG.md).
		// Not added to `blocked` — dependents may be independently publishable.
		if !fs.exists(&project.path().child("CHANGELOG.md")).await? {
			warn!(
				"Skipping {}: no CHANGELOG.md found (run 'cursus prepare' first, with an appropriate changeset)",
				project.name()
			);
			state.unprepared_count += 1;
			continue;
		}
		state.record_outcome(project, graph, dry_run).await;
	}

	Ok(state)
}

/// Logs the GitHub Releases portion of the publish summary.
///
/// `registry_published` excludes private-tagged packages; `private_tagged_count` is
/// reported separately. GitHub Releases are created for all packages in `published`
/// (registry + private), so `failed_count` is derived from their combined total.
fn log_github_releases_summary(
	registry_published: usize,
	private_tagged_count: usize,
	skipped_count: usize,
	dep_skipped_note: &str,
	unprepared_note: &str,
	github_created: usize,
	github_failed: bool,
) {
	let private_note = if private_tagged_count > 0 {
		format!(", {private_tagged_count} private (tag only)")
	} else {
		String::new()
	};
	// GitHub Releases are attempted for both registry-published and private-tagged packages.
	let total_releasable = registry_published + private_tagged_count;
	match (github_created, github_failed) {
		(created, false) => info!(
			"Summary: {registry_published} published{private_note}, {skipped_count} skipped, \
			 {created} GitHub Releases created{dep_skipped_note}{unprepared_note}",
		),
		(created, true) => {
			let failed_count = total_releasable.saturating_sub(created);
			info!(
				"Summary: {registry_published} published{private_note}, {skipped_count} skipped, \
				 {created} GitHub Release{} created, {failed_count} GitHub Release{} failed\
				 {dep_skipped_note}{unprepared_note}",
				if created == 1 { "" } else { "s" },
				if failed_count == 1 { "" } else { "s" },
			);
		}
	}
}

/// Logs the first line of the publish summary (published/skipped/GitHub counts).
fn log_summary_line(state: &PublishState, flags: &PublishFlags, outcome: &GitReleaseOutcome) {
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
	let registry_published = state.published.len() - state.private_tagged_count;
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
	} else if flags.github_enabled && !flags.no_git {
		log_github_releases_summary(
			registry_published,
			state.private_tagged_count,
			state.skipped_count,
			&dep_skipped_note,
			&unprepared_note,
			outcome.github_created,
			outcome.github_failed,
		);
	} else {
		info!(
			"Summary: {registry_published} published{private_note}, {} skipped{dep_skipped_note}{unprepared_note}",
			state.skipped_count
		);
	}
}

/// Logs the publish summary after all publish operations have completed.
fn log_publish_summary(state: &PublishState, flags: &PublishFlags, outcome: &GitReleaseOutcome) {
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
mod tests {
	use super::*;

	#[tokio::test]
	async fn default_publish_args() {
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

	#[tokio::test]
	async fn add_transitive_dependents_linear_chain() {
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

	#[tokio::test]
	async fn add_transitive_dependents_diamond() {
		// d <- b <- a, d <- c <- a: if d fails, b, c and a should be blocked
		let graph = make_graph(&[("a", &["b", "c"]), ("b", &["d"]), ("c", &["d"]), ("d", &[])]);
		let mut blocked = std::collections::HashSet::new();
		add_transitive_dependents(&graph, "d", &mut blocked);
		assert!(blocked.contains("b"));
		assert!(blocked.contains("c"));
		assert!(blocked.contains("a"));
		assert!(!blocked.contains("d"));
	}

	#[tokio::test]
	async fn add_transitive_dependents_cycle_terminates() {
		// a <-> b: if a fails, b should be blocked; cycle must not loop infinitely
		let graph = make_graph(&[("a", &["b"]), ("b", &["a"])]);
		let mut blocked = std::collections::HashSet::new();
		add_transitive_dependents(&graph, "a", &mut blocked);
		assert!(blocked.contains("b"));
		// Must terminate (would panic/hang otherwise)
	}

	#[tokio::test]
	async fn add_transitive_dependents_independent_subtree_not_blocked() {
		// a -> b, c -> d: if b fails only a is blocked; c and d are unaffected
		let graph = make_graph(&[("a", &["b"]), ("b", &[]), ("c", &["d"]), ("d", &[])]);
		let mut blocked = std::collections::HashSet::new();
		add_transitive_dependents(&graph, "b", &mut blocked);
		assert!(blocked.contains("a"));
		assert!(!blocked.contains("c"));
		assert!(!blocked.contains("d"));
	}

	// ── log_summary_line tests ────────────────────────────────────────────────

	fn make_empty_outcome() -> GitReleaseOutcome {
		GitReleaseOutcome {
			tags_created: 0,
			tags_skipped: 0,
			tags_push_failed: 0,
			github_created: 0,
			github_failed: false,
		}
	}

	fn make_published_package() -> PublishedPackage {
		PublishedPackage {
			name: "pkg".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: crate::path::AbsolutePath::new("/nonexistent").unwrap(),
		}
	}

	#[tokio::test]
	async fn log_summary_line_non_dry_run_dep_skipped_note_in_log() {
		// dep_skipped_note only appears in non-dry-run mode.
		// Guards `> 0` → `> 1` on dep_skipped_count condition.
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let mut state = PublishState::new();
		state.dep_skipped_count = 1; // exactly 1, to catch "> 1" mutation
		let flags = PublishFlags {
			dry_run: false,
			git_enabled: false,
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		log_summary_line(&state, &flags, &make_empty_outcome());
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("skipped (dependency failed)")),
			"Expected dep-skipped note in log: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_summary_line_non_dry_run_unprepared_note_in_log() {
		// unprepared_note appears in both dry-run (via tag_note path) and non-dry-run.
		// Guards `> 0` → `> 1` on unprepared_count condition.
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let mut state = PublishState::new();
		state.unprepared_count = 1; // exactly 1, to catch "> 1" mutation
		let flags = PublishFlags {
			dry_run: false,
			git_enabled: false,
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		log_summary_line(&state, &flags, &make_empty_outcome());
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("skipped (not yet prepared)")),
			"Expected unprepared note in log: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_summary_line_dry_run_git_disabled_no_tag_note() {
		// Guards &&→|| on `flags.git_enabled && !state.published.is_empty()` (tag_note guard).
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let mut state = PublishState::new();
		state.published.push(make_published_package()); // non-empty published
		let flags = PublishFlags {
			dry_run: true,
			git_enabled: false, // git disabled
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		log_summary_line(&state, &flags, &make_empty_outcome());
		let logs = crate::test_logging::take_logs();
		assert!(
			!logs.iter().any(|(_, m)| m.contains("would be tagged")),
			"Should NOT log 'would be tagged' when git is disabled: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_summary_line_dry_run_git_enabled_tag_note_present() {
		// Guards &&→|| on `flags.git_enabled && !state.published.is_empty()` (tag_note guard).
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let mut state = PublishState::new();
		state.published.push(make_published_package());
		let flags = PublishFlags {
			dry_run: true,
			git_enabled: true, // git enabled
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		log_summary_line(&state, &flags, &make_empty_outcome());
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter().any(|(_, m)| m.contains("would be tagged")),
			"Should log 'would be tagged' when git is enabled: {logs:?}"
		);
	}

	// ── log_publish_summary tests ─────────────────────────────────────────────

	#[tokio::test]
	async fn log_publish_summary_tags_created_appears_in_log() {
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let state = PublishState::new();
		let flags = PublishFlags {
			dry_run: false,
			git_enabled: true,
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		let outcome = GitReleaseOutcome {
			tags_created: 2,
			tags_skipped: 0,
			tags_push_failed: 0,
			github_created: 0,
			github_failed: false,
		};
		log_publish_summary(&state, &flags, &outcome);
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("tag") && m.contains("created")),
			"Expected 'tag(s) created' in logs: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_publish_summary_tags_push_failed_appears_in_log() {
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let state = PublishState::new();
		let flags = PublishFlags {
			dry_run: false,
			git_enabled: true,
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		let outcome = GitReleaseOutcome {
			tags_created: 0,
			tags_skipped: 0,
			tags_push_failed: 1,
			github_created: 0,
			github_failed: false,
		};
		log_publish_summary(&state, &flags, &outcome);
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("tag") && m.contains("failed")),
			"Expected 'tag push(es) failed' in logs: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_publish_summary_dry_run_no_tag_log_lines() {
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let state = PublishState::new();
		let flags = PublishFlags {
			dry_run: true, // dry-run: no tag log lines expected
			git_enabled: true,
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		let outcome = GitReleaseOutcome {
			tags_created: 3,
			tags_skipped: 0,
			tags_push_failed: 2,
			github_created: 0,
			github_failed: false,
		};
		log_publish_summary(&state, &flags, &outcome);
		let logs = crate::test_logging::take_logs();
		assert!(
			!logs
				.iter()
				.any(|(_, m)| m.contains("created") && m.contains("tag")),
			"Should NOT log tag created count in dry-run: {logs:?}"
		);
		assert!(
			!logs
				.iter()
				.any(|(_, m)| m.contains("tag") && m.contains("failed")),
			"Should NOT log tag push failed in dry-run: {logs:?}"
		);
	}

	// ── log_github_releases_summary tests ─────────────────────────────────────

	#[tokio::test]
	async fn log_github_releases_summary_no_failure_logs_created_count() {
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		log_github_releases_summary(3, 0, 0, "", "", 2, false);
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("3 published") && m.contains("2 GitHub Releases created")),
			"Expected GitHub Release summary: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_github_releases_summary_with_failure_logs_failed_count() {
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		log_github_releases_summary(3, 0, 0, "", "", 2, true);
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("GitHub Release") && m.contains("failed")),
			"Expected GitHub Release failure count: {logs:?}"
		);
	}

	// ── PublishState::record_outcome tests ────────────────────────────────────

	#[tokio::test]
	async fn record_outcome_skipped_increments_skipped_count() {
		// Guards the `Skipped => self.skipped_count += 1` branch in non-dry-run mode.
		// Uses the NpmAdapter (default in new_test_with_runner) with EPUBLISHCONFLICT in
		// stderr to trigger PublishOutcome::AlreadyPublished → PublishResult::Skipped.
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		use std::sync::Arc;
		let runner = Arc::new(
			crate::command::test_support::RecordingCommandRunner::new(1)
				.with_stderr(b"npm ERR! code EPUBLISHCONFLICT".to_vec()),
		);
		let project = crate::package_manager::Project::new_test_with_runner(
			"pkg",
			"/nonexistent",
			Arc::clone(&runner),
		);
		let graph = make_graph(&[]);
		let mut state = PublishState::new();
		state.record_outcome(&project, &graph, false).await;
		assert_eq!(state.skipped_count, 1, "Expected skipped_count == 1");
		assert_eq!(state.published.len(), 0, "Expected no published packages");
	}

	// ── private_tagged_count tests ────────────────────────────────────────────

	#[tokio::test]
	async fn publish_state_private_tagged_count_starts_at_zero() {
		let state = PublishState::new();
		assert_eq!(state.private_tagged_count, 0);
	}

	#[tokio::test]
	async fn log_github_releases_summary_private_note_appears_when_nonzero() {
		// Guards `> 0` → `> 1` on `private_tagged_count` condition.
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		log_github_releases_summary(2, 1, 0, "", "", 3, false);
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter().any(|(_, m)| m.contains("private (tag only)")),
			"Expected private note in log: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_github_releases_summary_no_private_note_when_zero() {
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		log_github_releases_summary(2, 0, 0, "", "", 2, false);
		let logs = crate::test_logging::take_logs();
		assert!(
			!logs.iter().any(|(_, m)| m.contains("private (tag only)")),
			"Private note should not appear when count is zero: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_summary_line_dry_run_shows_private_note() {
		// Guards `> 0` → `> 1` on `private_tagged_count` condition in log_summary_line.
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let mut state = PublishState::new();
		state.published.push(make_published_package());
		state.private_tagged_count = 1;
		let flags = PublishFlags {
			dry_run: true,
			git_enabled: false,
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		log_summary_line(&state, &flags, &make_empty_outcome());
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter().any(|(_, m)| m.contains("private (tag only)")),
			"Expected private note in dry-run summary: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_summary_line_dry_run_registry_published_excludes_private() {
		// registry_published = published.len() - private_tagged_count
		// Guards `- private_tagged_count` being dropped (mutant: subtract 0 instead).
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let mut state = PublishState::new();
		// 1 registry publish + 1 private-tagged = 2 total in published
		state.published.push(make_published_package());
		state.published.push(make_published_package());
		state.private_tagged_count = 1;
		let flags = PublishFlags {
			dry_run: true,
			git_enabled: false,
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		log_summary_line(&state, &flags, &make_empty_outcome());
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter().any(|(_, m)| m.contains("1 would be published")),
			"Expected '1 would be published' (not 2) in summary: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_summary_line_non_dry_run_shows_private_note() {
		// Guards `> 0` → `> 1` on `private_tagged_count` in non-dry-run path.
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let mut state = PublishState::new();
		state.published.push(make_published_package());
		state.private_tagged_count = 1;
		let flags = PublishFlags {
			dry_run: false,
			git_enabled: false,
			github_enabled: false,
			no_git: false,
			is_multi_package: false,
		};
		log_summary_line(&state, &flags, &make_empty_outcome());
		let logs = crate::test_logging::take_logs();
		assert!(
			logs.iter().any(|(_, m)| m.contains("private (tag only)")),
			"Expected private note in non-dry-run summary: {logs:?}"
		);
	}

	#[tokio::test]
	async fn record_private_tagged_adds_to_published_and_increments_count() {
		// Guards the `record_private_tagged` method: package must be pushed to `published`
		// and `private_tagged_count` incremented, regardless of dry_run mode.
		use std::sync::Arc;
		let runner = Arc::new(
			crate::command::test_support::RecordingCommandRunner::new(0)
				.with_stdout(b"1.0.0".to_vec()),
		);
		let project = crate::package_manager::Project::new_test_with_runner(
			"my-action",
			"/nonexistent",
			Arc::clone(&runner),
		);
		let mut state = PublishState::new();
		state.record_private_tagged(&project);
		assert_eq!(state.published.len(), 1, "Expected 1 package in published");
		assert_eq!(
			state.private_tagged_count, 1,
			"Expected private_tagged_count == 1"
		);
		assert_eq!(state.published[0].name, "my-action");
	}

	#[tokio::test]
	async fn add_transitive_dependents_with_prepopulated_blocked_set() {
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
