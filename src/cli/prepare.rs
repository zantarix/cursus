//! The `prepare` subcommand.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use clap::Args;
use log::info;

use semver::Version;

use crate::command::CommandRunner;
use crate::git::{self, DEFAULT_RELEASE_BRANCH_PREFIX, Strategy};
use crate::github::client::GitHubClient;
use crate::github::{DEFAULT_PR_TITLE, GitHubRepo};
use crate::model::changelog::Changelog;
use crate::model::changeset::{ChangeType, Changeset};
use crate::model::config::Config;
use crate::package_manager::filter_projects_by_name;
use crate::utils::today_iso_date;

/// Arguments for the `prepare` subcommand.
#[derive(Args, Default)]
pub struct PrepareArgs {
	/// Preview changes without modifying any files
	#[arg(long)]
	pub dry_run: bool,

	/// Only prepare specific packages (repeatable)
	#[arg(short = 'p', long = "package")]
	pub packages: Vec<String>,

	/// Skip git lifecycle automation even if enabled in config
	#[arg(long)]
	pub no_git: bool,

	/// Override the release branch name (branch strategy only).
	///
	/// If not provided, the branch name is derived from `release_branch_prefix`
	/// in the config plus the current branch name.
	#[arg(long)]
	pub branch: Option<String>,
}

/// Bumps a semver version according to the given change type.
fn bump_version(version: &semver::Version, change_type: ChangeType) -> semver::Version {
	let mut v = version.clone();
	match change_type {
		ChangeType::Major => {
			v.major += 1;
			v.minor = 0;
			v.patch = 0;
		}
		ChangeType::Minor => {
			v.minor += 1;
			v.patch = 0;
		}
		ChangeType::Patch => {
			v.patch += 1;
		}
	}
	v.pre = semver::Prerelease::EMPTY;
	v.build = semver::BuildMetadata::EMPTY;
	v
}

/// Checks that the working tree is clean before making changes.
///
/// # Errors
///
/// Returns an error if the working tree has uncommitted changes.
fn check_dirty_tree(git: &git::GitWorkdir<'_>) -> anyhow::Result<()> {
	let status = git.status_porcelain()?;
	if !status.trim().is_empty() {
		anyhow::bail!(
			"Working tree is dirty. Commit or stash changes before releasing.\n\
			 Run `git status` to see pending changes."
		);
	}
	Ok(())
}

/// Computes the release branch name from CLI flags, config, and current branch.
///
/// Priority order:
/// 1. `args_branch` — explicit `--branch` flag
/// 2. `{config_prefix}{current_branch}` — derived from config prefix and current branch
/// 3. `chronicle-release/detached` — fallback when HEAD is detached
fn compute_release_branch(
	args_branch: Option<&str>,
	config_prefix: Option<&str>,
	current_branch: Option<&str>,
) -> String {
	if let Some(branch) = args_branch {
		return branch.to_string();
	}
	let prefix = config_prefix.unwrap_or(DEFAULT_RELEASE_BRANCH_PREFIX);
	let base = current_branch.unwrap_or("detached");
	format!("{prefix}{base}")
}

/// Information about a single package prepared for release.
#[derive(Debug)]
struct ReleaseInfo {
	package_name: String,
	new_version: Version,
}

/// Normalises a path by resolving `.` and `..` components without requiring
/// the path to exist on disk.
fn normalize_path(path: &Path) -> PathBuf {
	use std::path::Component;
	let mut result = PathBuf::new();
	for component in path.components() {
		match component {
			Component::ParentDir => {
				result.pop();
			}
			Component::CurDir => {}
			c => result.push(c),
		}
	}
	result
}

/// Formats the git commit message for a prepare run.
///
/// Produces `chore(release): pkg1@1.0.0, pkg2@2.0.0` for the given releases.
///
/// # Note
///
/// `release_infos` must be non-empty; passing an empty slice produces the
/// malformed string `"chore(release): "`. This is guaranteed by the early
/// return in [`stage_and_commit`] which skips all git operations when there
/// are no releases.
fn format_commit_message(release_infos: &[ReleaseInfo]) -> String {
	let parts: Vec<String> = release_infos
		.iter()
		.map(|r| format!("{}@{}", r.package_name, r.new_version))
		.collect();
	format!("chore(release): {}", parts.join(", "))
}

/// Stages files and creates a commit for the prepare step.
///
/// This is the core git operation for `prepare` — it only commits.
/// Pushing is handled by the strategy dispatch in `cmd_prepare`, and tagging
/// happens in `publish`.
///
/// If `dry_run` is `true`, prints what would be done without executing any git commands.
///
/// # Arguments
///
/// * `git` - Git working directory with command runner.
/// * `extra_files` - Additional files to unconditionally stage, relative to the git root.
/// * `release_infos` - The packages that were prepared for release.
/// * `modified_files` - Files to stage before committing.
/// * `dry_run` - If `true`, only print a summary; do not modify git state.
///
/// # Errors
///
/// Returns an error if any git command fails.
fn stage_and_commit(
	git: &git::GitWorkdir<'_>,
	extra_files: &[String],
	release_infos: &[ReleaseInfo],
	modified_files: &[PathBuf],
	dry_run: bool,
) -> anyhow::Result<()> {
	if release_infos.is_empty() {
		return Ok(());
	}

	let commit_message = format_commit_message(release_infos);

	// Build the full staging list, validating that extra_files resolve inside the repo root.
	let git_workdir = git.path();
	let mut all_files = modified_files.to_vec();
	for f in extra_files {
		let resolved = normalize_path(&git_workdir.join(f));
		if !resolved.starts_with(git_workdir) {
			anyhow::bail!(
				"extra_files entry {:?} resolves outside the repository root",
				f
			);
		}
		all_files.push(resolved);
	}

	if dry_run {
		info!("Would create commit: {commit_message}");
		info!("Would stage files:");
		for file in &all_files {
			info!("  {}", file.display());
		}
		return Ok(());
	}

	// Stage and commit
	git.add(&all_files)
		.context("Failed to stage files for git commit")?;
	git.commit(&commit_message)
		.context("Failed to create git commit")?;

	Ok(())
}

/// Builds the pull request body listing each released package and its new version.
fn build_pr_body(releases: &[ReleaseInfo]) -> String {
	let items: Vec<String> = releases
		.iter()
		.map(|r| format!("- {}@{}", r.package_name, r.new_version))
		.collect();
	format!("Release:\n\n{}", items.join("\n"))
}

/// Runs the `prepare` subcommand.
pub(crate) fn cmd_prepare(
	git: &git::GitWorkdir<'_>,
	args: &PrepareArgs,
	config: Config,
	runner: Arc<dyn CommandRunner>,
	github_client: Option<Arc<dyn GitHubClient>>,
) -> anyhow::Result<ExitCode> {
	let adapters = config.create_adapters(Arc::clone(&runner))?;
	let projects = config.load_projects_for_adapters(&adapters)?;

	// Read all pending changesets
	let changesets = Changeset::read_all(git)?;
	if changesets.is_empty() {
		info!("No pending changesets found. Nothing to prepare.");
		return Ok(ExitCode::SUCCESS);
	}

	// Aggregate: find the maximum change type per package
	let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
	for (_, cs) in &changesets {
		for (pkg, ct) in &cs.packages {
			aggregated
				.entry(pkg.clone())
				.and_modify(|e| *e = (*e).max(*ct))
				.or_insert(*ct);
		}
	}

	// Collect changes per package for changelog: (ChangeType, Option<message>)
	let mut changes_per_package: BTreeMap<String, Vec<(ChangeType, Option<String>)>> =
		BTreeMap::new();
	for (_, cs) in &changesets {
		for (pkg, ct) in &cs.packages {
			changes_per_package
				.entry(pkg.clone())
				.or_default()
				.push((*ct, cs.message.clone()));
		}
	}

	// Filter by --package flags if specified
	if !args.packages.is_empty() {
		// Validate all requested packages exist
		filter_projects_by_name(&projects, &args.packages)?;

		// Filter aggregated and changes_per_package to only include requested packages
		aggregated.retain(|name, _| args.packages.contains(name));
		changes_per_package.retain(|name, _| args.packages.contains(name));
	}

	let git_enabled = config.git.enabled.unwrap_or(false) && !args.no_git;
	// strategy is always Some after config::load(); unwrap_or is a defensive fallback.
	let strategy = config.git.strategy.unwrap_or(Strategy::Push);

	if args.branch.is_some() && strategy == Strategy::Push {
		log::warn!("--branch has no effect with the push strategy; ignoring");
	}

	// Pre-flight: fail early if GitHub integration requires a client that was not supplied.
	if git_enabled
		&& strategy == Strategy::Branch
		&& config.github.enabled
		&& !args.dry_run
		&& github_client.is_none()
	{
		anyhow::bail!(
			"GitHub integration is enabled but no GitHub token found. \
			 Set GH_TOKEN or GITHUB_TOKEN environment variable."
		);
	}

	// Pre-flight dirty-tree check and branch strategy setup (before filesystem changes).
	let (original_branch, release_branch) = if git_enabled {
		if !args.dry_run {
			check_dirty_tree(git)?;
		}
		if strategy == Strategy::Branch {
			let current = if args.dry_run {
				git.current_branch().ok().flatten()
			} else {
				git.current_branch()?
			};
			let branch = compute_release_branch(
				args.branch.as_deref(),
				config.git.release_branch_prefix.as_deref(),
				current.as_deref(),
			);
			if !args.dry_run {
				git.checkout_new_branch(&branch)?;
			}
			(current, Some(branch))
		} else {
			(None, None)
		}
	} else {
		(None, None)
	};

	let mut release_infos: Vec<ReleaseInfo> = Vec::new();
	let mut modified_files: Vec<PathBuf> = Vec::new();

	// Process each affected package
	for (pkg_name, change_type) in &aggregated {
		let project = projects
			.iter()
			.find(|p| p.name() == pkg_name)
			.with_context(|| {
				format!("Package '{pkg_name}' from changeset not found in projects")
			})?;

		let current_version = project.version();
		let new_version = bump_version(current_version, *change_type);

		// Always track which files would be staged (used for git lifecycle and dry-run display).
		modified_files.push(project.manifest_path());
		modified_files.push(project.path().join("CHANGELOG.md"));

		if args.dry_run {
			info!("{pkg_name}: {current_version} -> {new_version} ({change_type})");
		} else {
			project.write_version(&new_version)?;

			// Generate changelog
			let changes = changes_per_package
				.get(pkg_name)
				.map(|v| v.as_slice())
				.unwrap_or_default()
				.to_vec();
			Changelog::new(
				new_version.clone(),
				today_iso_date(),
				changes,
				project.path().clone(),
			)
			.update()?;

			info!("{pkg_name}: {current_version} -> {new_version} ({change_type})");
		}

		release_infos.push(ReleaseInfo {
			package_name: pkg_name.clone(),
			new_version,
		});
	}

	// Build map of bumped package names → new versions for dependency propagation.
	let bumped_versions: BTreeMap<String, semver::Version> = release_infos
		.iter()
		.map(|info| (info.package_name.clone(), info.new_version.clone()))
		.collect();

	// Update intra-workspace dependency references for all projects.
	for project in &projects {
		for dep_name in project.dependency_names() {
			if let Some(new_version) = bumped_versions.get(dep_name.as_str()) {
				if args.dry_run {
					info!(
						"  {}: would update dependency {} to {}",
						project.name(),
						dep_name,
						new_version
					);
					// Predict the manifest that would be modified so git lifecycle
					// dry-run can report it as a file that would be staged.
					modified_files.push(project.manifest_path());
				} else {
					let paths = project.update_dependency_version(dep_name, new_version)?;
					modified_files.extend(paths);
				}
			}
		}
	}

	// Collect lock file paths. During dry-run, use lock_file_path() to predict which
	// file would be updated without running the update command.
	for adapter in &adapters {
		if args.dry_run {
			if let Some(lock_path) = adapter.lock_file_path() {
				modified_files.push(lock_path);
			}
		} else if let Some(lock_path) = adapter.update_lock_file()? {
			modified_files.push(lock_path);
		}
	}

	// Consume changesets: delete fully consumed, rewrite partially consumed.
	// Always track which changesets would be staged, but only consume during a real release.
	let released: BTreeSet<String> = aggregated.keys().cloned().collect();
	for (path, cs) in &changesets {
		// Only stage changesets that touch at least one released package
		if cs.packages.keys().any(|name| released.contains(name)) {
			modified_files.push(path.clone());
		}
		if !args.dry_run {
			cs.consume(path, &released)?;
		}
	}

	// Deduplicate modified files (e.g. workspace root Cargo.toml updated by multiple projects)
	modified_files.sort();
	modified_files.dedup();

	// Stage and commit
	if git_enabled {
		stage_and_commit(
			git,
			&config.git.extra_files,
			&release_infos,
			&modified_files,
			args.dry_run,
		)?;

		// Strategy push dispatch
		match strategy {
			Strategy::Push => {
				if args.dry_run {
					info!("Would push to origin");
				} else {
					git.push()?;
				}
			}
			Strategy::Branch => {
				if let Some(ref branch) = release_branch {
					if args.dry_run {
						info!("Would push branch '{branch}' to origin");
						if config.github.enabled {
							let title = config
								.github
								.pull_request_title
								.as_deref()
								.unwrap_or(DEFAULT_PR_TITLE);
							info!("Would create pull request: '{title}'");
						}
					} else {
						git.push_branch(branch).with_context(|| {
							format!(
								"Failed to push release branch '{branch}'. \
									 You are still on the release branch; run \
									 `git checkout <your-branch>` to return."
							)
						})?;

						// PR creation is non-fatal; warn on failure.
						// github_client is guaranteed Some by the pre-flight check above.
						if config.github.enabled
							&& let Some(ref client) = github_client
						{
							let base = original_branch.as_deref().unwrap_or_else(|| {
								log::warn!(
									"HEAD is detached; using \"main\" as the PR base branch."
								);
								"main"
							});
							match GitHubRepo::resolve(&config.github, git) {
								Ok(gh_repo) => {
									let title = config
										.github
										.pull_request_title
										.as_deref()
										.unwrap_or(DEFAULT_PR_TITLE);
									let pr_body = build_pr_body(&release_infos);
									match client.create_pull_request(
										&gh_repo, title, &pr_body, branch, base,
									) {
										Ok(url) => info!("Created pull request: {url}"),
										Err(e) => {
											log::warn!("Failed to create pull request: {e:#}")
										}
									}
								}
								Err(e) => log::warn!(
									"Could not resolve GitHub repository for PR creation: {e:#}"
								),
							}
						}
					}
				}
				if args.dry_run {
					match &original_branch {
						Some(orig) => info!("Would return to branch '{orig}'"),
						None => {
							info!("HEAD is detached; would remain on release branch after push")
						}
					}
				} else if let Some(ref orig) = original_branch {
					git.checkout(orig)?;
				}
			}
		}
	}

	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use crate::command::test_support::RecordingCommandRunner;
	use crate::model::config;

	use super::*;

	fn make_runner() -> Arc<dyn CommandRunner> {
		Arc::new(RecordingCommandRunner::new(0))
	}

	fn no_github() -> Option<Arc<dyn GitHubClient>> {
		None
	}

	// ── normalize_path ────────────────────────────────────────────────────────

	#[test]
	fn normalize_path_collapses_parent_dir() {
		let p = std::path::Path::new("/repo/foo/../bar");
		assert_eq!(normalize_path(p), std::path::Path::new("/repo/bar"));
	}

	#[test]
	fn normalize_path_collapses_current_dir() {
		let p = std::path::Path::new("/repo/./foo");
		assert_eq!(normalize_path(p), std::path::Path::new("/repo/foo"));
	}

	// ── format_commit_message ─────────────────────────────────────────────────

	#[test]
	fn format_commit_message_single_package() {
		let infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
		}];
		assert_eq!(
			format_commit_message(&infos),
			"chore(release): my-pkg@1.0.0"
		);
	}

	#[test]
	fn format_commit_message_multiple_packages() {
		let infos = vec![
			ReleaseInfo {
				package_name: "pkg-a".to_string(),
				new_version: "1.0.0".parse().unwrap(),
			},
			ReleaseInfo {
				package_name: "pkg-b".to_string(),
				new_version: "2.1.0".parse().unwrap(),
			},
		];
		assert_eq!(
			format_commit_message(&infos),
			"chore(release): pkg-a@1.0.0, pkg-b@2.1.0"
		);
	}

	#[test]
	fn format_commit_message_empty() {
		assert_eq!(format_commit_message(&[]), "chore(release): ");
	}

	// ── stage_and_commit ──────────────────────────────────────────────────────

	#[test]
	fn stage_and_commit_empty_releases_is_noop() {
		let dir = tempfile::tempdir().unwrap();
		let runner = RecordingCommandRunner::new(0);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&runner, &dir_abs);
		let result = stage_and_commit(&git, &[], &[], &[], false);
		assert!(result.is_ok());
	}

	#[test]
	fn stage_and_commit_dry_run_prints_summary() {
		let dir = tempfile::tempdir().unwrap();
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
		}];
		let runner = RecordingCommandRunner::new(0);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&runner, &dir_abs);
		let result = stage_and_commit(&git, &[], &release_infos, &[], true);
		assert!(result.is_ok());
	}

	#[test]
	fn extra_files_outside_repo_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		let extra_files = vec!["../../etc/passwd".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
		}];
		let runner = RecordingCommandRunner::new(0);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&runner, &dir_abs);
		let result = stage_and_commit(&git, &extra_files, &release_infos, &[], true);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[test]
	fn extra_files_absolute_path_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		let extra_files = vec!["/etc/passwd".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
		}];
		let runner = RecordingCommandRunner::new(0);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&runner, &dir_abs);
		let result = stage_and_commit(&git, &extra_files, &release_infos, &[], true);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[test]
	fn bump_version_major() {
		let v = "1.2.3".parse().unwrap();
		assert_eq!(bump_version(&v, ChangeType::Major).to_string(), "2.0.0");
	}

	#[test]
	fn bump_version_minor() {
		let v = "1.2.3".parse().unwrap();
		assert_eq!(bump_version(&v, ChangeType::Minor).to_string(), "1.3.0");
	}

	#[test]
	fn bump_version_patch() {
		let v = "1.2.3".parse().unwrap();
		assert_eq!(bump_version(&v, ChangeType::Patch).to_string(), "1.2.4");
	}

	#[test]
	fn bump_version_clears_prerelease() {
		let v = "1.0.0-alpha.1".parse().unwrap();
		assert_eq!(bump_version(&v, ChangeType::Patch).to_string(), "1.0.1");
	}

	#[test]
	fn bump_version_major_resets_minor_and_patch() {
		let v = "1.5.9".parse().unwrap();
		let bumped = bump_version(&v, ChangeType::Major);
		assert_eq!(bumped.to_string(), "2.0.0");
	}

	#[test]
	fn bump_version_minor_resets_patch() {
		let v = "1.5.9".parse().unwrap();
		let bumped = bump_version(&v, ChangeType::Minor);
		assert_eq!(bumped.to_string(), "1.6.0");
	}

	#[test]
	fn compute_release_branch_uses_flag_over_all() {
		assert_eq!(
			compute_release_branch(Some("my-branch"), Some("release/"), Some("main")),
			"my-branch"
		);
	}

	#[test]
	fn compute_release_branch_uses_config_prefix() {
		assert_eq!(
			compute_release_branch(None, Some("release/"), Some("main")),
			"release/main"
		);
	}

	#[test]
	fn compute_release_branch_uses_default_prefix() {
		assert_eq!(
			compute_release_branch(None, None, Some("main")),
			"chronicle-release/main"
		);
	}

	#[test]
	fn compute_release_branch_detached_fallback() {
		assert_eq!(
			compute_release_branch(None, None, None),
			"chronicle-release/detached"
		);
	}

	#[test]
	fn check_dirty_tree_succeeds_when_clean() {
		let dir = tempfile::tempdir().unwrap();
		let runner = RecordingCommandRunner::new(0); // empty stdout → clean
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&runner, &dir_abs);
		let result = check_dirty_tree(&git);
		assert!(result.is_ok());
	}

	#[test]
	fn check_dirty_tree_fails_when_dirty() {
		let dir = tempfile::tempdir().unwrap();
		let runner = RecordingCommandRunner::new(0).with_stdout(b" M src/main.rs\n".to_vec());
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&runner, &dir_abs);
		let result = check_dirty_tree(&git);
		assert!(result.is_err());
		assert!(
			result.unwrap_err().to_string().contains("dirty"),
			"Expected 'dirty' in error message"
		);
	}

	#[test]
	fn cmd_prepare_no_changesets_succeeds() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::package_manager::CargoConfig::enabled());
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();

		let config = config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let args = PrepareArgs::default();
		let runner = make_runner();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(runner.as_ref(), &dir_abs);
		let result = cmd_prepare(&git, &args, config, Arc::clone(&runner), no_github()).unwrap();
		assert_eq!(result, ExitCode::SUCCESS);
	}

	#[test]
	fn cmd_prepare_unknown_package_in_changeset_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::package_manager::CargoConfig::enabled());
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"real-project\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();
		// Changeset references a package that doesn't exist
		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::write(
			chronicle_dir.join("test.md"),
			"+++\nnonexistent-package = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let config = config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let args = PrepareArgs::default();
		let runner = make_runner();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(runner.as_ref(), &dir_abs);
		let result = cmd_prepare(&git, &args, config, Arc::clone(&runner), no_github());
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("not found in projects")
		);
	}

	/// Sets up a temporary Cargo workspace with `pkg-a` (0.1.0) and `pkg-b` (0.2.0).
	fn setup_two_package_workspace() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::package_manager::CargoConfig::enabled());
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[workspace]\nmembers = [\"pkg-a\", \"pkg-b\"]\n",
		)
		.unwrap();
		std::fs::create_dir_all(dir.path().join("pkg-a/src")).unwrap();
		std::fs::write(
			dir.path().join("pkg-a/Cargo.toml"),
			"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
		)
		.unwrap();
		std::fs::write(dir.path().join("pkg-a/src/lib.rs"), "").unwrap();
		std::fs::create_dir_all(dir.path().join("pkg-b/src")).unwrap();
		std::fs::write(
			dir.path().join("pkg-b/Cargo.toml"),
			"[package]\nname = \"pkg-b\"\nversion = \"0.2.0\"\nedition = \"2024\"\n",
		)
		.unwrap();
		std::fs::write(dir.path().join("pkg-b/src/lib.rs"), "").unwrap();
		dir
	}

	#[test]
	fn cmd_prepare_package_flag_filters_packages() {
		let dir = setup_two_package_workspace();

		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		let changeset_path = chronicle_dir.join("test.md");
		std::fs::write(
			&changeset_path,
			"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let config = config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let args = PrepareArgs {
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let runner = make_runner();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(runner.as_ref(), &dir_abs);
		let result = cmd_prepare(&git, &args, config, Arc::clone(&runner), no_github());
		assert!(result.is_ok());

		// Changeset should be rewritten with only pkg-b remaining
		assert!(
			changeset_path.exists(),
			"Changeset should still exist (partially consumed)"
		);
		let content = std::fs::read_to_string(&changeset_path).unwrap();
		assert!(
			content.contains("pkg-b = \"minor\""),
			"pkg-b should remain in changeset, got: {content}"
		);
		assert!(
			!content.contains("pkg-a"),
			"pkg-a should be removed from changeset, got: {content}"
		);
	}

	#[test]
	fn cmd_prepare_package_flag_with_dry_run_leaves_changeset_untouched() {
		let dir = setup_two_package_workspace();

		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		let changeset_path = chronicle_dir.join("test.md");
		let original = "+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n";
		std::fs::write(&changeset_path, original).unwrap();

		let config = config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let args = PrepareArgs {
			dry_run: true,
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let runner = make_runner();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(runner.as_ref(), &dir_abs);
		let result = cmd_prepare(&git, &args, config, Arc::clone(&runner), no_github());
		assert!(result.is_ok());

		// Dry-run must not touch the changeset even when scoped
		let content = std::fs::read_to_string(&changeset_path).unwrap();
		assert_eq!(
			content, original,
			"Changeset should be untouched in dry-run"
		);
	}

	#[test]
	fn cmd_prepare_unknown_package_flag_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::package_manager::CargoConfig::enabled());
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"real-project\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();

		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::write(
			chronicle_dir.join("test.md"),
			"+++\nreal-project = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let config = config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let args = PrepareArgs {
			packages: vec!["nonexistent".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let runner = make_runner();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(runner.as_ref(), &dir_abs);
		let result = cmd_prepare(&git, &args, config, Arc::clone(&runner), no_github());
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("Unknown package: nonexistent")
		);
	}

	#[test]
	fn build_pr_body_empty_releases() {
		assert_eq!(build_pr_body(&[]), "Release:\n\n");
	}

	#[test]
	fn build_pr_body_formats_single_release() {
		let releases = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.2.0".parse().unwrap(),
		}];
		assert_eq!(build_pr_body(&releases), "Release:\n\n- my-pkg@1.2.0");
	}

	#[test]
	fn build_pr_body_formats_multiple_releases() {
		let releases = vec![
			ReleaseInfo {
				package_name: "pkg-a".to_string(),
				new_version: "1.0.0".parse().unwrap(),
			},
			ReleaseInfo {
				package_name: "pkg-b".to_string(),
				new_version: "2.1.0".parse().unwrap(),
			},
		];
		assert_eq!(
			build_pr_body(&releases),
			"Release:\n\n- pkg-a@1.0.0\n- pkg-b@2.1.0"
		);
	}

	/// Sets up a temp dir with a Cargo project, branch strategy git config, and GitHub config.
	fn setup_branch_strategy_with_github() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::package_manager::CargoConfig::enabled())
				.with_git(crate::git::GitConfig {
					enabled: Some(true),
					strategy: Some(crate::git::Strategy::Branch),
					..Default::default()
				})
				.with_github(crate::github::GitHubConfig {
					enabled: true,
					owner: Some("acme".to_string()),
					repo: Some("app".to_string()),
					pull_request_title: Some("My Release PR".to_string()),
					..Default::default()
				});
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"test-pkg\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
		)
		.unwrap();
		std::fs::create_dir_all(dir.path().join("src")).unwrap();
		std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::write(
			chronicle_dir.join("change.md"),
			"+++\ntest-pkg = \"patch\"\n+++\n\nFix\n",
		)
		.unwrap();
		dir
	}

	#[test]
	fn cmd_prepare_branch_strategy_with_github_creates_pr() {
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let client = Arc::new(RecordingGitHubClient::new());
		let config = config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let args = PrepareArgs::default();

		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(runner.as_ref(), &dir_abs);
		let result = cmd_prepare(
			&git,
			&args,
			config,
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Some(Arc::clone(&client) as Arc<dyn GitHubClient>),
		);
		assert!(result.is_ok(), "Expected Ok, got: {result:?}");

		let invocations = client.invocations();
		let pr = invocations
			.iter()
			.find(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. }));
		assert!(pr.is_some(), "Expected PR creation, got: {invocations:?}");
		if let Some(GitHubInvocation::CreatePullRequest { title, gh_repo, .. }) = pr {
			assert_eq!(title, "My Release PR");
			assert_eq!(gh_repo.owner, "acme");
			assert_eq!(gh_repo.repo, "app");
		}
	}

	#[test]
	fn cmd_prepare_branch_strategy_pr_failure_is_nonfatal() {
		use crate::github::client::test_support::RecordingGitHubClient;
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let client = Arc::new(RecordingGitHubClient::new().with_create_pr_failure());
		let config = config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let args = PrepareArgs::default();

		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(runner.as_ref(), &dir_abs);
		let result = cmd_prepare(
			&git,
			&args,
			config,
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Some(Arc::clone(&client) as Arc<dyn GitHubClient>),
		);
		// PR failure is non-fatal — command should still succeed
		assert!(
			result.is_ok(),
			"PR failure should be non-fatal, got: {result:?}"
		);
	}

	#[test]
	fn cmd_prepare_no_github_client_errors() {
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		// No github client — pre-flight check should error
		let config = config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let args = PrepareArgs::default();

		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(runner.as_ref(), &dir_abs);
		let result = cmd_prepare(
			&git,
			&args,
			config,
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			no_github(),
		);
		assert!(result.is_err(), "Expected Err without github client");
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("no GitHub token"),
			"Expected 'no GitHub token' error, got: {msg}"
		);
	}
}
