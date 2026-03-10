//! The `release` subcommand.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use clap::Args;
use log::info;

use crate::command::CommandRunner;
use crate::git::{self, ReleaseInfo, Strategy};
use crate::github::client::GitHubClient;
use crate::model::changelog::Changelog;
use crate::model::changeset::{ChangeType, Changeset};
use crate::model::config;
use crate::package_manager::filter_projects_by_name;
use crate::utils::today_iso_date;

/// Default prefix for release branches in the `branch` strategy.
const DEFAULT_RELEASE_BRANCH_PREFIX: &str = "chronicle-release/";

/// Default pull request title when none is set in config.
const DEFAULT_PR_TITLE: &str = "Release updates";

/// Arguments for the `release` subcommand.
#[derive(Args, Default)]
pub struct ReleaseArgs {
	/// Preview changes without modifying any files
	#[arg(long)]
	pub dry_run: bool,

	/// Only release specific packages (repeatable)
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
fn check_dirty_tree(runner: &dyn CommandRunner, git_workdir: &Path) -> anyhow::Result<()> {
	let status = git::git_status_porcelain(runner, git_workdir)?;
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

/// Builds the pull request body listing each released package and its new version.
fn build_pr_body(releases: &[ReleaseInfo]) -> String {
	let items: Vec<String> = releases
		.iter()
		.map(|r| format!("- {}@{}", r.package_name, r.new_version))
		.collect();
	format!("Release:\n\n{}", items.join("\n"))
}

/// Runs the `release` subcommand.
pub fn cmd_release(
	git_workdir: &Path,
	args: &ReleaseArgs,
	runner: Arc<dyn CommandRunner>,
	github_client: Option<Arc<dyn GitHubClient>>,
) -> anyhow::Result<ExitCode> {
	let config = config::load(git_workdir)?;
	let adapters = config.create_adapters(Arc::clone(&runner));
	let projects = config.load_projects_for_adapters(&adapters)?;

	// Read all pending changesets
	let changesets = Changeset::read_all(config.git_workdir())?;
	if changesets.is_empty() {
		info!("No pending changesets found. Nothing to release.");
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

	// Pre-flight dirty-tree check and branch strategy setup (before filesystem changes).
	let (original_branch, release_branch) = if git_enabled && !args.dry_run {
		check_dirty_tree(runner.as_ref(), git_workdir)?;
		if strategy == Strategy::Branch {
			let current = git::git_current_branch(runner.as_ref(), git_workdir)?;
			let branch = compute_release_branch(
				args.branch.as_deref(),
				config.git.release_branch_prefix.as_deref(),
				current.as_deref(),
			);
			git::git_checkout_new_branch(runner.as_ref(), git_workdir, &branch)?;
			(current, Some(branch))
		} else {
			(None, None)
		}
	} else if git_enabled && args.dry_run && strategy == Strategy::Branch {
		// Compute branch name for dry-run reporting; no actual git operations.
		let current = git::git_current_branch(runner.as_ref(), git_workdir)
			.ok()
			.flatten();
		let branch = compute_release_branch(
			args.branch.as_deref(),
			config.git.release_branch_prefix.as_deref(),
			current.as_deref(),
		);
		(current, Some(branch))
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
		modified_files.push(project.manifest_path(git_workdir));
		modified_files.push(git_workdir.join(project.path()).join("CHANGELOG.md"));

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
				project.path().to_path_buf(),
			)
			.update(config.git_workdir())?;

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
					modified_files.push(project.manifest_path(git_workdir));
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

	// Git lifecycle: commit + strategy push
	if git_enabled {
		git::run_git_lifecycle(
			git_workdir,
			&config.git,
			&release_infos,
			&modified_files,
			args.dry_run,
			runner.as_ref(),
		)?;

		// Strategy push dispatch
		if args.dry_run {
			match strategy {
				Strategy::Push => info!("Would push to origin"),
				Strategy::Branch => {
					if let Some(ref branch) = release_branch {
						info!("Would push branch '{branch}' to origin");
						if config.github.enabled && github_client.is_some() {
							let title = config
								.github
								.pull_request_title
								.as_deref()
								.unwrap_or(DEFAULT_PR_TITLE);
							info!("Would create pull request: '{title}'");
						}
					}
					match &original_branch {
						Some(orig) => info!("Would return to branch '{orig}'"),
						None => {
							info!("HEAD is detached; would remain on release branch after push")
						}
					}
				}
			}
		} else {
			match strategy {
				Strategy::Push => {
					git::git_push(runner.as_ref(), git_workdir)?;
				}
				Strategy::Branch => {
					if let Some(ref branch) = release_branch {
						git::git_push_branch(runner.as_ref(), git_workdir, branch).with_context(
							|| {
								format!(
									"Failed to push release branch '{branch}'. \
								 You are still on the release branch; run \
								 `git checkout <your-branch>` to return."
								)
							},
						)?;

						// PR creation is non-fatal; warn on failure.
						if config.github.enabled
							&& let Some(ref client) = github_client
						{
							let base = original_branch.as_deref().unwrap_or_else(|| {
								log::warn!(
									"HEAD is detached; using \"main\" as the PR base branch. \
								 Configure [github].default_branch if your repo uses a different name."
								);
								"main"
							});
							match crate::github::remote::resolve_github_repo(
								&config.github,
								runner.as_ref(),
								git_workdir,
							) {
								Ok((owner, repo)) => {
									let title = config
										.github
										.pull_request_title
										.as_deref()
										.unwrap_or(DEFAULT_PR_TITLE);
									let pr_body = build_pr_body(&release_infos);
									match client.create_pull_request(
										&owner, &repo, title, &pr_body, branch, base,
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
					if let Some(ref orig) = original_branch {
						git::git_checkout(runner.as_ref(), git_workdir, orig)?;
					}
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

	use super::*;

	fn make_runner() -> Arc<dyn CommandRunner> {
		Arc::new(RecordingCommandRunner::new(0))
	}

	fn no_github() -> Option<Arc<dyn GitHubClient>> {
		None
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
		let result = check_dirty_tree(&runner, dir.path());
		assert!(result.is_ok());
	}

	#[test]
	fn check_dirty_tree_fails_when_dirty() {
		let dir = tempfile::tempdir().unwrap();
		let runner = RecordingCommandRunner::new(0).with_stdout(b" M src/main.rs\n".to_vec());
		let result = check_dirty_tree(&runner, dir.path());
		assert!(result.is_err());
		assert!(
			result.unwrap_err().to_string().contains("dirty"),
			"Expected 'dirty' in error message"
		);
	}

	#[test]
	fn cmd_release_no_config_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let args = ReleaseArgs::default();
		let result = cmd_release(dir.path(), &args, make_runner(), no_github());
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("No configuration found")
		);
	}

	#[test]
	fn cmd_release_no_changesets_succeeds() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let config = crate::model::config::Config::new(dir.path())
			.with_cargo(crate::package_manager::CargoConfig::enabled());
		config.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();

		let args = ReleaseArgs::default();
		let result = cmd_release(dir.path(), &args, make_runner(), no_github()).unwrap();
		assert_eq!(result, ExitCode::SUCCESS);
	}

	#[test]
	fn cmd_release_unknown_package_in_changeset_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let config = crate::model::config::Config::new(dir.path())
			.with_cargo(crate::package_manager::CargoConfig::enabled());
		config.save().unwrap();
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

		let args = ReleaseArgs::default();
		let result = cmd_release(dir.path(), &args, make_runner(), no_github());
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
		let config = crate::model::config::Config::new(dir.path())
			.with_cargo(crate::package_manager::CargoConfig::enabled());
		config.save().unwrap();
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
	fn cmd_release_package_flag_filters_packages() {
		let dir = setup_two_package_workspace();

		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		let changeset_path = chronicle_dir.join("test.md");
		std::fs::write(
			&changeset_path,
			"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let args = ReleaseArgs {
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..ReleaseArgs::default()
		};
		let result = cmd_release(dir.path(), &args, make_runner(), no_github());
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
	fn cmd_release_package_flag_with_dry_run_leaves_changeset_untouched() {
		let dir = setup_two_package_workspace();

		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		let changeset_path = chronicle_dir.join("test.md");
		let original = "+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n";
		std::fs::write(&changeset_path, original).unwrap();

		let args = ReleaseArgs {
			dry_run: true,
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..ReleaseArgs::default()
		};
		let result = cmd_release(dir.path(), &args, make_runner(), no_github());
		assert!(result.is_ok());

		// Dry-run must not touch the changeset even when scoped
		let content = std::fs::read_to_string(&changeset_path).unwrap();
		assert_eq!(
			content, original,
			"Changeset should be untouched in dry-run"
		);
	}

	#[test]
	fn cmd_release_unknown_package_flag_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let config = crate::model::config::Config::new(dir.path())
			.with_cargo(crate::package_manager::CargoConfig::enabled());
		config.save().unwrap();
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

		let args = ReleaseArgs {
			packages: vec!["nonexistent".to_string()],
			no_git: true,
			..ReleaseArgs::default()
		};
		let result = cmd_release(dir.path(), &args, make_runner(), no_github());
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
		let config = crate::model::config::Config::new(dir.path())
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
		config.save().unwrap();
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
	fn cmd_release_branch_strategy_with_github_creates_pr() {
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let client = Arc::new(RecordingGitHubClient::new());
		let args = ReleaseArgs::default();

		let result = cmd_release(
			dir.path(),
			&args,
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Some(Arc::clone(&client) as Arc<dyn GitHubClient>),
		);
		assert!(result.is_ok(), "Expected Ok, got: {result:?}");

		let invocations = client.invocations();
		let pr = invocations
			.iter()
			.find(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. }));
		assert!(pr.is_some(), "Expected PR creation, got: {invocations:?}");
		if let Some(GitHubInvocation::CreatePullRequest {
			title, owner, repo, ..
		}) = pr
		{
			assert_eq!(title, "My Release PR");
			assert_eq!(owner, "acme");
			assert_eq!(repo, "app");
		}
	}

	#[test]
	fn cmd_release_branch_strategy_pr_failure_is_nonfatal() {
		use crate::github::client::test_support::RecordingGitHubClient;
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let client = Arc::new(RecordingGitHubClient::new().with_create_pr_failure());
		let args = ReleaseArgs::default();

		let result = cmd_release(
			dir.path(),
			&args,
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
	fn cmd_release_no_github_client_no_pr() {
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		// No github client — PR creation should be skipped entirely
		let args = ReleaseArgs::default();

		let result = cmd_release(
			dir.path(),
			&args,
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			no_github(),
		);
		assert!(
			result.is_ok(),
			"Expected Ok without github client, got: {result:?}"
		);
	}
}
