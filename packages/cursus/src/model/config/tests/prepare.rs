use crate::model::config::prepare::*;

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
		let serialized = toml::to_string(&config).unwrap();
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
	let serialized = toml::to_string(&config).unwrap();
	let deserialized: PrepareConfig = toml::from_str(&serialized).unwrap();
	assert_eq!(config, deserialized);
}
