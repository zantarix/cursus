//! Publish command implementation.

use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::Args;
use log::{error, info, warn};

use crate::git;
use crate::github::GitHubRepo;
use crate::github::client::GitHubClient;
use crate::model::changelog::extract_version_body;
use crate::model::config::Config;
use crate::package_manager::{self, PublishOutcome, filter_projects_by_name};
use crate::path::AbsolutePath;

/// Result of attempting to publish a package.
enum PublishResult {
	/// Package was successfully published.
	Published,
	/// Package was already published (skipped).
	Skipped,
	/// Publish failed.
	Failed,
}

/// Data about a successfully published package needed for GitHub Release creation.
struct PublishedPackage {
	/// Package name.
	name: String,
	/// Published version.
	version: semver::Version,
	/// Absolute path to the project root.
	project_path: AbsolutePath,
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
fn sort_projects_by_dependency(
	projects: &[crate::package_manager::Project],
	selected_projects: Vec<crate::package_manager::Project>,
	disable_cycle_warnings: bool,
) -> anyhow::Result<Vec<crate::package_manager::Project>> {
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
				 in the [global] section of .chronicle/config.toml"
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
	Ok(sorted)
}

/// Creates git tags and GitHub Releases for all published packages.
///
/// Returns `(tags_created, tags_skipped, github_created, github_failed)`.
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
) -> anyhow::Result<(usize, usize, usize, bool)> {
	let (tags_created, tags_skipped) = maybe_create_tags(
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
	Ok((tags_created, tags_skipped, github_created, github_failed))
}

/// Creates git tags for published packages (or logs dry-run intent) and returns counts.
///
/// Returns `(tags_created, tags_skipped)`.
fn maybe_create_tags(
	published_packages: &[PublishedPackage],
	config: &Config,
	git: &git::GitWorkdir,
	dry_run: bool,
	git_enabled: bool,
	is_multi_package: bool,
) -> anyhow::Result<(usize, usize)> {
	if !git_enabled {
		return Ok((0, 0));
	}
	if dry_run {
		for pkg in published_packages {
			let tag = config
				.git
				.tag_format
				.tag(&pkg.name, &pkg.version, is_multi_package);
			info!("Would create tag {tag}");
		}
		return Ok((0, 0));
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
	let sorted_projects = sort_projects_by_dependency(
		&projects,
		selected_projects,
		config.global.disable_dependency_cycle_warnings,
	)?;
	if run_pre_publish_github_checks(env, &config, git, args.no_git, dry_run)? {
		return Ok(ExitCode::FAILURE);
	}
	let is_multi_package = projects.len() > 1;
	let (published_packages, skipped_count, publish_failed) =
		publish_projects(&sorted_projects, dry_run)?;
	let git_enabled = config.git.enabled() && !args.no_git;
	let (tags_created, tags_skipped, github_created, github_failed) = run_git_release_operations(
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
		dry_run,
		git_enabled,
		tags_created,
		tags_skipped,
		config.github.enabled,
		args.no_git,
		github_created,
		github_failed,
	);

	let code = if publish_failed || github_failed {
		ExitCode::FAILURE
	} else {
		ExitCode::SUCCESS
	};
	Ok(code)
}

/// Creates an annotated git tag for each published package and pushes all new tags.
///
/// Tags that already exist in the repository are skipped (making the operation idempotent).
/// Tags are pushed in a single `git push origin --tags` call after all tags are created.
///
/// Returns `(tags_created, tags_skipped)`.
fn create_and_push_tags(
	published: &[PublishedPackage],
	config: &Config,
	git: &git::GitWorkdir,
	is_multi_package: bool,
) -> anyhow::Result<(usize, usize)> {
	let mut created_tags: Vec<String> = Vec::new();
	let mut skipped = 0;

	for pkg in published {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);

		if git.tag_exists(&tag)? {
			info!("Tag {tag} already exists, skipping");
			skipped += 1;
			continue;
		}

		let message = format!("Release {} version {}", pkg.name, pkg.version);
		git.tag(&tag, &message)?;
		info!("Created tag {tag}");
		created_tags.push(tag);
	}

	// Push only the tags created in this invocation (not all local tags).
	for tag_name in &created_tags {
		git.push_tag(tag_name)?;
	}

	let created = created_tags.len();
	if created > 0 {
		info!(
			"Pushed {} tag{} to origin",
			created,
			if created == 1 { "" } else { "s" }
		);
	}

	Ok((created, skipped))
}

/// Publishes the given projects to their registries, tracking outcomes.
///
/// Projects should be pre-sorted in dependency order (leaves first).
/// Private packages (marked with `private: true` in npm or `publish = false` in Cargo)
/// are silently skipped.
///
/// Returns `(published_packages, skipped_count, failed)`.
///
/// # Arguments
///
/// * `projects` - Projects to publish, pre-sorted in dependency order.
/// * `dry_run` - If true, only print what would be published without actually publishing.
fn publish_projects(
	projects: &[package_manager::Project],
	dry_run: bool,
) -> anyhow::Result<(Vec<PublishedPackage>, usize, bool)> {
	let mut published = Vec::new();
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
				PublishResult::Failed => failed = true,
			}
		}
	}

	Ok((published, skipped_count, failed))
}

/// Logs what GitHub Releases and artifacts would be created in a dry run.
fn log_dry_run_github_releases(
	published_packages: &[PublishedPackage],
	config: &crate::model::config::Config,
	is_multi_package: bool,
) {
	for pkg in published_packages {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);
		info!("Would create GitHub Release for {tag}");
		for display_name in config.github.artifacts.keys() {
			info!("  Would attach: {display_name}");
		}
	}
}

/// Logs the publish summary after all publish operations have completed.
#[allow(clippy::too_many_arguments)]
fn log_publish_summary(
	published_packages: &[PublishedPackage],
	skipped_count: usize,
	dry_run: bool,
	git_enabled: bool,
	tags_created: usize,
	tags_skipped: usize,
	github_enabled: bool,
	no_git: bool,
	github_created: usize,
	github_failed: bool,
) {
	info!("");
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
	} else if github_enabled && !no_git {
		match (github_created, github_failed) {
			(created, false) => info!(
				"Summary: {} published, {} skipped, {} GitHub Releases created",
				published_packages.len(),
				skipped_count,
				created
			),
			(created, true) => {
				let failed_count = published_packages.len().saturating_sub(created);
				info!(
					"Summary: {} published, {} skipped, {} GitHub Release{} created, {} GitHub Release{} failed",
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
			"Summary: {} published, {} skipped",
			published_packages.len(),
			skipped_count
		);
	}
	if !dry_run && git_enabled && (tags_created > 0 || tags_skipped > 0) {
		info!(
			"{tags_created} tag{} created, {tags_skipped} skipped",
			if tags_created == 1 { "" } else { "s" }
		);
	}
}

/// Runs the configured GitHub pre-release build command, if any.
///
/// Returns `true` if the build command failed, `false` if it succeeded or was not configured.
fn run_github_build_command(
	env: &crate::Env,
	config: &Config,
	git: &git::GitWorkdir,
) -> anyhow::Result<bool> {
	if config.github.build_command.is_empty() {
		return Ok(false);
	}
	info!("Running build command: {}", config.github.build_command);
	let output = env
		.run_shell_mut(&config.github.build_command, git.path())
		.with_context(|| {
			format!(
				"Failed to execute build command: {}",
				config.github.build_command
			)
		})?;
	if !output.status.success() {
		error!("Build command failed with status {}", output.status);
		return Ok(true);
	}
	Ok(false)
}

/// Orchestrates GitHub Release creation for all successfully published packages.
///
/// The caller must ensure that a GitHub token is available and that `github_client`
/// is `Some` before calling this function (enforced by the early check in `cmd_publish`).
///
/// Returns `(releases_created, any_failed)`.
fn orchestrate_github_releases(
	git: &git::GitWorkdir,
	config: &Config,
	github_client: &dyn GitHubClient,
	published_packages: &[PublishedPackage],
	is_multi_package: bool,
) -> anyhow::Result<(usize, bool)> {
	if published_packages.is_empty() {
		return Ok((0, false));
	}

	let gh_repo = GitHubRepo::resolve(&config.github, git)?;
	let mut github_failed = false;
	let mut created_count = 0;

	for pkg in published_packages {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);

		// Read changelog body for the release
		let changelog_path = pkg.project_path.join("CHANGELOG.md");
		let body = if changelog_path.exists() {
			match extract_version_body(&changelog_path, &pkg.version) {
				Ok(text) => text,
				Err(e) => {
					warn!("could not read changelog for {}: {e:#}", pkg.name);
					String::new()
				}
			}
		} else {
			String::new()
		};

		// Create the release and upload artifacts
		match github_client.create_release(&gh_repo, &tag, &tag, &body) {
			Ok(release_id) => {
				info!("Created GitHub Release for {tag}");
				created_count += 1;
				if upload_release_artifacts(
					github_client,
					&gh_repo,
					&release_id,
					&config.github.artifacts,
					git.path(),
				) {
					github_failed = true;
				}
			}
			Err(e) => {
				error!("Failed to create GitHub Release for {tag}: {e:#}");
				github_failed = true;
			}
		}
	}

	Ok((created_count, github_failed))
}

/// Uploads all configured artifacts to a GitHub release.
///
/// Returns `true` if any upload failed, `false` if all succeeded.
fn upload_release_artifacts(
	github_client: &dyn GitHubClient,
	gh_repo: &GitHubRepo,
	release_id: &str,
	artifacts: &std::collections::BTreeMap<String, String>,
	git_root: &crate::path::AbsolutePath,
) -> bool {
	let mut any_failed = false;
	for (display_name, artifact_path) in artifacts {
		let full_path = git_root.join(artifact_path);
		match github_client.upload_asset(gh_repo, release_id, display_name, &full_path) {
			Ok(()) => info!("  Attached: {display_name}"),
			Err(e) => {
				warn!("  Failed to attach '{display_name}': {e:#}");
				any_failed = true;
			}
		}
	}
	any_failed
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
	use std::collections::BTreeMap;

	use std::sync::Arc;

	use super::*;
	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
	use crate::model::config::{Config, GitHubConfig};

	/// Builds a config with GitHub enabled, using known owner/repo to avoid git detection.
	fn make_github_config(
		build_command: &str,
		artifacts: BTreeMap<String, String>,
	) -> GitHubConfig {
		let mut config = GitHubConfig::enabled_config();
		config.build_command = build_command.to_string();
		config.artifacts = artifacts;
		config
			.with_owner("acme".to_string())
			.with_repo("app".to_string())
	}

	fn workdir() -> crate::path::AbsolutePath {
		crate::path::AbsolutePath::new("/tmp").unwrap()
	}

	// --- Tests for orchestrate_github_releases ---

	#[test]
	fn github_release_skipped_when_no_published_packages() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			wd.clone(),
		);

		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &[], false).unwrap();

		assert_eq!(created, 0);
		assert!(!failed);
		assert!(client.invocations().is_empty());
	}

	#[test]
	fn github_releases_created_for_published_packages() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			wd.clone(),
		);
		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, false).unwrap();

		assert_eq!(created, 1);
		assert!(!failed);
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(matches!(
			&invocations[0],
			GitHubInvocation::CreateRelease { tag_name, gh_repo, .. }
				if tag_name == "v1.2.0" && gh_repo.owner == "acme" && gh_repo.repo == "app"
		));
	}

	#[test]
	fn github_releases_uses_prefixed_tag_for_monorepo() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			wd.clone(),
		);
		let (created, failed) = orchestrate_github_releases(
			&git, &config, &client, &packages, true, // is_multi_package
		)
		.unwrap();

		assert_eq!(created, 1);
		assert!(!failed);
		let invocations = client.invocations();
		assert!(matches!(
			&invocations[0],
			GitHubInvocation::CreateRelease { tag_name, .. } if tag_name == "my-app@1.2.0"
		));
	}

	#[test]
	fn github_release_create_failure_continues_other_packages() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new().with_create_failure();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![
			PublishedPackage {
				name: "pkg-a".to_string(),
				version: "1.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
			PublishedPackage {
				name: "pkg-b".to_string(),
				version: "2.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
		];

		let wd = workdir();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			wd.clone(),
		);
		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, true).unwrap();

		assert_eq!(created, 0);
		assert!(failed);
		// Both packages should have been attempted
		assert_eq!(client.invocations().len(), 2);
	}

	#[test]
	fn github_release_upload_failure_continues_other_artifacts() {
		// Create the artifact files
		let dir = tempfile::tempdir().unwrap();
		let linux_path = dir.path().join("linux.tar.gz");
		let macos_path = dir.path().join("macos.tar.gz");
		std::fs::write(&linux_path, b"linux binary").unwrap();
		std::fs::write(&macos_path, b"macos binary").unwrap();

		let mut artifacts_with_paths = BTreeMap::new();
		artifacts_with_paths.insert(
			"linux".to_string(),
			linux_path.to_string_lossy().into_owned(),
		);
		artifacts_with_paths.insert(
			"macos".to_string(),
			macos_path.to_string_lossy().into_owned(),
		);

		let github_cfg = {
			let mut c = GitHubConfig::enabled_config();
			c.artifacts = artifacts_with_paths;
			c.with_owner("acme".to_string())
				.with_repo("app".to_string())
		};

		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_github(github_cfg);
		let client = RecordingGitHubClient::new().with_upload_failure();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);

		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, false).unwrap();

		// Release was created even though uploads failed
		assert_eq!(created, 1);
		assert!(failed);

		// Both artifacts were attempted despite first failure
		let uploads: Vec<_> = client
			.invocations()
			.into_iter()
			.filter(|i| matches!(i, GitHubInvocation::UploadAsset { .. }))
			.collect();
		assert_eq!(uploads.len(), 2);
	}

	#[test]
	fn github_release_artifacts_attached_to_every_release() {
		let dir = tempfile::tempdir().unwrap();
		let artifact_path = dir.path().join("app.tar.gz");
		std::fs::write(&artifact_path, b"binary content").unwrap();

		let mut artifacts = BTreeMap::new();
		artifacts.insert(
			"app".to_string(),
			artifact_path.to_string_lossy().into_owned(),
		);

		let github_cfg = {
			let mut c = GitHubConfig::enabled_config();
			c.artifacts = artifacts;
			c.with_owner("acme".to_string())
				.with_repo("app".to_string())
		};
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_github(github_cfg);
		let client = RecordingGitHubClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![
			PublishedPackage {
				name: "pkg-a".to_string(),
				version: "1.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
			PublishedPackage {
				name: "pkg-b".to_string(),
				version: "2.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
		];
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);

		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, true).unwrap();

		assert_eq!(created, 2);
		assert!(!failed);

		let invocations = client.invocations();
		let upload_count = invocations
			.iter()
			.filter(|i| matches!(i, GitHubInvocation::UploadAsset { .. }))
			.count();
		// Each of 2 packages should have 1 artifact each
		assert_eq!(upload_count, 2);
	}

	#[test]
	fn default_publish_args() {
		let args = PublishArgs::default();
		assert!(args.packages.is_empty());
		assert!(!args.no_git);
	}

	// --- Tests for create_and_push_tags ---

	#[test]
	fn create_and_push_tags_creates_annotated_tags_and_pushes() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// empty stdout → git_tag_exists returns false (no existing tag)
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let (created, skipped) = create_and_push_tags(&published, &config, &git, false).unwrap();

		assert_eq!(created, 1);
		assert_eq!(skipped, 0);
		let invocations = runner.invocations();
		// tag -l (exists check), tag -a (create), push origin <tag>
		assert_eq!(invocations.len(), 3);
		assert!(invocations[0].args.contains(&"-l".to_string()));
		assert!(invocations[1].args.contains(&"-a".to_string()));
		// Pushes the specific tag, not all tags
		assert!(
			invocations[2].args.contains(&"v1.2.0".to_string()),
			"Expected specific tag push, got: {:?}",
			invocations[2].args
		);
		assert!(
			!invocations[2].args.contains(&"--tags".to_string()),
			"Should not push --tags (all local tags)"
		);
	}

	#[test]
	fn create_and_push_tags_skips_existing_tag() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// non-empty stdout → git_tag_exists returns true (tag already exists)
		let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b"v1.0.0\n".to_vec()));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let (created, skipped) = create_and_push_tags(&published, &config, &git, false).unwrap();

		assert_eq!(created, 0);
		assert_eq!(skipped, 1);
		// Only the tag -l check; no tag creation or push
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].args.contains(&"-l".to_string()));
	}

	#[test]
	fn create_and_push_tags_empty_list_does_nothing() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);

		let (created, skipped) = create_and_push_tags(&[], &config, &git, false).unwrap();

		assert_eq!(created, 0);
		assert_eq!(skipped, 0);
		assert!(runner.invocations().is_empty());
	}

	#[test]
	fn create_and_push_tags_does_not_log_when_nothing_created() {
		// When no packages are published the "Pushed N tag(s)" info line must NOT appear.
		// This guards against mutations that make the `if created > 0` guard always true
		// (e.g. replace `>` with `>=`), which would log a misleading message for 0 tags.
		use crate::test_logging::{init_test_logger, take_logs};
		init_test_logger();
		let _ = take_logs();

		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs,
		);

		let (created, skipped) = create_and_push_tags(&[], &config, &git, false).unwrap();
		assert_eq!(created, 0);
		assert_eq!(skipped, 0);

		let logs = take_logs();
		assert!(
			!logs
				.iter()
				.any(|(_, m)| m.contains("Pushed") && m.contains("tag")),
			"Should not log a 'Pushed N tag(s)' message when nothing was created, got: {logs:?}"
		);
	}

	#[test]
	fn create_and_push_tags_logs_when_tags_created() {
		// When a tag IS created the "Pushed N tag(s)" info line MUST appear.
		// This guards against mutations that make `if created > 0` always false
		// (e.g. replace `>` with `<`), which would suppress the log even when tags
		// were actually pushed.
		use crate::test_logging::{init_test_logger, take_logs};
		init_test_logger();
		let _ = take_logs();

		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// Empty stdout → git_tag_exists returns false → tag gets created
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs,
		);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let (created, skipped) = create_and_push_tags(&published, &config, &git, false).unwrap();
		assert_eq!(created, 1);
		assert_eq!(skipped, 0);

		let logs = take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("Pushed") && m.contains("tag")),
			"Should log 'Pushed N tag(s)' when tags were created, got: {logs:?}"
		);
	}

	#[test]
	fn create_and_push_tags_uses_prefixed_tag_for_monorepo() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "2.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		create_and_push_tags(&published, &config, &git, true).unwrap();

		let invocations = runner.invocations();
		// The tag -l check should use the prefixed tag name
		assert!(
			invocations[0].args.contains(&"my-app@2.0.0".to_string()),
			"Expected monorepo tag name, got: {:?}",
			invocations[0].args
		);
	}
}
