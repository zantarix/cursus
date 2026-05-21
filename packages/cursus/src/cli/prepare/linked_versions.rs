use std::collections::BTreeMap;

use semver::Version;

use crate::model::changeset::ChangeType;
use crate::model::config::Config;
use crate::package_manager::Project;

use super::version::{bump_version, infer_change_type};
use super::{PackageChanges, PrepareArgs};

/// Validates that a scoped prepare does not partially overlap any linked group.
///
/// Returns an error if `--package` includes some but not all packages from a
/// linked group, which would break the version-sync invariant. Full group
/// inclusion or full group exclusion are both fine.
///
/// For global linking (`package_filter` non-empty with no groups, i.e., all
/// packages linked), any `--package` filter is rejected outright because there
/// is no valid subset.
///
/// # Errors
///
/// Returns an error when partial overlap is detected or global linking is
/// active with a package filter.
pub(crate) fn validate_scoped_prepare_linked_groups(
	package_filter: &[String],
	linked_groups: &[Vec<String>],
	is_global: bool,
) -> anyhow::Result<()> {
	if package_filter.is_empty() {
		return Ok(());
	}

	if is_global {
		anyhow::bail!(
			"Cannot use --package with global linked-versions (enabled = true with no groups). \
			 All packages must be prepared together when globally linked."
		);
	}

	for group in linked_groups {
		let in_scope: Vec<&String> = group
			.iter()
			.filter(|p| package_filter.contains(p))
			.collect();
		let out_of_scope: Vec<&String> = group
			.iter()
			.filter(|p| !package_filter.contains(p))
			.collect();

		if !in_scope.is_empty() && !out_of_scope.is_empty() {
			let group_list = group.join(", ");
			let missing_list: Vec<&str> = out_of_scope.iter().map(|s| s.as_str()).collect();
			anyhow::bail!(
				"--package scope partially overlaps a linked-versions group [{group_list}]. \
				 Missing packages: {}. \
				 Include all packages from the group or exclude all of them.",
				missing_list.join(", ")
			);
		}
	}

	Ok(())
}

/// Computes the final target version for all packages in a linked group.
///
/// The algorithm:
/// 1. Find the maximum **current** version across all packages in the group.
/// 2. Find the highest `ChangeType` from any pending changeset in the group.
/// 3. If any changeset exists, apply it to the group max: `bump_version(max_current, highest_ct)`.
///    Otherwise the group max itself is the target (pure sync, no increment).
///
/// This ensures that a changeset on any package always advances the group,
/// even when another package already holds a higher current version.
pub(crate) fn compute_group_final_version(
	group: &[String],
	aggregated: &BTreeMap<String, ChangeType>,
	projects: &[Project],
) -> Option<Version> {
	let mut max_current: Option<Version> = None;
	let mut highest_ct: Option<ChangeType> = None;
	for pkg_name in group {
		let Some(project) = projects.iter().find(|p| p.name() == pkg_name) else {
			continue;
		};
		let current = project.version().clone();
		max_current = Some(match max_current {
			Some(c) => c.max(current),
			None => current,
		});
		if let Some(&ct) = aggregated.get(pkg_name) {
			highest_ct = Some(match highest_ct {
				Some(h) => h.max(ct),
				None => ct,
			});
		}
	}
	let max_current = max_current?;
	Some(match highest_ct {
		Some(ct) => bump_version(&max_current, ct),
		None => max_current,
	})
}

/// Promotes a no-changeset package to `final_version`, inserting a sync changelog entry.
pub(crate) fn promote_package_to_final(
	pkg_name: &str,
	final_version: &Version,
	aggregated: &mut BTreeMap<String, ChangeType>,
	changes_per_package: &mut BTreeMap<String, PackageChanges>,
	version_overrides: &mut BTreeMap<String, Version>,
	projects: &[Project],
) {
	let Some(project) = projects.iter().find(|p| p.name() == pkg_name) else {
		log::warn!(
			"Package '{pkg_name}' in linked group not found in projects; skipping version sync"
		);
		return;
	};
	let sync_ct = infer_change_type(project.version(), final_version);
	aggregated.insert(pkg_name.to_string(), sync_ct);
	changes_per_package
		.entry(pkg_name.to_string())
		.or_default()
		.push((
			sync_ct,
			Some(format!("version sync to {final_version} (linked versions)")),
			None,
		));
	version_overrides.insert(pkg_name.to_string(), final_version.clone());
}

/// Applies the group `final_version` to every package in the group.
///
/// - Packages **without** a changeset that are below `final_version` get a
///   synthetic sync changelog entry and a version override.
/// - Packages **with** a changeset whose natural bump differs from `final_version`
///   get a version override only (their own changeset entry documents the change).
pub(super) fn apply_group_final_version(
	group: &[String],
	final_version: &Version,
	aggregated: &mut BTreeMap<String, ChangeType>,
	changes_per_package: &mut BTreeMap<String, PackageChanges>,
	version_overrides: &mut BTreeMap<String, Version>,
	projects: &[Project],
) {
	for pkg_name in group {
		let Some(project) = projects.iter().find(|p| p.name() == pkg_name) else {
			continue;
		};
		if let Some(&ct) = aggregated.get(pkg_name) {
			if bump_version(project.version(), ct) != *final_version {
				version_overrides.insert(pkg_name.clone(), final_version.clone());
			}
		} else if project.version() < final_version {
			promote_package_to_final(
				pkg_name,
				final_version,
				aggregated,
				changes_per_package,
				version_overrides,
				projects,
			);
		}
	}
}

/// Runs the linked-version reconciliation step.
///
/// For each linked group, computes the target version (max current version bumped
/// by the highest change type from any group changeset) and promotes all packages
/// to that target. Returns a map of `package_name → override_version` for packages
/// that need a non-default version.
pub(crate) fn reconcile_linked_versions(
	aggregated: &mut BTreeMap<String, ChangeType>,
	changes_per_package: &mut BTreeMap<String, PackageChanges>,
	linked_groups: &[Vec<String>],
	projects: &[Project],
) -> BTreeMap<String, Version> {
	let mut version_overrides: BTreeMap<String, Version> = BTreeMap::new();
	for group in linked_groups {
		let Some(final_version) = compute_group_final_version(group, aggregated, projects) else {
			continue;
		};
		apply_group_final_version(
			group,
			&final_version,
			aggregated,
			changes_per_package,
			&mut version_overrides,
			projects,
		);
	}
	version_overrides
}

/// After dependency propagation, syncs linked groups so that any member raised by
/// propagation pulls the rest of the group with it.
///
/// Unlike `reconcile_linked_versions`, this pass does not re-derive a bump level
/// from `aggregated` data (which would misread the synthetic sync entries the first
/// pass inserted). Instead it promotes all group members to the max *effective new
/// version* already determined for any member.
pub(crate) fn sync_linked_groups_after_propagation(
	aggregated: &mut BTreeMap<String, ChangeType>,
	changes_per_package: &mut BTreeMap<String, PackageChanges>,
	version_overrides: &mut BTreeMap<String, Version>,
	linked_groups: &[Vec<String>],
	projects: &[Project],
) {
	for group in linked_groups {
		let mut max_effective: Option<Version> = None;
		for pkg_name in group {
			let Some(project) = projects.iter().find(|p| p.name() == pkg_name) else {
				continue;
			};
			let effective = if let Some(v) = version_overrides.get(pkg_name) {
				v.clone()
			} else if let Some(&ct) = aggregated.get(pkg_name) {
				bump_version(project.version(), ct)
			} else {
				project.version().clone()
			};
			max_effective = Some(match max_effective {
				Some(m) => m.max(effective),
				None => effective,
			});
		}
		let Some(target) = max_effective else {
			continue;
		};
		apply_group_final_version(
			group,
			&target,
			aggregated,
			changes_per_package,
			version_overrides,
			projects,
		);
	}
}

/// Resolves and applies linked-version constraints to the aggregated changeset data.
///
/// Validates scoped-prepare rules, then runs reconciliation and returns the
/// resulting version overrides.
pub(super) fn resolve_linked_groups(
	config: &Config,
	args: &PrepareArgs,
	projects: &[Project],
) -> anyhow::Result<Vec<Vec<String>>> {
	let project_names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let linked_groups = config.linked_versions.resolve_groups(&project_names)?;
	validate_scoped_prepare_linked_groups(
		&args.packages,
		&linked_groups,
		config.linked_versions.is_global(),
	)?;
	Ok(linked_groups)
}
