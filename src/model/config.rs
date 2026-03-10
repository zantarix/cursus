use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::command::CommandRunner;
use crate::git::GitConfig;
use crate::github::GitHubConfig;
use crate::package_manager::{
	self, CargoAdapter, CargoConfig, NpmAdapter, NpmConfig, PackageManagerAdapter, Project,
};
use crate::path::AbsolutePath;

/// Global configuration settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GlobalConfig {
	/// Disable warnings about circular dependencies in monorepos.
	pub disable_dependency_cycle_warnings: bool,
}

/// Supported package managers for project configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
	/// Node Package Manager (npm).
	Npm,
	/// Rust's Cargo package manager.
	Cargo,
}

/// Chronicle configuration for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
	/// Global configuration settings.
	#[serde(default)]
	pub global: GlobalConfig,
	/// Configuration for npm package manager.
	#[serde(default)]
	pub npm: NpmConfig,
	/// Configuration for Cargo package manager.
	#[serde(default)]
	pub cargo: CargoConfig,
	/// Git lifecycle automation configuration.
	#[serde(default)]
	pub git: GitConfig,
	/// GitHub Releases configuration.
	#[serde(default)]
	pub github: GitHubConfig,
	/// Git repository root path.
	///
	/// Always `Some` when constructed via [`Config::new`] or [`load`].
	/// `None` only when deserialized directly from TOML (which skips this field).
	#[serde(skip)]
	git_workdir: Option<AbsolutePath>,
}

impl Config {
	/// Creates a new config with all package managers disabled.
	pub fn new(git_workdir: &AbsolutePath) -> Self {
		Self {
			global: GlobalConfig::default(),
			npm: NpmConfig::default(),
			cargo: CargoConfig::default(),
			git: GitConfig::default(),
			github: GitHubConfig::default(),
			git_workdir: Some(git_workdir.clone()),
		}
	}

	/// Sets global configuration (builder pattern).
	pub fn with_global(mut self, config: GlobalConfig) -> Self {
		self.global = config;
		self
	}

	/// Sets npm configuration (builder pattern).
	pub fn with_npm(mut self, config: NpmConfig) -> Self {
		self.npm = config;
		self
	}

	/// Sets cargo configuration (builder pattern).
	pub fn with_cargo(mut self, config: CargoConfig) -> Self {
		self.cargo = config;
		self
	}

	/// Sets git lifecycle configuration (builder pattern).
	pub fn with_git(mut self, config: GitConfig) -> Self {
		self.git = config;
		self
	}

	/// Sets GitHub Releases configuration (builder pattern).
	pub fn with_github(mut self, config: GitHubConfig) -> Self {
		self.github = config;
		self
	}

	/// Returns the git repository root path, if set.
	///
	/// Returns `None` only when `Config` was deserialized directly from TOML without
	/// going through [`load`]. All production code paths set this via [`Config::new`]
	/// or [`load`].
	pub fn git_workdir(&self) -> Option<&AbsolutePath> {
		self.git_workdir.as_ref()
	}

	/// Returns an iterator over all enabled package managers.
	pub fn enabled_package_managers(&self) -> impl Iterator<Item = PackageManager> {
		let mut managers = Vec::new();
		if self.npm.enabled {
			managers.push(PackageManager::Npm);
		}
		if self.cargo.enabled {
			managers.push(PackageManager::Cargo);
		}
		managers.into_iter()
	}

	/// Creates package manager adapters for all enabled package managers.
	///
	/// Returns a vector of adapter instances wrapped in `Arc` for shared ownership.
	///
	/// # Errors
	///
	/// Returns an error if this `Config` was not constructed via [`Config::new`] or [`load`].
	pub fn create_adapters(
		&self,
		runner: Arc<dyn CommandRunner>,
	) -> anyhow::Result<Vec<Arc<dyn PackageManagerAdapter>>> {
		let workdir = self.git_workdir.as_ref().context(
			"git_workdir not set — Config must be constructed via Config::new() or config::load()",
		)?;
		Ok(self
			.enabled_package_managers()
			.map(|pm| -> Arc<dyn PackageManagerAdapter> {
				match pm {
					PackageManager::Npm => Arc::new(NpmAdapter::new(
						self.npm.clone(),
						workdir.clone(),
						Arc::clone(&runner),
					)),
					PackageManager::Cargo => Arc::new(CargoAdapter::new(
						self.cargo.clone(),
						workdir.clone(),
						Arc::clone(&runner),
					)),
				}
			})
			.collect())
	}

	/// Loads all projects for the given adapters.
	///
	/// Enumerates all projects from the provided adapters.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - Projects cannot be enumerated
	/// - No projects are found
	pub fn load_projects_for_adapters(
		&self,
		adapters: &[Arc<dyn PackageManagerAdapter>],
	) -> anyhow::Result<Vec<Project>> {
		let projects = package_manager::enumerate_projects(adapters.to_vec())?;

		if projects.is_empty() {
			bail!("No projects found. Check that your package manager configuration is correct.");
		}

		Ok(projects)
	}

	/// Loads all projects using the configuration.
	///
	/// Builds package manager adapters and enumerates all projects.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - Projects cannot be enumerated
	/// - No projects are found
	pub fn load_projects(&self, runner: Arc<dyn CommandRunner>) -> anyhow::Result<Vec<Project>> {
		let adapters = self.create_adapters(runner)?;
		self.load_projects_for_adapters(&adapters)
	}

	/// Saves the configuration to `.chronicle/config.toml`.
	///
	/// Creates the `.chronicle` directory if it doesn't exist.
	///
	/// # Errors
	///
	/// Returns an error if the directory cannot be created or the file cannot be written.
	pub fn save(&self) -> anyhow::Result<PathBuf> {
		let workdir = self.git_workdir.as_ref().context(
			"git_workdir not set — Config must be constructed via Config::new() or config::load()",
		)?;
		let path = path(workdir);
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent)
				.with_context(|| format!("Failed to create directory: {}", parent.display()))?;
		}
		let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
		std::fs::write(&path, contents)
			.with_context(|| format!("Failed to create config: {}", path.display()))?;
		Ok(path)
	}
}

fn path(git_workdir: &Path) -> PathBuf {
	git_workdir.join(".chronicle/config.toml")
}

/// Checks if a Chronicle configuration file exists in the repository.
///
/// Returns `true` if `.chronicle/config.toml` exists at the given git root.
pub fn exists(git_workdir: &Path) -> bool {
	path(git_workdir).exists()
}

fn load_impl(git_workdir: &AbsolutePath) -> anyhow::Result<Config> {
	if !exists(git_workdir) {
		bail!("No configuration found. Run 'chronicle init' to create one.");
	}

	let path = path(git_workdir);
	let contents = std::fs::read_to_string(&path)
		.with_context(|| format!("Failed to read config file: {}", path.display()))?;
	let mut config: Config =
		toml::from_str(&contents).with_context(|| "Failed to parse config.toml")?;

	// Validate that at least one package manager is enabled
	if config.enabled_package_managers().next().is_none() {
		bail!("Configuration must have at least one package manager enabled");
	}

	// Derived default: [github].enabled = true implies [git].enabled = true,
	// unless [git].enabled was explicitly set in the config file.
	if config.github.enabled && config.git.enabled.is_none() {
		config.git.enabled = Some(true);
	}

	// Derived default: strategy is Branch when github.enabled, else Push.
	// Only set when not explicitly configured.
	if config.git.strategy.is_none() {
		config.git.strategy = Some(if config.github.enabled {
			crate::git::Strategy::Branch
		} else {
			crate::git::Strategy::Push
		});
	}

	// Set the git root
	config.git_workdir = Some(git_workdir.clone());

	Ok(config)
}

/// Loads the Chronicle configuration from the repository.
///
/// Reads and parses `.chronicle/config.toml` from the given git root.
///
/// # Errors
///
/// Returns an error if the config file cannot be read or parsed.
#[cfg(feature = "test-support")]
pub fn load(git_workdir: &AbsolutePath) -> anyhow::Result<Config> {
	load_impl(git_workdir)
}

/// Loads the Chronicle configuration from the repository.
///
/// Reads and parses `.chronicle/config.toml` from the given git root.
///
/// # Errors
///
/// Returns an error if the config file cannot be read or parsed.
#[cfg(not(feature = "test-support"))]
pub(crate) fn load(git_workdir: &AbsolutePath) -> anyhow::Result<Config> {
	load_impl(git_workdir)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::github::GitHubConfig;
	use tempfile::TempDir;

	fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	#[test]
	fn exists_returns_false_when_no_config() {
		let dir = temp_dir();
		assert!(!exists(dir.path()));
	}

	#[test]
	fn exists_returns_true_when_config_exists() {
		let dir = temp_dir();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_cargo(CargoConfig::enabled());
		config.save().unwrap();
		assert!(exists(dir.path()));
	}

	#[test]
	fn create_creates_config_file() {
		let dir = temp_dir();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_npm(NpmConfig::enabled());
		let path = config.save().unwrap();
		assert!(path.exists());
		assert_eq!(path, dir.path().join(".chronicle/config.toml"));
	}

	#[test]
	fn create_creates_directory_if_needed() {
		let dir = temp_dir();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_cargo(CargoConfig::enabled());
		config.save().unwrap();
		assert!(dir.path().join(".chronicle").is_dir());
	}

	#[test]
	fn load_reads_config_file() {
		let dir = temp_dir();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_npm(NpmConfig::enabled());
		config.save().unwrap();

		let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		// After load, strategy is derived: Push (no github)
		assert!(loaded.npm.enabled);
		assert!(!loaded.cargo.enabled);
		assert_eq!(loaded.git.strategy, Some(crate::git::Strategy::Push));
	}

	#[test]
	fn load_fails_when_no_config() {
		let dir = temp_dir();
		let result = load(&crate::path::AbsolutePath::new(dir.path()).unwrap());
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
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(config_dir.join("config.toml"), "invalid toml {{{").unwrap();

		let result = load(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		assert!(result.is_err());
	}

	#[test]
	fn load_fails_with_empty_config() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(config_dir.join("config.toml"), "").unwrap();

		let result = load(&crate::path::AbsolutePath::new(dir.path()).unwrap());
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

		let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		// After load, strategy is derived: Push (no github)
		assert!(loaded.cargo.enabled);
		assert_eq!(loaded.git.strategy, Some(crate::git::Strategy::Push));
	}

	#[test]
	fn config_defaults_all_disabled() {
		let config = Config {
			global: GlobalConfig::default(),
			npm: NpmConfig::default(),
			cargo: CargoConfig::default(),
			git: GitConfig::default(),
			github: GitHubConfig::default(),
			git_workdir: None,
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
			git_workdir: None,
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
			git_workdir: None,
		};
		config.npm.enabled = true;
		config.cargo.enabled = true;
		let enabled: Vec<_> = config.enabled_package_managers().collect();
		assert_eq!(enabled, vec![PackageManager::Npm, PackageManager::Cargo]);
	}

	#[test]
	fn config_serializes_with_sections() {
		let dir = temp_dir();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_npm(NpmConfig::enabled());
		let toml_str = toml::to_string(&config).unwrap();
		assert!(toml_str.contains("[npm]"));
		assert!(toml_str.contains("enabled = true"));
	}

	#[test]
	fn config_deserializes_with_sections() {
		let config: Config = toml::from_str("[npm]\nenabled = true").unwrap();
		assert!(config.npm.enabled);
		assert!(!config.cargo.enabled);

		let config: Config = toml::from_str("[cargo]\nenabled = true").unwrap();
		assert!(!config.npm.enabled);
		assert!(config.cargo.enabled);
	}

	#[test]
	fn load_fails_on_unknown_top_level_field() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(config_dir.join("config.toml"), "[rust]\nenabled = true").unwrap();

		let err = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap_err();
		let chain = format!("{err:#}");
		assert!(
			chain.contains("unknown field"),
			"Expected 'unknown field' error, got: {chain}"
		);
	}

	#[test]
	fn load_fails_on_unknown_package_manager_field() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(
			config_dir.join("config.toml"),
			"[npm]\nenabled = true\nversion = \"1.0\"",
		)
		.unwrap();

		let err = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap_err();
		let chain = format!("{err:#}");
		assert!(
			chain.contains("unknown field"),
			"Expected 'unknown field' error, got: {chain}"
		);
	}

	#[test]
	fn deserialize_config_with_path() {
		let config: Config = toml::from_str("[npm]\nenabled = true\npath = \"frontend\"").unwrap();
		assert!(config.npm.enabled);
		assert_eq!(config.npm.path, Some("frontend".to_string()));
	}

	#[test]
	fn deserialize_config_without_path() {
		let config: Config = toml::from_str("[npm]\nenabled = true").unwrap();
		assert!(config.npm.enabled);
		assert_eq!(config.npm.path, None);
	}

	#[test]
	fn serialize_config_omits_none_path() {
		let dir = temp_dir();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_npm(NpmConfig::enabled());
		let toml_str = toml::to_string(&config).unwrap();
		assert!(!toml_str.contains("path"), "None path should be omitted");
	}

	#[test]
	fn serialize_config_includes_some_path() {
		let dir = temp_dir();
		let mut config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_npm(NpmConfig::enabled());
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
		let mut config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_npm(NpmConfig::enabled());
		config.npm.path = Some("frontend".to_string());
		config.save().unwrap();
		let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		assert_eq!(loaded.npm.path, Some("frontend".to_string()));
	}

	#[test]
	fn config_roundtrip() {
		let dir = temp_dir();

		for pm in [PackageManager::Npm, PackageManager::Cargo] {
			let config = match pm {
				PackageManager::Npm => {
					Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
						.with_npm(NpmConfig::enabled())
				}
				PackageManager::Cargo => {
					Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
						.with_cargo(CargoConfig::enabled())
				}
			};
			config.save().unwrap();
			let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
			let enabled: Vec<_> = loaded.enabled_package_managers().collect();
			assert_eq!(enabled, vec![pm]);
		}
	}

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

		let config = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let projects = config.load_projects(runner).unwrap();
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

		let config = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let projects = config.load_projects(runner).unwrap();
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

		let config = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let result = config.load_projects(runner);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("No projects found")
		);
	}

	#[test]
	fn global_config_defaults_to_warnings_enabled() {
		let global = GlobalConfig::default();
		assert!(!global.disable_dependency_cycle_warnings);
	}

	#[test]
	fn config_deserializes_without_global_section() {
		let config: Config = toml::from_str("[npm]\nenabled = true").unwrap();
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
		let config: Config = toml::from_str(toml_str).unwrap();
		assert!(config.npm.enabled);
		assert!(config.global.disable_dependency_cycle_warnings);
	}

	#[test]
	fn config_roundtrip_with_global() {
		let dir = temp_dir();
		let mut global = GlobalConfig::default();
		global.disable_dependency_cycle_warnings = true;
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_global(global)
			.with_npm(NpmConfig::enabled());
		config.save().unwrap();
		let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		assert!(loaded.global.disable_dependency_cycle_warnings);
	}

	#[test]
	fn global_config_unknown_field_fails() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(
			config_dir.join("config.toml"),
			"[global]\nunknown_field = true\n[npm]\nenabled = true",
		)
		.unwrap();

		let err = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap_err();
		let chain = format!("{err:#}");
		assert!(
			chain.contains("unknown field"),
			"Expected 'unknown field' error, got: {chain}"
		);
	}

	#[test]
	fn config_deserializes_github_section() {
		let config: Config =
			toml::from_str("[npm]\nenabled = true\n[github]\nenabled = true").unwrap();
		assert!(config.github.enabled);
	}

	#[test]
	fn config_github_unknown_field_fails() {
		let result: Result<Config, _> =
			toml::from_str("[npm]\nenabled = true\n[github]\nunknown = true");
		assert!(
			result.is_err(),
			"Expected error for unknown field in [github]"
		);
	}

	#[test]
	fn config_without_github_section_defaults_disabled() {
		let config: Config = toml::from_str("[npm]\nenabled = true").unwrap();
		assert!(!config.github.enabled);
	}

	#[test]
	fn load_github_enabled_derives_git_enabled() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(
			config_dir.join("config.toml"),
			"[cargo]\nenabled = true\n[github]\nenabled = true\n",
		)
		.unwrap();

		let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		assert!(loaded.github.enabled);
		assert_eq!(
			loaded.git.enabled,
			Some(true),
			"git.enabled should be derived Some(true) when github.enabled = true"
		);
	}

	#[test]
	fn load_explicit_git_disabled_overrides_derived_default() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(
			config_dir.join("config.toml"),
			"[cargo]\nenabled = true\n[github]\nenabled = true\n[git]\nenabled = false\n",
		)
		.unwrap();

		let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		assert!(loaded.github.enabled);
		assert_eq!(
			loaded.git.enabled,
			Some(false),
			"explicit [git].enabled = false must not be overridden"
		);
	}

	#[test]
	fn load_derives_branch_strategy_when_github_enabled() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(
			config_dir.join("config.toml"),
			"[cargo]\nenabled = true\n[github]\nenabled = true\n",
		)
		.unwrap();

		let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		assert_eq!(
			loaded.git.strategy,
			Some(crate::git::Strategy::Branch),
			"strategy should be derived as Branch when github.enabled = true"
		);
	}

	#[test]
	fn load_derives_push_strategy_when_github_disabled() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(
			config_dir.join("config.toml"),
			"[cargo]\nenabled = true\n[git]\nenabled = true\n",
		)
		.unwrap();

		let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		assert_eq!(
			loaded.git.strategy,
			Some(crate::git::Strategy::Push),
			"strategy should be derived as Push when github is not enabled"
		);
	}

	#[test]
	fn load_explicit_strategy_overrides_derived_default() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(
			config_dir.join("config.toml"),
			"[cargo]\nenabled = true\n[github]\nenabled = true\n[git]\nstrategy = \"push\"\n",
		)
		.unwrap();

		let loaded = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap();
		assert_eq!(
			loaded.git.strategy,
			Some(crate::git::Strategy::Push),
			"explicit strategy must not be overridden by derivation"
		);
	}

	#[test]
	fn load_fails_on_old_run_until_field() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(
			config_dir.join("config.toml"),
			"[cargo]\nenabled = true\n[git]\nrun_until = \"push\"\n",
		)
		.unwrap();

		let err = load(&crate::path::AbsolutePath::new(dir.path()).unwrap()).unwrap_err();
		let chain = format!("{err:#}");
		assert!(
			chain.contains("unknown field"),
			"Expected 'unknown field' error for run_until, got: {chain}"
		);
	}
}
