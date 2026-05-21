//! `[prepare]` configuration section.

use serde::{Deserialize, Serialize};

use crate::model::changeset::ChangeType;

/// Determines how a dependency bump level maps to a propagated bump for dependents.
///
/// Configures `.cursus/config.toml` under `[prepare].dependency_bump`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DependencyBump {
	/// Propagate `major` dependency bumps as `major`; all others as `patch` (default).
	#[default]
	Auto,
	/// Propagated bump level matches the upstream dependency's bump level.
	Match,
	/// Always bump dependents by `patch`.
	Patch,
	/// Always bump dependents by `minor`.
	Minor,
	/// Always bump dependents by `major`.
	Major,
}

impl DependencyBump {
	/// Returns the bump level to apply to a dependent package given the upstream bump level.
	pub fn to_change_type(self, upstream: ChangeType) -> ChangeType {
		match self {
			Self::Patch => ChangeType::Patch,
			Self::Minor => ChangeType::Minor,
			Self::Major => ChangeType::Major,
			Self::Match => upstream,
			Self::Auto => match upstream {
				ChangeType::Major => ChangeType::Major,
				ChangeType::Minor | ChangeType::Patch => ChangeType::Patch,
			},
		}
	}
}

/// Configuration for the `prepare` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct PrepareConfig {
	/// How to compute the semver bump level for packages that receive a propagated version
	/// bump from a bumped dependency.
	///
	/// - `"auto"` (default): `patch` for minor/patch upstream bumps, `major` for major.
	/// - `"match"`: mirrors the upstream bump level exactly.
	/// - `"patch"`, `"minor"`, `"major"`: always use the specified level.
	pub dependency_bump: DependencyBump,
}
