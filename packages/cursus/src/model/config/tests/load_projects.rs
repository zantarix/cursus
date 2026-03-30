use super::*;

#[tokio::test]
async fn load_projects_succeeds_with_cargo_manifest() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let config = Config::new().with_cargo(CargoConfig::enabled());
	config.save(env.fs(), env.git().path()).await.unwrap();
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"test-package\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let config = load(env.fs(), env.git().path()).await.unwrap().unwrap();
	let projects = config.load_projects(&env).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "test-package");
}

#[tokio::test]
async fn load_projects_succeeds_with_npm_manifest() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let config = Config::new().with_npm(NpmConfig::enabled());
	config.save(env.fs(), env.git().path()).await.unwrap();
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "test-package", "version": "0.1.0"}"#,
	)
	.unwrap();

	let config = load(env.fs(), env.git().path()).await.unwrap().unwrap();
	let projects = config.load_projects(&env).await.unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "test-package");
}

#[tokio::test]
async fn load_projects_fails_when_no_projects_found() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let config = Config::new().with_cargo(CargoConfig::enabled());
	config.save(env.fs(), env.git().path()).await.unwrap();
	// No Cargo.toml file, so no projects will be found

	let config = load(env.fs(), env.git().path()).await.unwrap().unwrap();
	let result = config.load_projects(&env).await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No projects found")
	);
}
