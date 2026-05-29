//! The `prepare` subcommand.

pub(super) mod changeset;
pub(super) mod git_lifecycle;
pub(super) mod github;
pub(super) mod linked_versions;
pub(super) mod propagation;
pub(super) mod release_files;
pub(super) mod version;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use log::info;

use semver::Version;

use crate::model::changeset::{ChangeType, Changeset};
use crate::model::config::Config;
use crate::package_manager::Project;

use changeset::{aggregate_changesets, resolve_commit_references};
use git_lifecycle::{GitContext, finalize_git_lifecycle, preflight_checks, setup_git_context};
use linked_versions::{
	reconcile_linked_versions, resolve_linked_groups, sync_linked_groups_after_propagation,
};
use propagation::apply_dependency_propagation;
use release_files::prepare_release_files;

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

/// Information about a single package prepared for release.
#[derive(Debug)]
pub(crate) struct ReleaseInfo {
	pub(crate) package_name: String,
	pub(crate) new_version: Version,
	/// Formatted changelog sections (### headings + bullets) without the version heading.
	pub(crate) changelog_entry: String,
}

/// Per-package changelog entries: `(ChangeType, Option<message>, Option<CommitReference>)` tuples.
pub(crate) type PackageChanges = Vec<(
	ChangeType,
	Option<String>,
	Option<crate::model::changelog::CommitReference>,
)>;

/// `(dep_entries_per_package, new_changeset_paths)` returned by [`propagation::apply_dependency_propagation`].
pub(super) type PropagationResult = (BTreeMap<String, Vec<String>>, Vec<PathBuf>);

/// Map of `pkg_name → (effective_ct, [upstream_names])` from dependency propagation phase 1.
pub(crate) type PropagationMap = BTreeMap<String, (ChangeType, BTreeSet<String>)>;

/// Output of the release file preparation phase.
#[derive(Debug)]
pub(crate) struct PrepareOutput {
	pub(crate) release_infos: Vec<ReleaseInfo>,
	pub(crate) modified_files: Vec<PathBuf>,
}

/// Result of computing the version plan for a prepare run.
pub(super) struct VersionPlan {
	pub(super) aggregated: BTreeMap<String, ChangeType>,
	pub(super) changes_per_package: BTreeMap<String, PackageChanges>,
	pub(super) version_overrides: BTreeMap<String, Version>,
	pub(super) dep_entries: BTreeMap<String, Vec<String>>,
	pub(super) propagation_changeset_paths: Vec<PathBuf>,
}

/// Aggregates changesets, applies linked versions, and runs dependency propagation.
///
/// Returns the full version plan for the prepare run.
async fn compute_version_plan(
	changesets: &[(crate::path::AbsolutePath, Changeset)],
	args: &PrepareArgs,
	env: &crate::Env,
	config: &Config,
	projects: &[Project],
	git_ctx: &GitContext,
	dry_run: bool,
) -> anyhow::Result<VersionPlan> {
	let git = env.git();
	let commit_refs = resolve_commit_references(changesets, git, git_ctx.enabled).await;
	let (mut aggregated, mut changes_per_package) =
		aggregate_changesets(changesets, &args.packages, projects, &commit_refs)?;
	let linked_groups = resolve_linked_groups(config, args, projects)?;
	// First pass: sync linked groups from explicit changesets.
	let mut version_overrides = reconcile_linked_versions(
		&mut aggregated,
		&mut changes_per_package,
		&linked_groups,
		projects,
	);
	let (dep_entries, propagation_changeset_paths) = apply_dependency_propagation(
		projects,
		&mut aggregated,
		&version_overrides,
		&args.packages,
		config.prepare.dependency_bump,
		env,
		dry_run,
	)
	.await?;
	// Second pass: propagated bumps may have raised a linked member's version, so
	// re-sync to bring the rest of each group up to the new target.
	sync_linked_groups_after_propagation(
		&mut aggregated,
		&mut changes_per_package,
		&mut version_overrides,
		&linked_groups,
		projects,
	);
	Ok(VersionPlan {
		aggregated,
		changes_per_package,
		version_overrides,
		dep_entries,
		propagation_changeset_paths,
	})
}

/// Runs the `prepare` subcommand.
pub(crate) async fn cmd_prepare(
	args: &PrepareArgs,
	dry_run: bool,
	env: &crate::Env,
	config: Config,
) -> anyhow::Result<ExitCode> {
	let git = env.git();
	let adapters = config.create_adapters(env)?;
	let projects = config.load_projects_for_adapters(&adapters).await?;
	let changesets = Changeset::read_all(env).await?;

	if changesets.is_empty() {
		info!("No pending changesets found. Nothing to prepare.");
		return Ok(ExitCode::SUCCESS);
	}

	let git_ctx = setup_git_context(&config, args);
	let plan = compute_version_plan(
		&changesets,
		args,
		env,
		&config,
		&projects,
		&git_ctx,
		dry_run,
	)
	.await?;
	let branches = preflight_checks(git, &config, env, args, &git_ctx, dry_run).await?;
	let output =
		prepare_release_files(&adapters, &projects, &changesets, plan, dry_run, env.fs()).await?;

	finalize_git_lifecycle(git, &config, env, &output, &branches, &git_ctx, dry_run).await?;

	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests;
