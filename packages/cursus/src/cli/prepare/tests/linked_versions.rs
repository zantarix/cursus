use std::collections::BTreeMap;

use semver::Version;

use crate::cli::prepare::PackageChanges;
use crate::cli::prepare::linked_versions::*;
use crate::model::changeset::ChangeType;
use crate::package_manager::Project;

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
fn compute_group_final_version_unknown_member_is_skipped() {
	// "unknown" is not in projects — it should be silently skipped.
	let group = vec!["unknown".to_string(), "pkg-a".to_string()];
	let mut aggregated = BTreeMap::new();
	aggregated.insert("pkg-a".to_string(), ChangeType::Patch);
	let projects = vec![make_project("pkg-a", "1.0.0")];
	let result = compute_group_final_version(&group, &aggregated, &projects);
	// unknown skipped; pkg-a has Patch → bump(1.0.0, Patch) = 1.0.1
	assert_eq!(result, Some(v("1.0.1")));
}

#[test]
fn promote_package_to_final_unknown_project_is_skipped() {
	// When the named package is not in projects, nothing should be mutated.
	let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
	let mut changes: BTreeMap<String, PackageChanges> = BTreeMap::new();
	let mut overrides: BTreeMap<String, Version> = BTreeMap::new();
	promote_package_to_final(
		"nonexistent",
		&v("1.0.0"),
		&mut aggregated,
		&mut changes,
		&mut overrides,
		&[], // no projects
	);
	assert!(aggregated.is_empty(), "aggregated should be unchanged");
	assert!(overrides.is_empty(), "overrides should be unchanged");
}

#[test]
fn sync_linked_groups_after_propagation_unknown_member_is_skipped() {
	// Group contains "unknown" (not in projects) and "pkg-a".
	// Should not panic and should still sync known members.
	let group = vec!["unknown".to_string(), "pkg-a".to_string()];
	let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
	let mut changes: BTreeMap<String, PackageChanges> = BTreeMap::new();
	let mut overrides: BTreeMap<String, Version> = BTreeMap::new();
	overrides.insert("pkg-a".to_string(), v("2.0.0"));
	let projects = vec![make_project("pkg-a", "1.0.0")];

	sync_linked_groups_after_propagation(
		&mut aggregated,
		&mut changes,
		&mut overrides,
		&[group],
		&projects,
	);
	// pkg-a is at 2.0.0 (override), "unknown" skipped → max_effective = 2.0.0
	// pkg-a is already at the target → no additional override changes expected
	assert_eq!(overrides.get("pkg-a"), Some(&v("2.0.0")));
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
