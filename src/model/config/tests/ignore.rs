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
	let config: Config = toml::from_str(toml_str).unwrap();
	assert_eq!(
		config.global.ignore,
		vec!["example-*".to_string(), "internal-tool".to_string()]
	);
}

#[test]
fn config_roundtrip_with_global_ignore() {
	let dir = temp_dir();
	let mut global = GlobalConfig::default();
	global.ignore = vec!["example-*".to_string(), "internal-tool".to_string()];
	let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
		.with_global(global)
		.with_cargo(CargoConfig::enabled());
	config.with_env(make_env()).save().unwrap();
	let env = make_env_with_git(dir.path());
	let loaded = load(&env).unwrap();
	assert_eq!(
		loaded.global.ignore,
		vec!["example-*".to_string(), "internal-tool".to_string()]
	);
}

#[test]
fn load_projects_filters_ignored_packages() {
	// Set up a workspace with two packages; ignore one by exact name.
	let dir = temp_dir();
	let abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let mut global = GlobalConfig::default();
	global.ignore = vec!["internal-tool".to_string()];
	let config = Config::new(&abs)
		.with_global(global)
		.with_cargo(CargoConfig::enabled())
		.with_env(make_env());

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

	let adapters = config.create_adapters().unwrap();
	let projects = config.load_projects_for_adapters(&adapters).unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "app");
}

#[test]
fn load_projects_filters_by_glob_pattern() {
	// Wildcard pattern: ignore all packages matching "example-*".
	let dir = temp_dir();
	let abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let mut global = GlobalConfig::default();
	global.ignore = vec!["example-*".to_string()];
	let config = Config::new(&abs)
		.with_global(global)
		.with_cargo(CargoConfig::enabled())
		.with_env(make_env());

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

	let adapters = config.create_adapters().unwrap();
	let projects = config.load_projects_for_adapters(&adapters).unwrap();

	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "core");
}

#[test]
fn load_projects_ignore_invalid_glob_fails() {
	let dir = temp_dir();
	let abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let mut global = GlobalConfig::default();
	global.ignore = vec!["[invalid".to_string()];
	let config = Config::new(&abs)
		.with_global(global)
		.with_cargo(CargoConfig::enabled())
		.with_env(make_env());

	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let adapters = config.create_adapters().unwrap();
	let result = config.load_projects_for_adapters(&adapters);
	assert!(result.is_err());
	let err = result.unwrap_err().to_string();
	assert!(
		err.contains("Invalid ignore glob pattern"),
		"Expected 'Invalid ignore glob pattern' error, got: {err}"
	);
}

#[test]
fn load_projects_ignore_no_match_warns() {
	// A pattern that matches nothing should succeed (just log a warning).
	let dir = temp_dir();
	let abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let mut global = GlobalConfig::default();
	global.ignore = vec!["nonexistent-package".to_string()];
	let config = Config::new(&abs)
		.with_global(global)
		.with_cargo(CargoConfig::enabled())
		.with_env(make_env());

	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let adapters = config.create_adapters().unwrap();
	let projects = config.load_projects_for_adapters(&adapters).unwrap();

	// app is still returned; the no-match pattern just warns
	assert_eq!(projects.len(), 1);
	assert_eq!(projects[0].name(), "app");
}

#[test]
fn load_projects_ignoring_all_packages_fails_with_informative_error() {
	// When all projects are filtered by ignore patterns, the error message
	// should mention the ignore patterns rather than the package manager config.
	let dir = temp_dir();
	let abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let mut global = GlobalConfig::default();
	global.ignore = vec!["app".to_string()];
	let config = Config::new(&abs)
		.with_global(global)
		.with_cargo(CargoConfig::enabled())
		.with_env(make_env());

	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let adapters = config.create_adapters().unwrap();
	let result = config.load_projects_for_adapters(&adapters);

	assert!(result.is_err());
	let err = result.unwrap_err().to_string();
	assert!(
		err.contains("excluded by [global].ignore"),
		"Expected informative error about ignore patterns, got: {err}"
	);
}
