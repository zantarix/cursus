use super::*;

#[test]
fn config_defaults_all_disabled() {
	let config = Config {
		global: GlobalConfig::default(),
		npm: NpmConfig::default(),
		cargo: CargoConfig::default(),
		git: GitConfig::default(),
		github: GitHubConfig::default(),
		linked_versions: LinkedVersionsConfig::default(),
		prepare: PrepareConfig::default(),
		git_workdir: None,
		env: None,
	};
	assert!(!config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_npm_does_not_force_enabled() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_npm(NpmConfig::default());
	assert!(!config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_cargo_does_not_force_enabled() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::default());
	assert!(!config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_npm_enabled_enables_npm() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_npm(NpmConfig::enabled());
	assert!(config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_cargo_enabled_enables_cargo() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled());
	assert!(!config.npm.enabled);
	assert!(config.cargo.enabled);
}

#[test]
fn enabled_package_managers_returns_empty_when_none_enabled() {
	let config = Config {
		global: GlobalConfig::default(),
		npm: NpmConfig::default(),
		cargo: CargoConfig::default(),
		git: GitConfig::default(),
		github: GitHubConfig::default(),
		linked_versions: LinkedVersionsConfig::default(),
		prepare: PrepareConfig::default(),
		git_workdir: None,
		env: None,
	};
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert!(enabled.is_empty());
}

#[test]
fn enabled_package_managers_returns_npm_when_enabled() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_npm(NpmConfig::enabled());
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert_eq!(enabled, vec![PackageManager::Npm]);
}

#[test]
fn enabled_package_managers_returns_cargo_when_enabled() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled());
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert_eq!(enabled, vec![PackageManager::Cargo]);
}

#[test]
fn enabled_package_managers_returns_both_when_both_enabled() {
	let mut config = Config {
		global: GlobalConfig::default(),
		npm: NpmConfig::default(),
		cargo: CargoConfig::default(),
		git: GitConfig::default(),
		github: GitHubConfig::default(),
		linked_versions: LinkedVersionsConfig::default(),
		prepare: PrepareConfig::default(),
		git_workdir: None,
		env: None,
	};
	config.npm.enabled = true;
	config.cargo.enabled = true;
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert_eq!(enabled, vec![PackageManager::Npm, PackageManager::Cargo]);
}

#[test]
fn with_env_sets_env_and_env_returns_it() {
	let dir = temp_dir();
	let env = make_env();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled())
		.with_env(env);
	assert!(config.env().is_some());
}

#[test]
fn env_returns_none_when_not_set() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled());
	assert!(config.env().is_none());
}

#[test]
fn create_adapters_fails_when_env_not_set() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled());
	let result = config.create_adapters();
	assert!(result.is_err());
	assert!(
		result.unwrap_err().to_string().contains("env not set"),
		"Expected 'env not set' error"
	);
}
