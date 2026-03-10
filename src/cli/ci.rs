//! The `ci` subcommand — auto-detects repo state and dispatches to `release` or `publish`.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Args;
use log::info;

use crate::command::CommandRunner;
use crate::git;
use crate::github::client::GitHubClient;
use crate::model::changeset::Changeset;
use crate::model::config::Config;
use crate::package_manager::filter_projects_by_name;

use super::{PublishArgs, ReleaseArgs, cmd_publish, cmd_release};

/// Arguments for the `ci` subcommand.
#[derive(Args, Default)]
pub struct CiArgs {
	/// Preview changes without modifying any files
	#[arg(long)]
	pub dry_run: bool,

	/// Only process specific packages (repeatable)
	#[arg(short = 'p', long = "package")]
	pub packages: Vec<String>,

	/// Override the release branch name (branch strategy only).
	///
	/// Passed through to `release` when pending changesets are found.
	#[arg(long)]
	pub branch: Option<String>,

	/// Skip git tag creation, tag pushing, and GitHub Releases even if enabled in config.
	///
	/// When set, the publish-state detection also falls back to "nothing to do".
	#[arg(long)]
	pub no_git: bool,
}

/// Runs the `ci` subcommand.
///
/// Auto-detects the current repository state and dispatches accordingly:
///
/// 1. Pending changesets found → run `release`
/// 2. No changesets, git enabled, and at least one expected tag is absent → run `publish`
/// 3. Otherwise → log "Nothing to do" and return success
///
/// This subcommand is always non-interactive.
///
/// # State detection
///
/// Post-release state (step 2) is detected by checking whether each package's expected git
/// tag exists. This is sufficient because publish is idempotent (ADR-004): if a package was
/// already published to a registry but the tag push failed, re-running `publish` will skip the
/// already-published registry step and only retry the tag. Checking registry state directly
/// would add network dependencies and is not necessary for correctness.
pub fn cmd_ci(
	git_workdir: &Path,
	args: &CiArgs,
	config: Config,
	runner: Arc<dyn CommandRunner>,
	github_client: Option<Arc<dyn GitHubClient>>,
) -> anyhow::Result<ExitCode> {
	// Step 1: check for pending changesets.
	let changesets = Changeset::read_all(config.git_workdir())?;
	if !changesets.is_empty() {
		info!("ci: pending changesets found, running release");
		let release_args = ReleaseArgs {
			dry_run: args.dry_run,
			packages: args.packages.clone(),
			no_git: args.no_git,
			branch: args.branch.clone(),
		};
		return cmd_release(
			git_workdir,
			&release_args,
			config,
			Arc::clone(&runner),
			github_client,
		);
	}

	// Step 2: when git is enabled and --no-git is not set, check for packages that
	// have not yet been tagged (post-release, pre-publish state).
	if config.git.enabled.unwrap_or(false) && !args.no_git {
		let projects = config.load_projects(Arc::clone(&runner))?;
		let selected = filter_projects_by_name(&projects, &args.packages)?;
		let is_multi = projects.len() > 1;

		let any_tag_missing = selected.iter().any(|project| {
			let version = project.version();
			let tag = config.git.tag_format.tag(project.name(), version, is_multi);
			// Treat git errors as "tag not found" — conservative, triggers publish.
			!git::git_tag_exists(runner.as_ref(), git_workdir, &tag).unwrap_or(false)
		});

		if any_tag_missing {
			info!("ci: no changesets but unpublished tags detected, running publish");
			let publish_args = PublishArgs {
				dry_run: args.dry_run,
				packages: args.packages.clone(),
				no_git: args.no_git,
			};
			return cmd_publish(
				git_workdir,
				&publish_args,
				config,
				Arc::clone(&runner),
				github_client,
			);
		}
	}

	info!("ci: nothing to do");
	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ci_args_default() {
		let args = CiArgs::default();
		assert!(!args.dry_run);
		assert!(args.packages.is_empty());
		assert!(args.branch.is_none());
		assert!(!args.no_git);
	}
}
