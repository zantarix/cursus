use super::*;

#[test]
fn load_projects_succeeds_with_cargo_manifest() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled());
	config.save().unwrap();
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"test-package\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let config = load(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	)
	.unwrap();
	let projects = config.load_projects().unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "test-package");
}

#[test]
fn load_projects_succeeds_with_npm_manifest() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_npm(NpmConfig::enabled());
	config.save().unwrap();
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "test-package", "version": "0.1.0"}"#,
	)
	.unwrap();

	let config = load(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	)
	.unwrap();
	let projects = config.load_projects().unwrap();
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "test-package");
}

#[test]
fn load_projects_fails_when_no_projects_found() {
	let dir = temp_dir();
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled());
	config.save().unwrap();
	// No Cargo.toml file, so no projects will be found

	let config = load(
		&crate::path::AbsolutePath::new(dir.path()).unwrap(),
		&make_env(),
	)
	.unwrap();
	let result = config.load_projects();
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No projects found")
	);
}
