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
pub(super) fn validate_scoped_prepare_linked_groups(
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
pub(super) fn compute_group_final_version(
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
pub(super) fn promote_package_to_final(
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
pub(super) fn reconcile_linked_versions(
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
pub(super) fn sync_linked_groups_after_propagation(
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

#[cfg(test)]
mod tests {
	use super::*;

	fn v(s: &str) -> Version {
		s.parse().unwrap()
	}

	fn make_project(name: &str, version: &str) -> Project {
		crate::package_manager::Project::new_test_with_version(name, v(version))
	}

	// ── validate_scoped_prepare_linked_groups ─────────────────────────────────

	#[test]
	fn validate_empty_filter_always_passes() {
		let groups = vec![vec!["pkg-a".to_string(), "pkg-b".to_string()]];
		assert!(validate_scoped_prepare_linked_groups(&[], &groups, false).is_ok());
		assert!(validate_scoped_prepare_linked_groups(&[], &groups, true).is_ok());
	}

	#[test]
	fn validate_global_with_filter_errors() {
		let result = validate_scoped_prepare_linked_groups(&["pkg-a".to_string()], &[], true);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("global linked-versions")
		);
	}

	#[test]
	fn validate_partial_overlap_errors() {
		let groups = vec![vec!["pkg-a".to_string(), "pkg-b".to_string()]];
		let filter = vec!["pkg-a".to_string()];
		let result = validate_scoped_prepare_linked_groups(&filter, &groups, false);
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(msg.contains("partially overlaps"));
		assert!(msg.contains("pkg-b"));
	}

	#[test]
	fn validate_full_group_in_scope_passes() {
		let groups = vec![vec!["pkg-a".to_string(), "pkg-b".to_string()]];
		let filter = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		assert!(validate_scoped_prepare_linked_groups(&filter, &groups, false).is_ok());
	}

	#[test]
	fn validate_group_entirely_out_of_scope_passes() {
		let groups = vec![vec!["pkg-a".to_string(), "pkg-b".to_string()]];
		let filter = vec!["standalone".to_string()];
		assert!(validate_scoped_prepare_linked_groups(&filter, &groups, false).is_ok());
	}

	// ── compute_group_final_version ───────────────────────────────────────────

	#[test]
	fn compute_group_final_version_no_changeset_returns_max_current() {
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let aggregated = BTreeMap::new(); // no changesets
		let projects = vec![
			make_project("pkg-a", "1.5.0"),
			make_project("pkg-b", "1.2.0"),
		];
		let result = compute_group_final_version(&group, &aggregated, &projects);
		// No changeset → final version = max current = 1.5.0
		assert_eq!(result, Some(v("1.5.0")));
	}

	#[test]
	fn compute_group_final_version_applies_highest_change_type_to_max_current() {
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated = BTreeMap::new();
		aggregated.insert("pkg-b".to_string(), ChangeType::Minor); // only B has changeset
		let projects = vec![
			make_project("pkg-a", "2.3.4"), // higher current, no changeset
			make_project("pkg-b", "1.2.3"), // lower current, has changeset
		];
		let result = compute_group_final_version(&group, &aggregated, &projects);
		// max_current = 2.3.4; highest_ct = Minor → bump(2.3.4, Minor) = 2.4.0
		assert_eq!(result, Some(v("2.4.0")));
	}

	#[test]
	fn compute_group_final_version_major_wins_over_minor() {
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Major);
		aggregated.insert("pkg-b".to_string(), ChangeType::Minor);
		let projects = vec![
			make_project("pkg-a", "1.0.0"),
			make_project("pkg-b", "1.0.0"),
		];
		let result = compute_group_final_version(&group, &aggregated, &projects);
		// highest_ct = Major → bump(1.0.0, Major) = 2.0.0
		assert_eq!(result, Some(v("2.0.0")));
	}

	#[test]
	fn compute_group_final_version_returns_none_for_empty_group_in_projects() {
		let group = vec!["nonexistent".to_string()];
		let aggregated = BTreeMap::new();
		let projects: Vec<Project> = vec![];
		let result = compute_group_final_version(&group, &aggregated, &projects);
		// No project found → max_current is None → returns None
		assert_eq!(result, None);
	}

	// ── reconcile_linked_versions ─────────────────────────────────────────────

	#[test]
	fn reconcile_promotes_no_changeset_package_to_final() {
		// A@2.3.4 (no cs) + B@1.2.3 (patch) → both should end at 2.3.5
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
		aggregated.insert("pkg-b".to_string(), ChangeType::Patch);
		let mut changes_per_package: BTreeMap<String, PackageChanges> = BTreeMap::new();
		changes_per_package
			.entry("pkg-b".to_string())
			.or_default()
			.push((ChangeType::Patch, Some("a fix".to_string()), None));
		let projects = vec![
			make_project("pkg-a", "2.3.4"),
			make_project("pkg-b", "1.2.3"),
		];
		let overrides = reconcile_linked_versions(
			&mut aggregated,
			&mut changes_per_package,
			&[group],
			&projects,
		);
		// pkg-a has no changeset, is below 2.3.5 → gets a sync override
		assert_eq!(overrides.get("pkg-a"), Some(&v("2.3.5")));
		// pkg-b has a changeset; natural bump(1.2.3, Patch)=1.2.4 ≠ 2.3.5 → override
		assert_eq!(overrides.get("pkg-b"), Some(&v("2.3.5")));
		// pkg-a should now have a sync changelog entry
		let a_changes = changes_per_package.get("pkg-a").unwrap();
		assert!(
			a_changes
				.iter()
				.any(|(_, msg, _)| msg.as_deref().is_some_and(|m| m.contains("version sync")))
		);
	}

	#[test]
	fn reconcile_no_override_when_natural_bump_matches_final() {
		// A@1.0.0 (patch) + B@1.0.0 (no cs): final = 1.0.1; A natural bump = 1.0.1 → no override for A
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Patch);
		let mut changes_per_package: BTreeMap<String, PackageChanges> = BTreeMap::new();
		changes_per_package
			.entry("pkg-a".to_string())
			.or_default()
			.push((ChangeType::Patch, Some("a fix".to_string()), None));
		let projects = vec![
			make_project("pkg-a", "1.0.0"),
			make_project("pkg-b", "1.0.0"),
		];
		let overrides = reconcile_linked_versions(
			&mut aggregated,
			&mut changes_per_package,
			&[group],
			&projects,
		);
		// A natural bump = 1.0.1 == final → no override for A
		assert!(
			!overrides.contains_key("pkg-a"),
			"pkg-a should not be overridden"
		);
		// B has no changeset, current 1.0.0 < 1.0.1 → promoted
		assert_eq!(overrides.get("pkg-b"), Some(&v("1.0.1")));
	}

	#[test]
	fn reconcile_skips_packages_already_at_final_version() {
		// Both packages at the same version, only A has a changeset
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Patch);
		let mut changes_per_package: BTreeMap<String, PackageChanges> = BTreeMap::new();
		changes_per_package
			.entry("pkg-a".to_string())
			.or_default()
			.push((ChangeType::Patch, Some("a fix".to_string()), None));
		let projects = vec![
			make_project("pkg-a", "1.0.1"),
			make_project("pkg-b", "1.0.1"),
		];
		// final = bump(1.0.1, Patch) = 1.0.2; B is at 1.0.1 < 1.0.2 → promoted
		let overrides = reconcile_linked_versions(
			&mut aggregated,
			&mut changes_per_package,
			&[group],
			&projects,
		);
		assert_eq!(overrides.get("pkg-b"), Some(&v("1.0.2")));
	}
}
