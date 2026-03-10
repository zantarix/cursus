//! Publish command implementation.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, bail};
use clap::Args;
use log::{error, info, warn};

use crate::command::CommandRunner;
use crate::git;
use crate::github::client::GitHubClient;
use crate::github::remote::detect_github_repo;
use crate::model::changelog::extract_version_body;
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

/// Data about a successfully published package needed for GitHub Release creation.
struct PublishedPackage {
	/// Package name.
	name: String,
	/// Published version.
	version: semver::Version,
	/// Path to the project root, relative to the git root.
	project_path: PathBuf,
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
	/// Skip git tag creation, tag pushing, and GitHub Releases even if enabled in config
	#[arg(long)]
	pub no_git: bool,
}

/// Execute the publish command.
pub fn cmd_publish(
	git_workdir: &Path,
	args: &PublishArgs,
	runner: Arc<dyn CommandRunner>,
	github_client: Option<Arc<dyn GitHubClient>>,
) -> anyhow::Result<ExitCode> {
	// Load configuration and enumerate projects
	let config = config::load(git_workdir)?;
	let projects = config.load_projects(Arc::clone(&runner))?;

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
				warn!(
					"circular dependencies detected between: {}",
					group.join(", ")
				);
			}
			warn!(
				"To disable this warning, set `disable_dependency_cycle_warnings = true` in the [global] section of .chronicle/config.toml"
			);
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

	// Fail fast: validate GitHub token before publishing anything
	if config.github.enabled && !args.no_git && !args.dry_run && github_client.is_none() {
		bail!(
			"GitHub Releases is enabled but no GitHub token found. \
			 Set GH_TOKEN or GITHUB_TOKEN environment variable."
		);
	}

	let is_multi_package = projects.len() > 1;

	let (published_packages, skipped_count, publish_failed) =
		publish_projects(&sorted_projects, args.dry_run)?;

	// Git tag creation
	let git_enabled = config.git.enabled.unwrap_or(false) && !args.no_git;
	let (tags_created, tags_skipped) = if git_enabled {
		if args.dry_run {
			for pkg in &published_packages {
				let tag = config
					.git
					.tag_format
					.tag(&pkg.name, &pkg.version, is_multi_package);
				info!("Would create tag {tag}");
			}
			(0usize, 0usize)
		} else {
			create_and_push_tags(
				&published_packages,
				&config,
				runner.as_ref(),
				git_workdir,
				is_multi_package,
			)?
		}
	} else {
		(0, 0)
	};

	// GitHub Release orchestration — skipped when --no-git is set
	let (github_created, github_failed) = if config.github.enabled && !args.no_git {
		if args.dry_run {
			// Dry-run: print what would happen without making API calls
			for pkg in &published_packages {
				let tag = config
					.git
					.tag_format
					.tag(&pkg.name, &pkg.version, is_multi_package);
				info!("Would create GitHub Release for {tag}");
				for display_name in config.github.artifacts.keys() {
					info!("  Would attach: {display_name}");
				}
			}
			(0usize, false)
		} else {
			// The early token check above guarantees github_client is Some here.
			// If it is somehow None, bail rather than panic.
			let client = match github_client.as_deref() {
				Some(c) => c,
				None => bail!("GitHub client not available despite token being set"),
			};
			orchestrate_github_releases(
				git_workdir,
				&config,
				runner.as_ref(),
				client,
				&published_packages,
				is_multi_package,
			)?
		}
	} else {
		(0usize, false)
	};

	// Summary
	info!("");
	if args.dry_run {
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
	} else if config.github.enabled && !args.no_git {
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
	if !args.dry_run && git_enabled && (tags_created > 0 || tags_skipped > 0) {
		info!(
			"{tags_created} tag{} created, {tags_skipped} skipped",
			if tags_created == 1 { "" } else { "s" }
		);
	}

	if publish_failed || github_failed {
		Ok(ExitCode::FAILURE)
	} else {
		Ok(ExitCode::SUCCESS)
	}
}

/// Creates an annotated git tag for each published package and pushes all new tags.
///
/// Tags that already exist in the repository are skipped (making the operation idempotent).
/// Tags are pushed in a single `git push origin --tags` call after all tags are created.
///
/// Returns `(tags_created, tags_skipped)`.
fn create_and_push_tags(
	published: &[PublishedPackage],
	config: &config::Config,
	runner: &dyn CommandRunner,
	git_workdir: &Path,
	is_multi_package: bool,
) -> anyhow::Result<(usize, usize)> {
	let mut created_tags: Vec<String> = Vec::new();
	let mut skipped = 0;

	for pkg in published {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);

		if git::git_tag_exists(runner, git_workdir, &tag)? {
			info!("Tag {tag} already exists, skipping");
			skipped += 1;
			continue;
		}

		let message = format!("Release {} version {}", pkg.name, pkg.version);
		git::git_tag(runner, git_workdir, &tag, &message)?;
		info!("Created tag {tag}");
		created_tags.push(tag);
	}

	// Push only the tags created in this invocation (not all local tags).
	for tag_name in &created_tags {
		git::git_push_tag(runner, git_workdir, tag_name)?;
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
				project_path: project.path().to_path_buf(),
			});
		} else {
			// Real publish: delegate to do_publish which handles everything
			match do_publish(project) {
				PublishResult::Published => {
					published.push(PublishedPackage {
						name: project.name().to_string(),
						version: project.version().clone(),
						project_path: project.path().to_path_buf(),
					});
				}
				PublishResult::Skipped => skipped_count += 1,
				PublishResult::Failed => failed = true,
			}
		}
	}

	Ok((published, skipped_count, failed))
}

/// Orchestrates GitHub Release creation for all successfully published packages.
///
/// The caller must ensure that a GitHub token is available and that `github_client`
/// is `Some` before calling this function (enforced by the early check in `cmd_publish`).
///
/// Returns `(releases_created, any_failed)`.
fn orchestrate_github_releases(
	git_workdir: &Path,
	config: &config::Config,
	runner: &dyn CommandRunner,
	github_client: &dyn GitHubClient,
	published_packages: &[PublishedPackage],
	is_multi_package: bool,
) -> anyhow::Result<(usize, bool)> {
	if published_packages.is_empty() {
		return Ok((0, false));
	}

	// Resolve owner/repo from config or git remote
	let (owner, repo) = resolve_github_repo(config, runner, git_workdir)?;

	// Run build command if configured
	let mut github_failed = false;
	if !config.github.build_command.is_empty() {
		info!("Running build command: {}", config.github.build_command);
		let output = runner
			.run_shell(&config.github.build_command, git_workdir)
			.with_context(|| {
				format!(
					"Failed to execute build command: {}",
					config.github.build_command
				)
			})?;
		if !output.status.success() {
			error!("Build command failed with status {}", output.status);
			return Ok((0, true));
		}
	}

	let mut created_count = 0;

	for pkg in published_packages {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);

		// Read changelog body for the release
		let changelog_path = git_workdir.join(&pkg.project_path).join("CHANGELOG.md");
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

		// Create the release
		match github_client.create_release(&owner, &repo, &tag, &tag, &body) {
			Ok(release_id) => {
				info!("Created GitHub Release for {tag}");
				created_count += 1;

				// Upload artifacts
				for (display_name, artifact_path) in &config.github.artifacts {
					let full_path = git_workdir.join(artifact_path);
					match github_client.upload_asset(
						&owner,
						&repo,
						&release_id,
						display_name,
						&full_path,
					) {
						Ok(()) => info!("  Attached: {display_name}"),
						Err(e) => {
							warn!("  Failed to attach '{display_name}': {e:#}");
							github_failed = true;
						}
					}
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

/// Resolves the GitHub owner and repo name.
///
/// Checks config fields first, then falls back to detecting from the git remote URL.
fn resolve_github_repo(
	config: &config::Config,
	runner: &dyn CommandRunner,
	git_workdir: &Path,
) -> anyhow::Result<(String, String)> {
	match (&config.github.owner, &config.github.repo) {
		(Some(owner), Some(repo)) => return Ok((owner.clone(), repo.clone())),
		(Some(_), None) | (None, Some(_)) => bail!(
			"[github].owner and [github].repo must be set together; \
			 set both or omit both for auto-detection."
		),
		(None, None) => {}
	}

	match detect_github_repo(runner, git_workdir)? {
		Some(gh_repo) => Ok((gh_repo.owner, gh_repo.repo)),
		None => bail!(
			"Could not determine GitHub repository. Set [github] owner and repo in config, \
			 or ensure the git remote 'origin' points to a GitHub repository."
		),
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
	use std::collections::BTreeMap;
	use std::path::PathBuf;

	use super::*;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::github::GitHubConfig;
	use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
	use crate::model::config::Config;

	/// Builds a config with GitHub enabled, using known owner/repo to avoid git detection.
	fn make_github_config(
		build_command: &str,
		artifacts: BTreeMap<String, String>,
	) -> GitHubConfig {
		GitHubConfig {
			enabled: true,
			owner: Some("acme".to_string()),
			repo: Some("app".to_string()),
			build_command: build_command.to_string(),
			artifacts,
			pull_request_title: None,
		}
	}

	fn workdir() -> PathBuf {
		PathBuf::from("/tmp")
	}

	// --- Tests for orchestrate_github_releases ---

	#[test]
	fn github_release_skipped_when_no_published_packages() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = RecordingCommandRunner::new(0);

		let (created, failed) =
			orchestrate_github_releases(&workdir(), &config, &runner, &client, &[], false).unwrap();

		assert_eq!(created, 0);
		assert!(!failed);
		assert!(client.invocations().is_empty());
	}

	#[test]
	fn github_releases_created_for_published_packages() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = RecordingCommandRunner::new(0);

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: PathBuf::new(),
		}];

		let (created, failed) =
			orchestrate_github_releases(&workdir(), &config, &runner, &client, &packages, false)
				.unwrap();

		assert_eq!(created, 1);
		assert!(!failed);
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(matches!(
			&invocations[0],
			GitHubInvocation::CreateRelease { tag_name, owner, repo, .. }
				if tag_name == "v1.2.0" && owner == "acme" && repo == "app"
		));
	}

	#[test]
	fn github_releases_uses_prefixed_tag_for_monorepo() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = RecordingCommandRunner::new(0);

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: PathBuf::new(),
		}];

		let (created, failed) = orchestrate_github_releases(
			&workdir(),
			&config,
			&runner,
			&client,
			&packages,
			true, // is_multi_package
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
	fn github_release_build_command_failure_skips_releases() {
		let config =
			Config::new(&workdir()).with_github(make_github_config("exit 1", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = RecordingCommandRunner::new(1); // fails

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: PathBuf::new(),
		}];

		let (created, failed) =
			orchestrate_github_releases(&workdir(), &config, &runner, &client, &packages, false)
				.unwrap();

		assert_eq!(created, 0);
		assert!(failed);
		assert!(client.invocations().is_empty());
	}

	#[test]
	fn github_release_create_failure_continues_other_packages() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new().with_create_failure();
		let runner = RecordingCommandRunner::new(0);

		let packages = vec![
			PublishedPackage {
				name: "pkg-a".to_string(),
				version: "1.0.0".parse().unwrap(),
				project_path: PathBuf::new(),
			},
			PublishedPackage {
				name: "pkg-b".to_string(),
				version: "2.0.0".parse().unwrap(),
				project_path: PathBuf::new(),
			},
		];

		let (created, failed) =
			orchestrate_github_releases(&workdir(), &config, &runner, &client, &packages, true)
				.unwrap();

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

		let github_cfg = GitHubConfig {
			enabled: true,
			owner: Some("acme".to_string()),
			repo: Some("app".to_string()),
			build_command: String::new(),
			artifacts: artifacts_with_paths,
			pull_request_title: None,
		};

		let config = Config::new(dir.path()).with_github(github_cfg);
		let client = RecordingGitHubClient::new().with_upload_failure();
		let runner = RecordingCommandRunner::new(0);

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: PathBuf::new(),
		}];

		let (created, failed) =
			orchestrate_github_releases(dir.path(), &config, &runner, &client, &packages, false)
				.unwrap();

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

		let github_cfg = GitHubConfig {
			enabled: true,
			owner: Some("acme".to_string()),
			repo: Some("app".to_string()),
			build_command: String::new(),
			artifacts,
			pull_request_title: None,
		};
		let config = Config::new(dir.path()).with_github(github_cfg);
		let client = RecordingGitHubClient::new();
		let runner = RecordingCommandRunner::new(0);

		let packages = vec![
			PublishedPackage {
				name: "pkg-a".to_string(),
				version: "1.0.0".parse().unwrap(),
				project_path: PathBuf::new(),
			},
			PublishedPackage {
				name: "pkg-b".to_string(),
				version: "2.0.0".parse().unwrap(),
				project_path: PathBuf::new(),
			},
		];

		let (created, failed) =
			orchestrate_github_releases(dir.path(), &config, &runner, &client, &packages, true)
				.unwrap();

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

	// --- Tests for resolve_github_repo ---

	#[test]
	fn resolve_github_repo_uses_config_when_set() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let runner = RecordingCommandRunner::new(0);

		let (owner, repo) = resolve_github_repo(&config, &runner, &workdir()).unwrap();
		assert_eq!(owner, "acme");
		assert_eq!(repo, "app");
		// Config values take priority — no git command should run
		assert!(runner.invocations().is_empty());
	}

	#[test]
	fn resolve_github_repo_falls_back_to_git_remote() {
		// Config with no owner/repo
		let github_cfg = GitHubConfig {
			enabled: true,
			owner: None,
			repo: None,
			build_command: String::new(),
			artifacts: BTreeMap::new(),
			pull_request_title: None,
		};
		let config = Config::new(&workdir()).with_github(github_cfg);
		// Runner returns a GitHub HTTPS remote URL
		let runner = RecordingCommandRunner::new(0)
			.with_stdout(b"https://github.com/myorg/myapp.git\n".to_vec());

		let (owner, repo) = resolve_github_repo(&config, &runner, &workdir()).unwrap();
		assert_eq!(owner, "myorg");
		assert_eq!(repo, "myapp");
	}

	#[test]
	fn resolve_github_repo_errors_when_neither_config_nor_remote() {
		// Config with no owner/repo
		let github_cfg = GitHubConfig {
			enabled: true,
			owner: None,
			repo: None,
			build_command: String::new(),
			artifacts: BTreeMap::new(),
			pull_request_title: None,
		};
		let config = Config::new(&workdir()).with_github(github_cfg);
		// Runner returns failure (no origin remote)
		let runner = RecordingCommandRunner::new(1);

		let result = resolve_github_repo(&config, &runner, &workdir());
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("Could not determine GitHub repository"),
			"Expected repo detection error, got: {msg}"
		);
	}

	#[test]
	fn resolve_github_repo_errors_when_only_owner_set() {
		let github_cfg = GitHubConfig {
			enabled: true,
			owner: Some("acme".to_string()),
			repo: None,
			build_command: String::new(),
			artifacts: BTreeMap::new(),
			pull_request_title: None,
		};
		let config = Config::new(&workdir()).with_github(github_cfg);
		let runner = RecordingCommandRunner::new(0);

		let result = resolve_github_repo(&config, &runner, &workdir());
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("must be set together"),
			"Expected partial config error, got: {msg}"
		);
	}

	#[test]
	fn default_publish_args() {
		let args = PublishArgs::default();
		assert!(!args.dry_run);
		assert!(args.packages.is_empty());
		assert!(!args.no_git);
	}

	// --- Tests for create_and_push_tags ---

	#[test]
	fn create_and_push_tags_creates_annotated_tags_and_pushes() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(dir.path());
		// empty stdout → git_tag_exists returns false (no existing tag)
		let runner = RecordingCommandRunner::new(0);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: PathBuf::new(),
		}];

		let (created, skipped) =
			create_and_push_tags(&published, &config, &runner, dir.path(), false).unwrap();

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
		let config = Config::new(dir.path());
		// non-empty stdout → git_tag_exists returns true (tag already exists)
		let runner = RecordingCommandRunner::new(0).with_stdout(b"v1.0.0\n".to_vec());
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: PathBuf::new(),
		}];

		let (created, skipped) =
			create_and_push_tags(&published, &config, &runner, dir.path(), false).unwrap();

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
		let config = Config::new(dir.path());
		let runner = RecordingCommandRunner::new(0);

		let (created, skipped) =
			create_and_push_tags(&[], &config, &runner, dir.path(), false).unwrap();

		assert_eq!(created, 0);
		assert_eq!(skipped, 0);
		assert!(runner.invocations().is_empty());
	}

	#[test]
	fn create_and_push_tags_uses_prefixed_tag_for_monorepo() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(dir.path());
		let runner = RecordingCommandRunner::new(0);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "2.0.0".parse().unwrap(),
			project_path: PathBuf::new(),
		}];

		create_and_push_tags(&published, &config, &runner, dir.path(), true).unwrap();

		let invocations = runner.invocations();
		// The tag -l check should use the prefixed tag name
		assert!(
			invocations[0].args.contains(&"my-app@2.0.0".to_string()),
			"Expected monorepo tag name, got: {:?}",
			invocations[0].args
		);
	}
}
