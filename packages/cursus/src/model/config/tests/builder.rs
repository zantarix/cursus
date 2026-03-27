use super::*;

#[test]
fn config_defaults_all_disabled() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path()));
	assert!(!config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_npm_does_not_force_enabled() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_npm(NpmConfig::default());
	assert!(!config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_cargo_does_not_force_enabled() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_cargo(CargoConfig::default());
	assert!(!config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_npm_enabled_enables_npm() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_npm(NpmConfig::enabled());
	assert!(config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_cargo_enabled_enables_cargo() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_cargo(CargoConfig::enabled());
	assert!(!config.npm.enabled);
	assert!(config.cargo.enabled);
}

#[test]
fn enabled_package_managers_returns_empty_when_none_enabled() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path()));
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert!(enabled.is_empty());
}

#[test]
fn enabled_package_managers_returns_npm_when_enabled() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_npm(NpmConfig::enabled());
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert_eq!(enabled, vec![PackageManager::Npm]);
}

#[test]
fn enabled_package_managers_returns_cargo_when_enabled() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_cargo(CargoConfig::enabled());
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert_eq!(enabled, vec![PackageManager::Cargo]);
}

#[test]
fn enabled_package_managers_returns_both_when_both_enabled() {
	let dir = temp_dir();
	let mut config = Config::new(&make_env_with_git(dir.path()));
	config.npm.enabled = true;
	config.cargo.enabled = true;
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert_eq!(enabled, vec![PackageManager::Npm, PackageManager::Cargo]);
}

#[test]
fn env_returns_env() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_cargo(CargoConfig::enabled());
	// env() now always returns &Env — just verify it's accessible
	let _env = config.env();
}
