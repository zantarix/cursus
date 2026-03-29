use super::*;

#[tokio::test]
async fn exists_returns_false_when_no_config() {
	let dir = temp_dir();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let env = make_env_with_git(dir.path());
	assert!(!exists(&env).await.unwrap());
}

#[tokio::test]
async fn exists_returns_true_when_config_exists() {
	let dir = temp_dir();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let env = make_env_with_git(dir.path());
	let config = Config::new(&env).with_cargo(CargoConfig::enabled());
	config.save().await.unwrap();
	assert!(exists(&env).await.unwrap());
}

#[tokio::test]
async fn create_creates_config_file() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_npm(NpmConfig::enabled());
	let path = config.save().await.unwrap();
	assert!(path.exists());
	assert_eq!(path, dir.path().join(".cursus/config.toml"));
}

#[tokio::test]
async fn create_creates_directory_if_needed() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_cargo(CargoConfig::enabled());
	config.save().await.unwrap();
	assert!(dir.path().join(".cursus").is_dir());
}

#[tokio::test]
async fn load_reads_config_file() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_npm(NpmConfig::enabled());
	config.save().await.unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(&env).await.unwrap();
	// After load, strategy is derived: Push (no github)
	assert!(loaded.npm.enabled);
	assert!(!loaded.cargo.enabled);
	assert_eq!(loaded.git.strategy(), Strategy::Push);
}

#[tokio::test]
async fn load_fails_when_no_config() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let result = load(&env).await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No configuration found")
	);
}

#[tokio::test]
async fn load_fails_on_invalid_toml() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), "invalid toml {{{").unwrap();

	let env = make_env_with_git(dir.path());
	let result = load(&env).await;
	assert!(result.is_err());
}

#[tokio::test]
async fn load_fails_with_empty_config() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), "").unwrap();

	let env = make_env_with_git(dir.path());
	let result = load(&env).await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("at least one package manager")
	);
}

#[tokio::test]
async fn load_succeeds_with_one_package_manager() {
	let dir = temp_dir();
	let config = Config::new(&make_env_with_git(dir.path())).with_cargo(CargoConfig::enabled());
	config.save().await.unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(&env).await.unwrap();
	// After load, strategy is derived: Push (no github)
	assert!(loaded.cargo.enabled);
	assert_eq!(loaded.git.strategy(), Strategy::Push);
}

#[test]
fn git_workdir_returns_path_after_new() {
	let dir = temp_dir();
	let abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let env = make_env_with_git(dir.path());
	let config = Config::new(&env);
	assert_eq!(
		config.git_workdir(),
		&abs,
		"git_workdir() should return the env's git path after Config::new"
	);
}

#[tokio::test]
async fn load_impl_fails_when_no_config_file() {
	// Call load_impl directly to cover the non-test-support `load` code path,
	// which is otherwise compiled out when the test-support feature is active.
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let result = load_impl(&env).await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No configuration found"),
		"Expected 'No configuration found' from load_impl"
	);
}

#[tokio::test]
async fn load_fails_on_old_run_until_field() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[git]\nrun_until = \"push\"\n",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let err = load(&env).await.unwrap_err();
	let chain = format!("{err:#}");
	assert!(
		chain.contains("unknown field"),
		"Expected 'unknown field' error for run_until, got: {chain}"
	);
}
