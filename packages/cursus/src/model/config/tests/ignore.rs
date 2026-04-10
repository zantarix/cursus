// --- ignore field tests ---

use super::*;

#[test]
fn global_config_defaults_to_empty_ignore() {
	let global = GlobalConfig::default();
	assert!(global.ignore.is_empty());
}

#[test]
fn config_deserializes_with_global_ignore() {
	let toml_str = r#"
[global]
ignore = ["example-*", "internal-tool"]

[npm]
enabled = true
"#;
	let config: ConfigData = toml::from_str(toml_str).unwrap();
	assert_eq!(
		config.global.ignore,
		vec!["example-*".to_string(), "internal-tool".to_string()]
	);
}

#[tokio::test]
async fn config_roundtrip_with_global_ignore() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let global = GlobalConfig {
		ignore: vec!["example-*".to_string(), "internal-tool".to_string()],
		..Default::default()
	};
	let config = Config::new()
		.with_global(global)
		.with_cargo(CargoConfig::enabled());
	config.save(env.fs(), env.git().path()).await.unwrap();
	let loaded = load(env.fs(), env.git().path()).await.unwrap().unwrap();
	assert_eq!(
		loaded.global.ignore,
		vec!["example-*".to_string(), "internal-tool".to_string()]
	);
}

#[tokio::test]
async fn load_projects_filters_ignored_packages() {
	// Set up a workspace with two packages; ignore one by exact name.
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let global = GlobalConfig {
		ignore: vec!["internal-tool".to_string()],
		..Default::default()
	};
	let config = Config::new()
		.with_global(global)
		.with_cargo(CargoConfig::enabled());

	// Create a workspace with two members.
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[workspace]\nmembers = [\"app\", \"internal-tool\"]\n",
	)
	.unwrap();
	for (name, version) in [("app", "0.1.0"), ("internal-tool", "0.1.0")] {
		let pkg_dir = dir.path().join(name);
		std::fs::create_dir_all(pkg_dir.join("src")).unwrap();
		std::fs::write(
			pkg_dir.join("Cargo.toml"),
			format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
		)
		.unwrap();
		std::fs::write(pkg_dir.join("src/lib.rs"), "").unwrap();
	}

	let adapters = config.create_adapters(&env).unwrap();
	let projects = config.load_projects_for_adapters(&adapters).await.unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "app");
}

#[tokio::test]
async fn load_projects_filters_by_glob_pattern() {
	// Wildcard pattern: ignore all packages matching "example-*".
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let global = GlobalConfig {
		ignore: vec!["example-*".to_string()],
		..Default::default()
	};
	let config = Config::new()
		.with_global(global)
		.with_cargo(CargoConfig::enabled());

	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[workspace]\nmembers = [\"core\", \"example-basic\", \"example-advanced\"]\n",
	)
	.unwrap();
	for (name, version) in [
		("core", "0.1.0"),
		("example-basic", "0.1.0"),
		("example-advanced", "0.1.0"),
	] {
		let pkg_dir = dir.path().join(name);
		std::fs::create_dir_all(pkg_dir.join("src")).unwrap();
		std::fs::write(
			pkg_dir.join("Cargo.toml"),
			format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
		)
		.unwrap();
		std::fs::write(pkg_dir.join("src/lib.rs"), "").unwrap();
	}

	let adapters = config.create_adapters(&env).unwrap();
	let projects = config.load_projects_for_adapters(&adapters).await.unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "core");
}

#[tokio::test]
async fn load_projects_ignore_invalid_glob_fails() {
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let global = GlobalConfig {
		ignore: vec!["[invalid".to_string()],
		..Default::default()
	};
	let config = Config::new()
		.with_global(global)
		.with_cargo(CargoConfig::enabled());

	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let adapters = config.create_adapters(&env).unwrap();
	let result = config.load_projects_for_adapters(&adapters).await;
	assert!(result.is_err());
	let err = result.unwrap_err().to_string();
	assert!(
		err.contains("Invalid ignore glob pattern"),
		"Expected 'Invalid ignore glob pattern' error, got: {err}"
	);
}

#[tokio::test]
async fn load_projects_ignore_no_match_warns() {
	// A pattern that matches nothing should succeed (just log a warning).
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let global = GlobalConfig {
		ignore: vec!["nonexistent-package".to_string()],
		..Default::default()
	};
	let config = Config::new()
		.with_global(global)
		.with_cargo(CargoConfig::enabled());

	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let adapters = config.create_adapters(&env).unwrap();
	let projects = config.load_projects_for_adapters(&adapters).await.unwrap();

	// app is still returned; the no-match pattern just warns
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "app");
}

#[tokio::test]
async fn load_projects_ignoring_all_packages_fails_with_informative_error() {
	// When all projects are filtered by ignore patterns, the error message
	// should mention the ignore patterns rather than the package manager config.
	let dir = temp_dir();
	let env = make_env_with_git(dir.path());
	let global = GlobalConfig {
		ignore: vec!["app".to_string()],
		..Default::default()
	};
	let config = Config::new()
		.with_global(global)
		.with_cargo(CargoConfig::enabled());

	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let adapters = config.create_adapters(&env).unwrap();
	let result = config.load_projects_for_adapters(&adapters).await;

	assert!(result.is_err());
	let err = result.unwrap_err().to_string();
	assert!(
		err.contains("excluded by [global].ignore"),
		"Expected informative error about ignore patterns, got: {err}"
	);
}
