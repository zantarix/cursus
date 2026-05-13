use super::*;

#[tokio::test]
async fn exists_returns_false_when_no_config() {
	let dir = temp_dir();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let env = make_env_with_git(dir.path());
	assert!(!exists(env.fs(), env.git().path()).await.unwrap());
}

#[tokio::test]
async fn exists_returns_true_when_config_exists() {
	let dir = temp_dir();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let env = make_env_with_git(dir.path());
	let config = Config::new().with_cargo(CargoConfig::enabled());
	config.save(env.fs(), env.git().path()).await.unwrap();
	assert!(exists(env.fs(), env.git().path()).await.unwrap());
}

#[tokio::test]
async fn create_creates_config_file() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let config = Config::new().with_npm(NpmConfig::enabled());
	let path = config.save(env.fs(), env.git().path()).await.unwrap();
	assert!(path.exists());
	assert_eq!(path, dir.path().join(".cursus/config.toml"));
}

#[tokio::test]
async fn create_creates_directory_if_needed() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let config = Config::new().with_cargo(CargoConfig::enabled());
	config.save(env.fs(), env.git().path()).await.unwrap();
	assert!(dir.path().join(".cursus").is_dir());
}

#[tokio::test]
async fn load_reads_config_file() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let config = Config::new().with_npm(NpmConfig::enabled());
	config.save(env.fs(), env.git().path()).await.unwrap();

	let loaded = load(env.fs(), env.git().path()).await.unwrap().unwrap();
	// After load, strategy is derived: Push (no github)
	assert!(loaded.npm.enabled);
	assert!(!loaded.cargo.enabled);
	assert_eq!(loaded.git.strategy(), Strategy::Push);
}

#[tokio::test]
async fn load_returns_none_when_no_config() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let result = load(env.fs(), env.git().path()).await;
	assert!(result.is_ok());
	assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn load_fails_on_invalid_toml() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), "invalid toml {{{").unwrap();

	let env = make_env_with_git(dir.path());
	let result = load(env.fs(), env.git().path()).await;
	assert!(result.is_err());
}

#[tokio::test]
async fn load_fails_with_empty_config() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), "").unwrap();

	let env = make_env_with_git(dir.path());
	let result = load(env.fs(), env.git().path()).await;
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
	let env = make_env_with_git(dir.path());
	let config = Config::new().with_cargo(CargoConfig::enabled());
	config.save(env.fs(), env.git().path()).await.unwrap();

	let loaded = load(env.fs(), env.git().path()).await.unwrap().unwrap();
	// After load, strategy is derived: Push (no github)
	assert!(loaded.cargo.enabled);
	assert_eq!(loaded.git.strategy(), Strategy::Push);
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
	let err = load(env.fs(), env.git().path()).await.unwrap_err();
	let chain = format!("{err:#}");
	assert!(
		chain.contains("unknown field"),
		"Expected 'unknown field' error for run_until, got: {chain}"
	);
}

#[tokio::test]
async fn load_fails_when_both_forges_enabled() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[github]\nenabled = true\n[gitlab]\nenabled = true\n",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let err = load(env.fs(), env.git().path()).await.unwrap_err();
	let msg = err.to_string();
	assert!(
		msg.contains("[github].enabled"),
		"expected message to name [github].enabled, got: {msg}"
	);
	assert!(
		msg.contains("[gitlab].enabled"),
		"expected message to name [gitlab].enabled, got: {msg}"
	);
	assert!(
		msg.contains("At most one forge section may have `enabled = true`"),
		"expected rule statement, got: {msg}"
	);
	assert!(
		msg.contains("set the others to `false`"),
		"expected actionable fix, got: {msg}"
	);
}

#[tokio::test]
async fn load_succeeds_when_only_github_enabled() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[github]\nenabled = true\n[gitlab]\nenabled = false\n",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(env.fs(), env.git().path()).await.unwrap().unwrap();
	assert!(loaded.github.enabled);
	assert!(!loaded.gitlab.enabled);
}

#[tokio::test]
async fn load_succeeds_when_only_gitlab_enabled() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(
		config_dir.join("config.toml"),
		"[cargo]\nenabled = true\n[github]\nenabled = false\n[gitlab]\nenabled = true\n",
	)
	.unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(env.fs(), env.git().path()).await.unwrap().unwrap();
	assert!(!loaded.github.enabled);
	assert!(loaded.gitlab.enabled);
}

#[tokio::test]
async fn load_succeeds_when_no_forge_enabled() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), "[cargo]\nenabled = true\n").unwrap();

	let env = make_env_with_git(dir.path());
	let loaded = load(env.fs(), env.git().path()).await.unwrap().unwrap();
	assert!(!loaded.github.enabled);
	assert!(!loaded.gitlab.enabled);
}

#[tokio::test]
async fn load_rejects_oversized_config() {
	let dir = temp_dir();
	let config_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	// Write a file larger than 256 KiB.
	let oversized = vec![b'x'; 257 * 1024];
	std::fs::write(config_dir.join("config.toml"), &oversized).unwrap();

	let env = make_env_with_git(dir.path());
	let err = load(env.fs(), env.git().path()).await.unwrap_err();
	assert!(
		err.to_string().contains("too large"),
		"unexpected error: {err}"
	);
}
