//! The `prepare` subcommand.

pub mod changeset;
pub mod git_lifecycle;
pub mod github;
pub mod linked_versions;
pub mod propagation;
pub mod release_files;
pub mod version;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use clap::Args;
use log::info;

use semver::Version;

use crate::git;
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
pub(super) struct ReleaseInfo {
	pub(super) package_name: String,
	pub(super) new_version: Version,
	/// Formatted changelog sections (### headings + bullets) without the version heading.
	pub(super) changelog_entry: String,
}

/// Per-package changelog entries: `(ChangeType, Option<message>, Option<CommitReference>)` tuples.
pub(super) type PackageChanges = Vec<(
	ChangeType,
	Option<String>,
	Option<crate::model::changelog::CommitReference>,
)>;

/// `(dep_entries_per_package, new_changeset_paths)` returned by [`propagation::apply_dependency_propagation`].
pub(super) type PropagationResult = (BTreeMap<String, Vec<String>>, Vec<PathBuf>);

/// Map of `pkg_name → (effective_ct, [upstream_names])` from dependency propagation phase 1.
pub(super) type PropagationMap = BTreeMap<String, (ChangeType, BTreeSet<String>)>;

/// Output of the release file preparation phase.
#[derive(Debug)]
pub(super) struct PrepareOutput {
	pub(super) release_infos: Vec<ReleaseInfo>,
	pub(super) modified_files: Vec<PathBuf>,
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
fn compute_version_plan(
	git: &dyn git::Git,
	changesets: &[(PathBuf, Changeset)],
	args: &PrepareArgs,
	config: &Config,
	projects: &[Project],
	git_ctx: &GitContext,
	dry_run: bool,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<VersionPlan> {
	let commit_refs = resolve_commit_references(changesets, git, git_ctx.enabled);
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
		git,
		dry_run,
		fs,
	)?;
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
pub(crate) fn cmd_prepare(
	git: &dyn git::Git,
	args: &PrepareArgs,
	dry_run: bool,
	config: Config,
) -> anyhow::Result<ExitCode> {
	let env = config.env().context("env not set")?;
	let adapters = config.create_adapters()?;
	let projects = config.load_projects_for_adapters(&adapters)?;
	let changesets = Changeset::read_all(git, env.fs())?;

	if changesets.is_empty() {
		info!("No pending changesets found. Nothing to prepare.");
		return Ok(ExitCode::SUCCESS);
	}

	let git_ctx = setup_git_context(&config, args);
	let plan = compute_version_plan(
		git,
		&changesets,
		args,
		&config,
		&projects,
		&git_ctx,
		dry_run,
		env.fs(),
	)?;
	let branches = preflight_checks(git, &config, env, args, &git_ctx, dry_run)?;
	let output = prepare_release_files(&adapters, &projects, &changesets, plan, dry_run, env.fs())?;

	finalize_git_lifecycle(git, &config, env, &output, &branches, &git_ctx, dry_run)?;

	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::filesystem::LocalFilesystem;
	use crate::model::config;

	use super::*;

	fn make_runner() -> Arc<dyn CommandRunner> {
		Arc::new(RecordingCommandRunner::new(0))
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
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
		);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
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
		let cursus_dir = dir.path().join(".cursus");
		std::fs::write(
			cursus_dir.join("test.md"),
			"+++\nnonexistent-package = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let args = PrepareArgs::default();
		let runner = make_runner();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
		);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
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

		let cursus_dir = dir.path().join(".cursus");
		std::fs::write(
			cursus_dir.join("test.md"),
			"+++\nreal-project = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let runner = make_runner();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
		);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs {
			packages: vec!["nonexistent".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
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
}
