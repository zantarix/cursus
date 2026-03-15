use std::collections::BTreeMap;

use semver::Version;

use crate::model::changeset::ChangeType;
use crate::package_manager::Project;

use super::PropagationMap;

/// Bumps a semver version according to the given change type.
pub(super) fn bump_version(version: &semver::Version, change_type: ChangeType) -> semver::Version {
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

/// Infers the [`ChangeType`] needed to move from `old` to `new`.
///
/// Returns the highest change type implied by the version difference:
/// - Major if the major component changed
/// - Minor if only the minor component changed
/// - Patch otherwise
pub(super) fn infer_change_type(old: &Version, new: &Version) -> ChangeType {
	if new.major != old.major {
		ChangeType::Major
	} else if new.minor != old.minor {
		ChangeType::Minor
	} else {
		ChangeType::Patch
	}
}

/// Computes the effective new version for a package given its current state.
///
/// Checks, in priority order:
/// 1. `version_overrides` (linked-version reconciliation)
/// 2. `aggregated` (changeset-driven bump)
/// 3. `propagation_map` (propagation-driven bump, complete after phase 1)
///
/// When called during the sweep phase, `aggregated` is being mutated. This
/// function is correct because both the `aggregated` fallback (catches packages
/// already updated earlier in the loop, ordered by BTreeMap iteration) and the
/// `propagation_map` fallback (contains the complete phase-1 result) cover all
/// upstream packages.
pub(super) fn effective_new_version(
	pkg_name: &str,
	projects: &[Project],
	aggregated: &BTreeMap<String, ChangeType>,
	version_overrides: &BTreeMap<String, Version>,
	propagation_map: &PropagationMap,
) -> Option<Version> {
	if let Some(v) = version_overrides.get(pkg_name) {
		return Some(v.clone());
	}
	let ct = aggregated
		.get(pkg_name)
		.copied()
		.or_else(|| propagation_map.get(pkg_name).map(|(ct, _)| *ct))?;
	let project = projects.iter().find(|p| p.name() == pkg_name)?;
	Some(bump_version(project.version(), ct))
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeSet;

	use super::*;

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
}
