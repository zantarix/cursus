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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn dependency_bump_default_is_auto() {
		assert_eq!(DependencyBump::default(), DependencyBump::Auto);
	}

	#[test]
	fn prepare_config_default_has_auto_dependency_bump() {
		let config = PrepareConfig::default();
		assert_eq!(config.dependency_bump, DependencyBump::Auto);
	}

	#[test]
	fn dependency_bump_serializes_as_lowercase() {
		// TOML requires a root table, so wrap in PrepareConfig for serialization.
		let cases = [
			(DependencyBump::Auto, "auto"),
			(DependencyBump::Match, "match"),
			(DependencyBump::Patch, "patch"),
			(DependencyBump::Minor, "minor"),
			(DependencyBump::Major, "major"),
		];
		for (variant, expected) in cases {
			let config = PrepareConfig {
				dependency_bump: variant,
			};
			let serialized = toml::to_string(&config.data).unwrap();
			assert!(
				serialized.contains(&format!("dependency_bump = \"{expected}\"")),
				"Expected 'dependency_bump = \"{expected}\"' in:\n{serialized}"
			);
		}
	}

	#[test]
	fn prepare_config_deserializes_dependency_bump_values() {
		let config: PrepareConfig = toml::from_str("dependency_bump = \"auto\"").unwrap();
		assert_eq!(config.dependency_bump, DependencyBump::Auto);

		let config: PrepareConfig = toml::from_str("dependency_bump = \"match\"").unwrap();
		assert_eq!(config.dependency_bump, DependencyBump::Match);

		let config: PrepareConfig = toml::from_str("dependency_bump = \"patch\"").unwrap();
		assert_eq!(config.dependency_bump, DependencyBump::Patch);

		let config: PrepareConfig = toml::from_str("dependency_bump = \"minor\"").unwrap();
		assert_eq!(config.dependency_bump, DependencyBump::Minor);

		let config: PrepareConfig = toml::from_str("dependency_bump = \"major\"").unwrap();
		assert_eq!(config.dependency_bump, DependencyBump::Major);
	}

	#[test]
	fn prepare_config_rejects_unknown_fields() {
		let result = toml::from_str::<PrepareConfig>("unknown_field = true");
		assert!(result.is_err(), "Unknown fields should be rejected");
	}

	#[test]
	fn prepare_config_round_trips_through_toml() {
		let config = PrepareConfig {
			dependency_bump: DependencyBump::Match,
		};
		let serialized = toml::to_string(&config.data).unwrap();
		let deserialized: PrepareConfig = toml::from_str(&serialized).unwrap();
		assert_eq!(config, deserialized);
	}
}
