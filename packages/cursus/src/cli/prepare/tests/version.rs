use std::collections::{BTreeMap, BTreeSet};

use crate::cli::prepare::version::*;
use crate::model::changeset::ChangeType;

fn v(s: &str) -> semver::Version {
	s.parse().unwrap()
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
fn infer_change_type_major_when_major_differs() {
	let old = "1.2.3".parse().unwrap();
	let new = "2.0.0".parse().unwrap();
	assert_eq!(infer_change_type(&old, &new), ChangeType::Major);
}

#[test]
fn infer_change_type_minor_when_only_minor_differs() {
	let old = "1.2.3".parse().unwrap();
	let new = "1.3.0".parse().unwrap();
	assert_eq!(infer_change_type(&old, &new), ChangeType::Minor);
}

#[test]
fn infer_change_type_patch_when_only_patch_differs() {
	let old = "1.2.3".parse().unwrap();
	let new = "1.2.4".parse().unwrap();
	assert_eq!(infer_change_type(&old, &new), ChangeType::Patch);
}

#[test]
fn infer_change_type_patch_when_equal() {
	let v_ver: semver::Version = "1.2.3".parse().unwrap();
	assert_eq!(infer_change_type(&v_ver, &v_ver), ChangeType::Patch);
}

fn make_project(name: &str, version: &str) -> crate::package_manager::Project {
	crate::package_manager::Project::new_test_with_version(name, v(version))
}

#[test]
fn effective_new_version_returns_none_for_unknown_package() {
	let projects = vec![make_project("pkg-a", "1.0.0")];
	let aggregated = BTreeMap::new();
	let version_overrides = BTreeMap::new();
	let propagation_map = BTreeMap::new();
	let result = effective_new_version(
		"unknown",
		&projects,
		&aggregated,
		&version_overrides,
		&propagation_map,
	);
	assert!(result.is_none());
}

#[test]
fn effective_new_version_prefers_version_override() {
	let projects = vec![make_project("pkg-a", "1.0.0")];
	let mut aggregated = BTreeMap::new();
	aggregated.insert("pkg-a".to_string(), ChangeType::Major);
	let mut version_overrides = BTreeMap::new();
	version_overrides.insert("pkg-a".to_string(), "9.9.9".parse().unwrap());
	let propagation_map = BTreeMap::new();
	let result = effective_new_version(
		"pkg-a",
		&projects,
		&aggregated,
		&version_overrides,
		&propagation_map,
	);
	assert_eq!(result, Some("9.9.9".parse().unwrap()));
}

#[test]
fn effective_new_version_uses_aggregated_changeset() {
	let projects = vec![make_project("pkg-a", "1.2.0")];
	let mut aggregated = BTreeMap::new();
	aggregated.insert("pkg-a".to_string(), ChangeType::Minor);
	let version_overrides = BTreeMap::new();
	let propagation_map = BTreeMap::new();
	let result = effective_new_version(
		"pkg-a",
		&projects,
		&aggregated,
		&version_overrides,
		&propagation_map,
	);
	assert_eq!(result, Some("1.3.0".parse().unwrap()));
}

#[test]
fn effective_new_version_falls_back_to_propagation_map() {
	let projects = vec![make_project("pkg-a", "1.0.0")];
	let aggregated = BTreeMap::new();
	let version_overrides = BTreeMap::new();
	let mut propagation_map = BTreeMap::new();
	propagation_map.insert("pkg-a".to_string(), (ChangeType::Patch, BTreeSet::new()));
	let result = effective_new_version(
		"pkg-a",
		&projects,
		&aggregated,
		&version_overrides,
		&propagation_map,
	);
	assert_eq!(result, Some("1.0.1".parse().unwrap()));
}
