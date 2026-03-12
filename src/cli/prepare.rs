//! The `prepare` subcommand.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use clap::Args;
use log::info;

use semver::Version;

use crate::git;
use crate::github::GitHubRepo;
use crate::github::client::GitHubClient;
use crate::model::changelog::Changelog;
use crate::model::changeset::{ChangeType, Changeset};
use crate::model::config::{Config, Strategy};
use crate::package_manager::{PackageManagerAdapter, filter_projects_by_name};
use crate::utils::today_iso_date;

/// Arguments for the `prepare` subcommand.
#[derive(Args, Default)]
pub struct PrepareArgs {
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
fn check_dirty_tree(git: &git::GitWorkdir) -> anyhow::Result<()> {
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
/// 3. `{config_prefix}detached` — fallback when HEAD is detached
fn compute_release_branch(
	args_branch: Option<&str>,
	config_prefix: &str,
	current_branch: Option<&str>,
) -> String {
	if let Some(branch) = args_branch {
		return branch.to_string();
	}
	let base = current_branch.unwrap_or("detached");
	format!("{config_prefix}{base}")
}

/// Information about a single package prepared for release.
#[derive(Debug)]
struct ReleaseInfo {
	package_name: String,
	new_version: Version,
	/// Formatted changelog sections (### headings + bullets) without the version heading.
	changelog_entry: String,
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
	git: &git::GitWorkdir,
	extra_files: &[String],
	release_infos: &[ReleaseInfo],
	modified_files: &[PathBuf],
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

	git.add(&all_files)
		.context("Failed to stage files for git commit")?;
	git.commit(&commit_message)
		.context("Failed to create git commit")?;

	Ok(())
}

/// Creates or updates a pull request for the given head branch.
///
/// If an open pull request already exists for `head`, it is updated with the
/// new `title` and `body`. Otherwise a new pull request is created from `head`
/// into `base`. Returns the URL of the created or updated pull request.
///
/// # Errors
///
/// Returns an error if the find, create, or update API call fails.
fn upsert_pull_request(
	client: &dyn GitHubClient,
	gh_repo: &GitHubRepo,
	title: &str,
	body: &str,
	head: &str,
	base: &str,
) -> anyhow::Result<String> {
	match client.find_open_pull_request(gh_repo, head)? {
		Some(pr) => {
			let url = client.update_pull_request(gh_repo, pr.number, title, body)?;
			info!("Updated pull request: {url}");
			Ok(url)
		}
		None => {
			let url = client.create_pull_request(gh_repo, title, body, head, base)?;
			info!("Created pull request: {url}");
			Ok(url)
		}
	}
}

/// Builds the pull request body with an introduction and per-package changelog sections.
fn build_pr_body(releases: &[ReleaseInfo], base_branch: &str) -> String {
	let mut body = format!(
		"This PR was opened by Chronicle. When ready to release, you should merge this PR \
		 which will trigger a release. If you're not ready to do a release then simply leave \
		 this PR and it will be updated as you merge more changesets into `{base_branch}`.\n\
		 \n\
		 # Releases\n"
	);
	for r in releases {
		let _ = write!(
			body,
			"\n## {}@{}\n\n{}",
			r.package_name, r.new_version, r.changelog_entry
		);
	}
	body
}

/// Per-package changelog entries: `(ChangeType, Option<message>)` pairs.
type PackageChanges = Vec<(ChangeType, Option<String>)>;

/// Aggregates changeset data into per-package maps, applying optional package filters.
///
/// Returns a tuple of:
/// - `aggregated`: the maximum `ChangeType` per package name
/// - `changes_per_package`: all `(ChangeType, message)` pairs per package name, for changelog
fn aggregate_changesets(
	changesets: &[(PathBuf, Changeset)],
	package_filter: &[String],
	projects: &[crate::package_manager::Project],
) -> anyhow::Result<(
	BTreeMap<String, ChangeType>,
	BTreeMap<String, PackageChanges>,
)> {
	let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
	for (_, cs) in changesets {
		for (pkg, ct) in &cs.packages {
			aggregated
				.entry(pkg.clone())
				.and_modify(|e| *e = (*e).max(*ct))
				.or_insert(*ct);
		}
	}
	let mut changes_per_package: BTreeMap<String, Vec<(ChangeType, Option<String>)>> =
		BTreeMap::new();
	for (_, cs) in changesets {
		for (pkg, ct) in &cs.packages {
			changes_per_package
				.entry(pkg.clone())
				.or_default()
				.push((*ct, cs.message.clone()));
		}
	}
	if !package_filter.is_empty() {
		filter_projects_by_name(projects, package_filter)?;
		aggregated.retain(|name, _| package_filter.contains(name));
		changes_per_package.retain(|name, _| package_filter.contains(name));
	}
	Ok((aggregated, changes_per_package))
}

/// Runs pre-release checks and sets up the git branch if needed.
///
/// Validates GitHub token availability when required, checks for a dirty working
/// tree, and checks out the release branch for the branch strategy.
/// Returns `(original_branch, release_branch)`.
fn preflight_checks(
	git: &git::GitWorkdir,
	config: &Config,
	env: &crate::Env,
	args: &PrepareArgs,
	git_enabled: bool,
	strategy: Strategy,
	dry_run: bool,
) -> anyhow::Result<(Option<String>, Option<String>)> {
	if git_enabled
		&& strategy == Strategy::Branch
		&& config.github.enabled
		&& !dry_run
		&& env.github_client().is_none()
	{
		anyhow::bail!(
			"GitHub integration is enabled but no GitHub token found. \
			 Set GH_TOKEN or GITHUB_TOKEN environment variable."
		);
	}
	if !git_enabled {
		return Ok((None, None));
	}
	if !dry_run {
		check_dirty_tree(git)?;
	}
	if strategy == Strategy::Branch {
		let current = if dry_run {
			git.current_branch().ok().flatten()
		} else {
			git.current_branch()?
		};
		let branch = compute_release_branch(
			args.branch.as_deref(),
			config.git.release_branch_prefix(),
			current.as_deref(),
		);
		git.checkout_or_reset_branch(&branch)?;
		Ok((current, Some(branch)))
	} else {
		Ok((None, None))
	}
}

/// Bumps versions and generates changelog entries for all affected packages.
///
/// Returns a tuple of `(release_infos, modified_files)` where `release_infos` describes
/// each package prepared for release and `modified_files` is the list of paths modified.
fn bump_versions_and_generate_changelogs(
	aggregated: &BTreeMap<String, ChangeType>,
	changes_per_package: &BTreeMap<String, PackageChanges>,
	projects: &[crate::package_manager::Project],
	dry_run: bool,
) -> anyhow::Result<(Vec<ReleaseInfo>, Vec<PathBuf>)> {
	let mut release_infos: Vec<ReleaseInfo> = Vec::new();
	let mut modified_files: Vec<PathBuf> = Vec::new();
	for (pkg_name, change_type) in aggregated {
		let project = projects
			.iter()
			.find(|p| p.name() == pkg_name)
			.with_context(|| {
				format!("Package '{pkg_name}' from changeset not found in projects")
			})?;
		let current_version = project.version();
		let new_version = bump_version(current_version, *change_type);
		modified_files.push(project.manifest_path());
		modified_files.push(project.path().join("CHANGELOG.md"));
		let changes = changes_per_package
			.get(pkg_name)
			.cloned()
			.unwrap_or_default();
		let changelog = Changelog::new(
			new_version.clone(),
			today_iso_date(),
			changes,
			project.path().clone(),
		);
		let changelog_entry = changelog.format_sections();
		project.write_version(&new_version, dry_run)?;
		changelog.update(dry_run)?;
		info!("{pkg_name}: {current_version} -> {new_version} ({change_type})");
		release_infos.push(ReleaseInfo {
			package_name: pkg_name.clone(),
			new_version,
			changelog_entry,
		});
	}
	Ok((release_infos, modified_files))
}

/// Updates intra-workspace dependency references for all projects.
///
/// For each project that depends on a bumped package, calls `update_dependency_version`
/// and returns the list of modified manifest paths.
fn propagate_dependency_updates(
	projects: &[crate::package_manager::Project],
	release_infos: &[ReleaseInfo],
	dry_run: bool,
) -> anyhow::Result<Vec<PathBuf>> {
	let bumped_versions: BTreeMap<String, semver::Version> = release_infos
		.iter()
		.map(|info| (info.package_name.clone(), info.new_version.clone()))
		.collect();
	let update_verb = if dry_run { "would update" } else { "update" };
	let mut additional_files: Vec<PathBuf> = Vec::new();
	for project in projects {
		for dep_name in project.dependency_names() {
			let Some(new_version) = bumped_versions.get(dep_name.as_str()) else {
				continue;
			};
			let paths = project.update_dependency_version(dep_name, new_version, dry_run)?;
			if !paths.is_empty() {
				info!(
					"  {}: {update_verb} dependency {} to {}",
					project.name(),
					dep_name,
					new_version
				);
				additional_files.extend(paths);
			}
		}
	}
	Ok(additional_files)
}

/// Runs `update_lock_file` on all adapters and collects the resulting paths.
fn update_lock_files(adapters: &[Arc<dyn PackageManagerAdapter>]) -> anyhow::Result<Vec<PathBuf>> {
	let mut files: Vec<PathBuf> = Vec::new();
	for adapter in adapters {
		if let Some(path) = adapter.update_lock_file()? {
			files.push(path);
		}
	}
	Ok(files)
}

/// Consumes or dry-runs the given changeset files for the released packages.
///
/// Returns the list of changeset paths that would be (or were) modified or deleted.
fn consume_changesets(
	changesets: &[(PathBuf, Changeset)],
	released: &BTreeSet<String>,
	dry_run: bool,
) -> anyhow::Result<Vec<PathBuf>> {
	let mut additional_files: Vec<PathBuf> = Vec::new();
	for (path, cs) in changesets {
		let released_pkgs: Vec<&String> = cs
			.packages
			.keys()
			.filter(|name| released.contains(*name))
			.collect();
		if !released_pkgs.is_empty() {
			additional_files.push(path.clone());
		}
		if dry_run {
			if !released_pkgs.is_empty() {
				let pkg_list = released_pkgs
					.iter()
					.map(|s| s.as_str())
					.collect::<Vec<_>>()
					.join(", ");
				info!("Would consume changeset {}: {pkg_list}", path.display());
			}
		} else {
			cs.consume(path, released)?;
		}
	}
	Ok(additional_files)
}

/// Creates or updates the GitHub pull request for the release branch.
///
/// No-ops in dry-run mode or when no GitHub client is available. The dry-run
/// short-circuit is intentional per ADR-017: the PR upsert is the side-effecting
/// operation being guarded, so the check lives here rather than at the call site.
fn upsert_release_pull_request(
	git: &git::GitWorkdir,
	config: &Config,
	env: &crate::Env,
	release_infos: &[ReleaseInfo],
	branch: &str,
	original_branch: Option<&str>,
	dry_run: bool,
) -> anyhow::Result<()> {
	if dry_run {
		info!("Would attempt to create or update a PR in GitHub.");
		return Ok(());
	}
	let Some(client) = env.github_client() else {
		return Ok(());
	};
	let base = original_branch.unwrap_or_else(|| {
		log::warn!("HEAD is detached; using \"main\" as the PR base branch.");
		"main"
	});
	match GitHubRepo::resolve(&config.github, git) {
		Ok(gh_repo) => {
			let title = config.github.pull_request_title();
			let pr_body = build_pr_body(release_infos, base);
			if let Err(e) = upsert_pull_request(client, &gh_repo, title, &pr_body, branch, base) {
				log::warn!("Failed to create or update pull request: {e:#}");
			}
		}
		Err(e) => {
			log::warn!("Could not resolve GitHub repository for PR creation: {e:#}");
		}
	}
	Ok(())
}

/// Stages, commits, and pushes release changes according to the configured git strategy.
#[allow(clippy::too_many_arguments)]
fn finalize_git_lifecycle(
	git: &git::GitWorkdir,
	config: &Config,
	env: &crate::Env,
	release_infos: &[ReleaseInfo],
	modified_files: &[PathBuf],
	original_branch: Option<&str>,
	release_branch: Option<&str>,
	git_enabled: bool,
	strategy: Strategy,
	dry_run: bool,
) -> anyhow::Result<()> {
	if !git_enabled {
		return Ok(());
	}
	stage_and_commit(git, &config.git.extra_files, release_infos, modified_files)?;
	match strategy {
		Strategy::Push => {
			git.push()?;
		}
		Strategy::Branch => {
			if let Some(branch) = release_branch {
				info!("Pushing branch '{branch}' to origin");
				git.force_push_branch(branch).with_context(|| {
					format!(
						"Failed to push release branch '{branch}'. \
						 You are still on the release branch; run \
						 `git checkout <your-branch>` to return."
					)
				})?;
				if config.github.enabled {
					upsert_release_pull_request(
						git,
						config,
						env,
						release_infos,
						branch,
						original_branch,
						dry_run,
					)?;
				}
			}
			if let Some(orig) = original_branch {
				git.checkout(orig)?;
			}
		}
	}
	Ok(())
}

/// Runs the `prepare` subcommand.
pub(crate) fn cmd_prepare(
	git: &git::GitWorkdir,
	args: &PrepareArgs,
	dry_run: bool,
	config: Config,
) -> anyhow::Result<ExitCode> {
	let env = config.env().context("env not set")?;
	let adapters = config.create_adapters()?;
	let projects = config.load_projects_for_adapters(&adapters)?;

	let changesets = Changeset::read_all(git)?;
	if changesets.is_empty() {
		info!("No pending changesets found. Nothing to prepare.");
		return Ok(ExitCode::SUCCESS);
	}

	let (aggregated, changes_per_package) =
		aggregate_changesets(&changesets, &args.packages, &projects)?;

	let git_enabled = config.git.enabled() && !args.no_git;
	let strategy = config.git.strategy();
	if args.branch.is_some() && strategy == Strategy::Push {
		log::warn!("--branch has no effect with the push strategy; ignoring");
	}

	let (original_branch, release_branch) =
		preflight_checks(git, &config, env, args, git_enabled, strategy, dry_run)?;

	let (release_infos, mut modified_files) = bump_versions_and_generate_changelogs(
		&aggregated,
		&changes_per_package,
		&projects,
		dry_run,
	)?;
	modified_files.extend(propagate_dependency_updates(
		&projects,
		&release_infos,
		dry_run,
	)?);
	modified_files.extend(update_lock_files(&adapters)?);
	let released: BTreeSet<String> = aggregated.keys().cloned().collect();
	modified_files.extend(consume_changesets(&changesets, &released, dry_run)?);
	modified_files.sort();
	modified_files.dedup();

	finalize_git_lifecycle(
		git,
		&config,
		env,
		&release_infos,
		&modified_files,
		original_branch.as_deref(),
		release_branch.as_deref(),
		git_enabled,
		strategy,
		dry_run,
	)?;

	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::model::config;

	use super::*;

	fn make_runner() -> Arc<dyn CommandRunner> {
		Arc::new(RecordingCommandRunner::new(0))
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
			changelog_entry: String::new(),
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
				changelog_entry: String::new(),
			},
			ReleaseInfo {
				package_name: "pkg-b".to_string(),
				new_version: "2.1.0".parse().unwrap(),
				changelog_entry: String::new(),
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
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = stage_and_commit(&git, &[], &[], &[]);
		assert!(result.is_ok());
	}

	#[test]
	fn stage_and_commit_dry_run_suppresses_git_commands() {
		let dir = tempfile::tempdir().unwrap();
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let dry_run_runner =
			crate::command::DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::new(dry_run_runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = stage_and_commit(&git, &[], &release_infos, &[]);
		assert!(result.is_ok());
		// DryRunCommandRunner suppresses run_mut calls — the inner recorder receives nothing.
		assert!(inner.invocations().is_empty());
	}

	#[test]
	fn extra_files_outside_repo_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		let extra_files = vec!["../../etc/passwd".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = stage_and_commit(&git, &extra_files, &release_infos, &[]);
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
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = stage_and_commit(&git, &extra_files, &release_infos, &[]);
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
			compute_release_branch(Some("my-branch"), "release/", Some("main")),
			"my-branch"
		);
	}

	#[test]
	fn compute_release_branch_uses_config_prefix() {
		assert_eq!(
			compute_release_branch(None, "release/", Some("main")),
			"release/main"
		);
	}

	#[test]
	fn compute_release_branch_uses_default_prefix() {
		assert_eq!(
			compute_release_branch(None, "chronicle-release/", Some("main")),
			"chronicle-release/main"
		);
	}

	#[test]
	fn compute_release_branch_detached_fallback() {
		assert_eq!(
			compute_release_branch(None, "chronicle-release/", None),
			"chronicle-release/detached"
		);
	}

	#[test]
	fn check_dirty_tree_succeeds_when_clean() {
		let dir = tempfile::tempdir().unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0)); // empty stdout → clean
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = check_dirty_tree(&git);
		assert!(result.is_ok());
	}

	#[test]
	fn check_dirty_tree_fails_when_dirty() {
		let dir = tempfile::tempdir().unwrap();
		let runner =
			Arc::new(RecordingCommandRunner::new(0).with_stdout(b" M src/main.rs\n".to_vec()));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
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
				.with_cargo(crate::model::config::CargoConfig::enabled());
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();

		let args = PrepareArgs::default();
		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, false, config).unwrap();
		assert_eq!(result, ExitCode::SUCCESS);
	}

	#[test]
	fn cmd_prepare_unknown_package_in_changeset_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::model::config::CargoConfig::enabled());
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

		let args = PrepareArgs::default();
		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, false, config);
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
				.with_cargo(crate::model::config::CargoConfig::enabled());
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

		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs {
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, false, config);
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

		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs {
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, true, config);
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
				.with_cargo(crate::model::config::CargoConfig::enabled());
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

		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs {
			packages: vec!["nonexistent".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, false, config);
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
		let body = build_pr_body(&[], "main");
		assert!(body.contains("# Releases"));
		assert!(body.contains("`main`"));
		assert!(body.contains("Chronicle"));
	}

	#[test]
	fn build_pr_body_formats_single_release() {
		let releases = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.2.0".parse().unwrap(),
			changelog_entry: "### Features\n\n- Added something\n".to_string(),
		}];
		let body = build_pr_body(&releases, "main");
		assert!(body.contains("## my-pkg@1.2.0"));
		assert!(body.contains("### Features"));
		assert!(body.contains("- Added something"));
		assert!(body.contains("`main`"));
	}

	#[test]
	fn build_pr_body_formats_multiple_releases() {
		let releases = vec![
			ReleaseInfo {
				package_name: "pkg-a".to_string(),
				new_version: "1.0.0".parse().unwrap(),
				changelog_entry: "### Bug Fixes\n\n- Fixed a bug\n".to_string(),
			},
			ReleaseInfo {
				package_name: "pkg-b".to_string(),
				new_version: "2.1.0".parse().unwrap(),
				changelog_entry: String::new(),
			},
		];
		let body = build_pr_body(&releases, "develop");
		assert!(body.contains("## pkg-a@1.0.0"));
		assert!(body.contains("### Bug Fixes"));
		assert!(body.contains("- Fixed a bug"));
		assert!(body.contains("## pkg-b@2.1.0"));
		assert!(body.contains("`develop`"));
		// pkg-a section must appear before pkg-b
		let pos_a = body.find("## pkg-a").unwrap();
		let pos_b = body.find("## pkg-b").unwrap();
		assert!(pos_a < pos_b);
	}

	#[test]
	fn build_pr_body_includes_base_branch_in_intro() {
		let body = build_pr_body(&[], "my-feature-branch");
		assert!(body.contains("`my-feature-branch`"));
	}

	#[test]
	fn build_pr_body_snapshot() {
		let releases = vec![
			ReleaseInfo {
				package_name: "pkg-a".to_string(),
				new_version: "2.0.0".parse().unwrap(),
				changelog_entry: "### Breaking Changes\n\n- Removed old API\n".to_string(),
			},
			ReleaseInfo {
				package_name: "pkg-b".to_string(),
				new_version: "1.3.0".parse().unwrap(),
				changelog_entry:
					"### Features\n\n- Added widget\n\n### Bug Fixes\n\n- Fixed crash\n".to_string(),
			},
			ReleaseInfo {
				package_name: "pkg-c".to_string(),
				new_version: "0.9.1".parse().unwrap(),
				changelog_entry: String::new(),
			},
		];
		insta::assert_snapshot!(build_pr_body(&releases, "main"));
	}

	/// Sets up a temp dir with a Cargo project, branch strategy git config, and GitHub config.
	fn setup_branch_strategy_with_github() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::model::config::CargoConfig::enabled())
				.with_git(
					crate::model::config::GitConfig::enabled_config()
						.with_strategy(crate::model::config::Strategy::Branch),
				)
				.with_github(
					crate::model::config::GitHubConfig::enabled_config()
						.with_owner("acme".to_string())
						.with_repo("app".to_string())
						.with_pull_request_title("My Release PR".to_string()),
				);
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
		use crate::github::client::GitHubClient;
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let client = Arc::new(RecordingGitHubClient::new());
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>)
			.with_github_client(Arc::clone(&client) as Arc<dyn GitHubClient>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs::default();

		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&env, dir_abs.clone());
		let result = cmd_prepare(&git, &args, false, config);
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
		use crate::github::client::GitHubClient;
		use crate::github::client::test_support::RecordingGitHubClient;
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let client = Arc::new(RecordingGitHubClient::new().with_create_pr_failure());
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>)
			.with_github_client(Arc::clone(&client) as Arc<dyn GitHubClient>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs::default();

		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&env, dir_abs.clone());
		let result = cmd_prepare(&git, &args, false, config);
		// PR failure is non-fatal — command should still succeed
		assert!(
			result.is_ok(),
			"PR failure should be non-fatal, got: {result:?}"
		);
	}

	// ── upsert_pull_request ───────────────────────────────────────────────────

	#[test]
	fn upsert_pull_request_creates_when_no_existing() {
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let client = RecordingGitHubClient::new(); // no existing PR
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"chronicle-release/main",
			"main",
		);
		assert!(result.is_ok(), "Expected Ok, got: {result:?}");
		let invocations = client.invocations();
		// Should have called find then create
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::FindOpenPullRequest { .. })),
			"Expected FindOpenPullRequest invocation"
		);
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. })),
			"Expected CreatePullRequest invocation"
		);
	}

	#[test]
	fn upsert_pull_request_updates_when_existing() {
		use crate::github::client::PullRequest;
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let existing_pr = PullRequest {
			number: 7,
			html_url: "https://github.com/acme/app/pull/7".to_string(),
		};
		let client = RecordingGitHubClient::new().with_existing_pr(existing_pr);
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"updated body",
			"chronicle-release/main",
			"main",
		);
		assert!(result.is_ok(), "Expected Ok, got: {result:?}");
		let invocations = client.invocations();
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::FindOpenPullRequest { .. })),
			"Expected FindOpenPullRequest invocation"
		);
		assert!(
			invocations.iter().any(|i| matches!(
				i,
				GitHubInvocation::UpdatePullRequest { pull_number, .. } if *pull_number == 7
			)),
			"Expected UpdatePullRequest invocation for PR #7"
		);
		assert!(
			!invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. })),
			"Should NOT call CreatePullRequest when existing PR found"
		);
	}

	#[test]
	fn upsert_pull_request_propagates_find_error() {
		use crate::github::client::test_support::RecordingGitHubClient;
		let client = RecordingGitHubClient::new().with_find_pr_failure();
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"release-branch",
			"main",
		);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("simulated find_open_pull_request failure"),
			"Expected find failure error, got: {msg}"
		);
	}

	#[test]
	fn upsert_pull_request_propagates_update_error() {
		use crate::github::client::PullRequest;
		use crate::github::client::test_support::RecordingGitHubClient;
		let existing_pr = PullRequest {
			number: 1,
			html_url: "https://github.com/acme/app/pull/1".to_string(),
		};
		let client = RecordingGitHubClient::new()
			.with_existing_pr(existing_pr)
			.with_update_pr_failure();
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"release-branch",
			"main",
		);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("simulated update_pull_request failure"),
			"Expected update failure error, got: {msg}"
		);
	}

	#[test]
	fn upsert_pull_request_propagates_create_error() {
		use crate::github::client::test_support::RecordingGitHubClient;
		let client = RecordingGitHubClient::new().with_create_pr_failure();
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"release-branch",
			"main",
		);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("simulated create_pull_request failure"),
			"Expected create failure error, got: {msg}"
		);
	}

	#[test]
	fn cmd_prepare_no_github_client_errors() {
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		// No github client — pre-flight check should error
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs::default();

		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&env, dir_abs.clone());
		let result = cmd_prepare(&git, &args, false, config);
		assert!(result.is_err(), "Expected Err without github client");
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("no GitHub token"),
			"Expected 'no GitHub token' error, got: {msg}"
		);
	}
}
