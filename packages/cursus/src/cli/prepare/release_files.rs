use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use log::info;

use crate::model::changelog::Changelog;
use crate::model::changeset::{ChangeType, Changeset};
use crate::package_manager::{PackageManagerAdapter, Project};
use crate::utils::today_iso_date;

use super::version::bump_version;
use super::{PackageChanges, PrepareOutput, ReleaseInfo, VersionPlan};

/// Bumps package versions, writes changelogs, and collects all modified file paths.
///
/// Runs version bumping, changelog generation, dependency propagation, lock
/// file updates, and changeset consumption. Returns a [`PrepareOutput`] containing
/// release infos and the deduplicated list of all paths written.
pub(super) async fn prepare_release_files(
	adapters: &[Arc<dyn PackageManagerAdapter>],
	projects: &[Project],
	changesets: &[(crate::path::AbsolutePath, Changeset)],
	plan: VersionPlan,
	dry_run: bool,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<PrepareOutput> {
	let (release_infos, mut files) = bump_versions_and_generate_changelogs(
		&plan.aggregated,
		&plan.changes_per_package,
		projects,
		&plan.version_overrides,
		&plan.dep_entries,
		dry_run,
		fs,
	)
	.await?;
	files.extend(propagate_dependency_updates(projects, &release_infos, dry_run).await?);
	files.extend(update_lock_files(adapters).await?);
	let released: BTreeSet<String> = plan.aggregated.keys().cloned().collect();
	files.extend(consume_changesets(changesets, &released, dry_run, fs).await?);
	files.extend(plan.propagation_changeset_paths);
	files.sort();
	files.dedup();
	Ok(PrepareOutput {
		release_infos,
		modified_files: files,
	})
}

/// Bumps versions and generates changelog entries for all affected packages.
///
/// Returns a tuple of `(release_infos, modified_files)` where `release_infos` describes
/// each package prepared for release and `modified_files` is the list of paths modified.
///
/// When `version_overrides` is non-empty, packages in that map use the override
/// version instead of the standard semver bump.
pub(crate) async fn bump_versions_and_generate_changelogs(
	aggregated: &BTreeMap<String, ChangeType>,
	changes_per_package: &BTreeMap<String, PackageChanges>,
	projects: &[Project],
	version_overrides: &BTreeMap<String, semver::Version>,
	dep_entries: &BTreeMap<String, Vec<String>>,
	dry_run: bool,
	fs: &dyn crate::filesystem::Filesystem,
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
		let new_version = version_overrides
			.get(pkg_name)
			.cloned()
			.unwrap_or_else(|| bump_version(current_version, *change_type));
		modified_files.push(project.path().join("CHANGELOG.md"));
		let changes = changes_per_package
			.get(pkg_name)
			.cloned()
			.unwrap_or_default();
		let pkg_dep_entries = dep_entries.get(pkg_name).cloned().unwrap_or_default();
		let changelog = Changelog::new(
			new_version.clone(),
			today_iso_date(),
			changes,
			project.path().clone(),
		)
		.with_dependency_entries(pkg_dep_entries);
		let changelog_entry = changelog.format_sections();
		modified_files.extend(project.write_version(&new_version, dry_run).await?);
		changelog.update(dry_run, fs).await?;
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
pub(crate) async fn propagate_dependency_updates(
	projects: &[Project],
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
			let paths = project
				.update_dependency_version(dep_name, new_version, dry_run)
				.await?;
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
pub(super) async fn update_lock_files(
	adapters: &[Arc<dyn PackageManagerAdapter>],
) -> anyhow::Result<Vec<PathBuf>> {
	let mut files: Vec<PathBuf> = Vec::new();
	for adapter in adapters {
		if let Some(path) = adapter.update_lock_file().await? {
			files.push(path);
		}
	}
	Ok(files)
}

/// Consumes or dry-runs the given changeset files for the released packages.
///
/// Returns the list of changeset paths that would be (or were) modified or deleted.
pub(super) async fn consume_changesets(
	changesets: &[(crate::path::AbsolutePath, Changeset)],
	released: &BTreeSet<String>,
	dry_run: bool,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<Vec<PathBuf>> {
	let mut additional_files: Vec<PathBuf> = Vec::new();
	for (path, cs) in changesets {
		let released_pkgs: Vec<&String> = cs
			.packages
			.keys()
			.filter(|name| released.contains(*name))
			.collect();
		if !released_pkgs.is_empty() {
			additional_files.push(path.clone().into_path_buf());
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
			cs.consume(path, released, fs).await?;
		}
	}
	Ok(additional_files)
}
