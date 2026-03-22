use super::*;

#[test]
fn exists_returns_false_when_no_config() {
	let dir = temp_dir();
	assert!(!exists(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&crate::filesystem::LocalFilesystem
	));
}

#[test]
fn exists_returns_true_when_config_exists() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled());
	config.save().unwrap();
	assert!(exists(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&crate::filesystem::LocalFilesystem
	));
}

#[test]
fn create_creates_config_file() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_npm(NpmConfig::enabled());
	let path = config.save().unwrap();
	assert!(path.exists());
	assert_eq!(path, dir.path().join(".cursus/config.toml"));
}

#[test]
fn create_creates_directory_if_needed() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled());
	config.save().unwrap();
	assert!(dir.path().join(".cursus").is_dir());
}

#[test]
fn load_reads_config_file() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_npm(NpmConfig::enabled());
	config.save().unwrap();

	let loaded = load(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	)
	.unwrap();
	// After load, strategy is derived: Push (no github)
	assert!(loaded.npm.enabled);
	assert!(!loaded.cargo.enabled);
	assert_eq!(loaded.git.strategy(), Strategy::Push);
}

#[test]
fn load_fails_when_no_config() {
	let dir = temp_dir();
	let result = load(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	);
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No configuration found")
	);
}

#[test]
fn load_fails_on_invalid_toml() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), "invalid toml {{{").unwrap();

	let result = load(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	);
	assert!(result.is_err());
}

#[test]
fn load_fails_with_empty_config() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), "").unwrap();

	let result = load(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	);
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("at least one package manager")
	);
}

#[test]
fn load_succeeds_with_one_package_manager() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled());
	config.save().unwrap();

	let loaded = load(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	)
	.unwrap();
	// After load, strategy is derived: Push (no github)
	assert!(loaded.cargo.enabled);
	assert_eq!(loaded.git.strategy(), Strategy::Push);
}

#[test]
fn git_workdir_returns_some_after_new() {
	let dir = temp_dir();
	let abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let config = Config::new(&abs);
	assert_eq!(
		config.git_workdir(),
		Some(&abs),
		"git_workdir() should return Some after Config::new"
	);
}

#[test]
fn load_impl_fails_when_no_config_file() {
	// Call load_impl directly to cover the non-test-support `load` code path,
	// which is otherwise compiled out when the test-support feature is active.
	let dir = temp_dir();
	let result = load_impl(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	);
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No configuration found"),
		"Expected 'No configuration found' from load_impl"
	);
}

#[test]
fn load_fails_on_old_run_until_field() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[git]\nrun_until = \"push\"\n",
	)
	.unwrap();

	let err = load(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	)
	.unwrap_err();
	let chain = format!("{err:#}");
	assert!(
		chain.contains("unknown field"),
		"Expected 'unknown field' error for run_until, got: {chain}"
	);
}
