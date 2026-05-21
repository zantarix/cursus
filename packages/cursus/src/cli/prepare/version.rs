use std::collections::BTreeMap;

use semver::Version;

use crate::model::changeset::ChangeType;
use crate::package_manager::Project;

use super::PropagationMap;

/// Bumps a semver version according to the given change type.
pub(crate) fn bump_version(version: &semver::Version, change_type: ChangeType) -> semver::Version {
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
pub(crate) fn infer_change_type(old: &Version, new: &Version) -> ChangeType {
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
pub(crate) fn effective_new_version(
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
