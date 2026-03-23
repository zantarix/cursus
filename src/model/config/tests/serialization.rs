use super::*;

#[test]
fn config_serializes_with_sections() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_npm(NpmConfig::enabled());
	let toml_str = toml::to_string(&config).unwrap();
	assert!(toml_str.contains("[npm]"));
	assert!(toml_str.contains("enabled = true"));
}

#[test]
fn config_deserializes_with_sections() {
	let config: ConfigData = toml::from_str("[npm]\nenabled = true").unwrap();
	assert!(config.npm.enabled);
	assert!(!config.cargo.enabled);

	let config: ConfigData = toml::from_str("[cargo]\nenabled = true").unwrap();
	assert!(!config.npm.enabled);
	assert!(config.cargo.enabled);
}

#[test]
fn load_fails_on_unknown_top_level_field() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), "[rust]\nenabled = true").unwrap();

	let env = make_env_with_git(dir.path());
	let err = load(&env).unwrap_err();
	let chain = format!("{err:#}");
	assert!(
		chain.contains("unknown field"),
		"Expected 'unknown field' error, got: {chain}"
	);
}

#[test]
fn load_fails_on_unknown_package_manager_field() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[npm]\nenabled = true\nversion = \"1.0\"",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let err = load(&env).unwrap_err();
	let chain = format!("{err:#}");
	assert!(
		chain.contains("unknown field"),
		"Expected 'unknown field' error, got: {chain}"
	);
}

#[test]
fn deserialize_config_with_path() {
	let config: ConfigData = toml::from_str("[npm]\nenabled = true\npath = \"frontend\"").unwrap();
	assert!(config.npm.enabled);
	assert_eq!(config.npm.path, Some("frontend".to_string()));
}

#[test]
fn deserialize_config_without_path() {
	let config: ConfigData = toml::from_str("[npm]\nenabled = true").unwrap();
	assert!(config.npm.enabled);
	assert_eq!(config.npm.path, None);
}

#[test]
fn serialize_config_omits_none_path() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_npm(NpmConfig::enabled());
	let toml_str = toml::to_string(&config).unwrap();
	assert!(!toml_str.contains("path"), "None path should be omitted");
}

#[test]
fn serialize_config_includes_some_path() {
	let dir = temp_dir();
	let mut config = Config::new(&make_env_with_git(dir.path())).with_npm(NpmConfig::enabled());
	config.npm.path = Some("frontend".to_string());
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		toml_str.contains("path = \"frontend\""),
		"Some path should be serialized, got: {toml_str}"
	);
}

#[test]
fn config_roundtrip_with_path() {
	let dir = temp_dir();
	let mut config = Config::new(&make_env_with_git(dir.path())).with_npm(NpmConfig::enabled());
	config.npm.path = Some("frontend".to_string());
	config.save().unwrap();
	let env = make_env_with_git(dir.path());
	let loaded = load(&env).unwrap();
	assert_eq!(loaded.npm.path, Some("frontend".to_string()));
}

#[test]
fn config_roundtrip() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());

	for pm in [PackageManager::Npm, PackageManager::Cargo] {
		let config = match pm {
			PackageManager::Npm => Config::new(&env).with_npm(NpmConfig::enabled()),
			PackageManager::Cargo => Config::new(&env).with_cargo(CargoConfig::enabled()),
		};
		config.save().unwrap();
		let loaded = load(&env).unwrap();
		let enabled: Vec<_> = loaded.enabled_package_managers().collect();
		assert_eq!(enabled, vec![pm]);
	}
}

#[test]
fn global_config_defaults_to_warnings_enabled() {
	let global = GlobalConfig::default();
	assert!(!global.disable_dependency_cycle_warnings);
}

#[test]
fn config_deserializes_without_global_section() {
	let config: ConfigData = toml::from_str("[npm]\nenabled = true").unwrap();
	assert!(config.npm.enabled);
	assert!(!config.global.disable_dependency_cycle_warnings);
}

#[test]
fn config_deserializes_with_global_section() {
	let toml_str = r#"
[global]
disable_dependency_cycle_warnings = true

[npm]
enabled = true
"#;
	let config: ConfigData = toml::from_str(toml_str).unwrap();
	assert!(config.npm.enabled);
	assert!(config.global.disable_dependency_cycle_warnings);
}

#[test]
fn config_roundtrip_with_global() {
	let dir = temp_dir();
	let mut global = GlobalConfig::default();
	global.disable_dependency_cycle_warnings = true;
	let config = Config::new(&make_env_with_git(dir.path()))
		.with_global(global)
		.with_npm(NpmConfig::enabled());
	config.save().unwrap();
	let env = make_env_with_git(dir.path());
	let loaded = load(&env).unwrap();
	assert!(loaded.global.disable_dependency_cycle_warnings);
}

#[test]
fn global_config_unknown_field_fails() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[global]\nunknown_field = true\n[npm]\nenabled = true",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let err = load(&env).unwrap_err();
	let chain = format!("{err:#}");
	assert!(
		chain.contains("unknown field"),
		"Expected 'unknown field' error, got: {chain}"
	);
}

#[test]
fn config_deserializes_github_section() {
	let config: ConfigData =
		toml::from_str("[npm]\nenabled = true\n[github]\nenabled = true").unwrap();
	assert!(config.github.enabled);
}

#[test]
fn config_github_unknown_field_fails() {
	let result: Result<ConfigData, _> =
		toml::from_str("[npm]\nenabled = true\n[github]\nunknown = true");
	assert!(
		result.is_err(),
		"Expected error for unknown field in [github]"
	);
}

#[test]
fn config_without_github_section_defaults_disabled() {
	let config: ConfigData = toml::from_str("[npm]\nenabled = true").unwrap();
	assert!(!config.github.enabled);
}

#[test]
fn load_github_enabled_derives_git_enabled() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[github]\nenabled = true\n",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(&env).unwrap();
	assert!(loaded.github.enabled);
	assert!(
		loaded.git.enabled(),
		"git.enabled should be true when github.enabled = true"
	);
}

#[test]
fn load_explicit_git_disabled_overrides_derived_default() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[github]\nenabled = true\n[git]\nenabled = false\n",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(&env).unwrap();
	assert!(loaded.github.enabled);
	assert!(
		!loaded.git.enabled(),
		"explicit [git].enabled = false must not be overridden"
	);
}

#[test]
fn load_derives_branch_strategy_when_github_enabled() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[github]\nenabled = true\n",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(&env).unwrap();
	assert_eq!(
		loaded.git.strategy(),
		Strategy::Branch,
		"strategy should be derived as Branch when github.enabled = true"
	);
}

#[test]
fn load_derives_push_strategy_when_github_disabled() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[git]\nenabled = true\n",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(&env).unwrap();
	assert_eq!(
		loaded.git.strategy(),
		Strategy::Push,
		"strategy should be derived as Push when github is not enabled"
	);
}

#[test]
fn load_explicit_strategy_overrides_derived_default() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[github]\nenabled = true\n[git]\nstrategy = \"push\"\n",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(&env).unwrap();
	assert_eq!(
		loaded.git.strategy(),
		Strategy::Push,
		"explicit strategy must not be overridden by derivation"
	);
}
