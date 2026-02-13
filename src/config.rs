use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Supported package managers for project configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
	/// Node Package Manager (npm).
	Npm,
	/// Rust's Cargo package manager.
	Cargo,
}

/// Configuration for an individual package manager.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManagerConfig {
	/// Whether this package manager is enabled for the project.
	#[serde(default)]
	pub enabled: bool,
}

/// Chronicle configuration for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Config {
	/// Configuration for npm package manager.
	#[serde(default)]
	pub npm: PackageManagerConfig,
	/// Configuration for Cargo package manager.
	#[serde(default)]
	pub cargo: PackageManagerConfig,
}

impl Config {
	/// Creates a new config with only the specified package manager enabled.
	pub fn with_package_manager(pm: PackageManager) -> Self {
		let mut config = Self::default();
		match pm {
			PackageManager::Npm => config.npm.enabled = true,
			PackageManager::Cargo => config.cargo.enabled = true,
		}
		config
	}

	/// Returns an iterator over all enabled package managers.
	pub fn enabled_package_managers(&self) -> impl Iterator<Item = PackageManager> + '_ {
		[
			(PackageManager::Npm, &self.npm),
			(PackageManager::Cargo, &self.cargo),
		]
		.into_iter()
		.filter(|(_, config)| config.enabled)
		.map(|(pm, _)| pm)
	}
}

fn path(git_root: &Path) -> PathBuf {
	git_root.join(".chronicle/config.toml")
}

/// Checks if a Chronicle configuration file exists in the repository.
///
/// Returns `true` if `.chronicle/config.toml` exists at the given git root.
pub fn exists(git_root: &Path) -> bool {
	path(git_root).exists()
}

/// Loads the Chronicle configuration from the repository.
///
/// Reads and parses `.chronicle/config.toml` from the given git root.
///
/// # Errors
///
/// Returns an error if the config file cannot be read or parsed.
pub fn load(git_root: &Path) -> anyhow::Result<Config> {
	let path = path(git_root);
	let contents = std::fs::read_to_string(&path)
		.with_context(|| format!("Failed to read config file: {}", path.display()))?;
	let config: Config =
		toml::from_str(&contents).with_context(|| "Failed to parse config.toml")?;
	Ok(config)
}

/// Creates a new Chronicle configuration file in the repository.
///
/// Writes the configuration to `.chronicle/config.toml`, creating the
/// `.chronicle` directory if it doesn't exist.
///
/// # Errors
///
/// Returns an error if the directory cannot be created or the file cannot be written.
pub fn create(git_root: &Path, config: &Config) -> anyhow::Result<PathBuf> {
	let path = path(git_root);
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)
			.with_context(|| format!("Failed to create directory: {}", parent.display()))?;
	}
	let contents = toml::to_string_pretty(config).context("Failed to serialize config")?;
	std::fs::write(&path, contents)
		.with_context(|| format!("Failed to create config: {}", path.display()))?;
	Ok(path)
}

#[cfg(test)]
mod tests {
	use super::*;
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
		let config = Config::with_package_manager(PackageManager::Cargo);
		create(dir.path(), &config).unwrap();
		assert!(exists(dir.path()));
	}

	#[test]
	fn create_creates_config_file() {
		let dir = temp_dir();
		let config = Config::with_package_manager(PackageManager::Npm);
		let path = create(dir.path(), &config).unwrap();
		assert!(path.exists());
		assert_eq!(path, dir.path().join(".chronicle/config.toml"));
	}

	#[test]
	fn create_creates_directory_if_needed() {
		let dir = temp_dir();
		let config = Config::with_package_manager(PackageManager::Cargo);
		create(dir.path(), &config).unwrap();
		assert!(dir.path().join(".chronicle").is_dir());
	}

	#[test]
	fn load_reads_config_file() {
		let dir = temp_dir();
		let config = Config::with_package_manager(PackageManager::Npm);
		create(dir.path(), &config).unwrap();

		let loaded = load(dir.path()).unwrap();
		assert_eq!(loaded, config);
	}

	#[test]
	fn load_fails_when_no_config() {
		let dir = temp_dir();
		let result = load(dir.path());
		assert!(result.is_err());
	}

	#[test]
	fn load_fails_on_invalid_toml() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(config_dir.join("config.toml"), "invalid toml {{{").unwrap();

		let result = load(dir.path());
		assert!(result.is_err());
	}

	#[test]
	fn load_succeeds_with_empty_config() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(config_dir.join("config.toml"), "").unwrap();

		let config = load(dir.path()).unwrap();
		assert!(!config.npm.enabled);
		assert!(!config.cargo.enabled);
	}

	#[test]
	fn package_manager_config_defaults_to_disabled() {
		let config = PackageManagerConfig::default();
		assert!(!config.enabled);
	}

	#[test]
	fn config_defaults_all_disabled() {
		let config = Config::default();
		assert!(!config.npm.enabled);
		assert!(!config.cargo.enabled);
	}

	#[test]
	fn config_with_package_manager_enables_npm() {
		let config = Config::with_package_manager(PackageManager::Npm);
		assert!(config.npm.enabled);
		assert!(!config.cargo.enabled);
	}

	#[test]
	fn config_with_package_manager_enables_cargo() {
		let config = Config::with_package_manager(PackageManager::Cargo);
		assert!(!config.npm.enabled);
		assert!(config.cargo.enabled);
	}

	#[test]
	fn enabled_package_managers_returns_empty_when_none_enabled() {
		let config = Config::default();
		let enabled: Vec<_> = config.enabled_package_managers().collect();
		assert!(enabled.is_empty());
	}

	#[test]
	fn enabled_package_managers_returns_npm_when_enabled() {
		let config = Config::with_package_manager(PackageManager::Npm);
		let enabled: Vec<_> = config.enabled_package_managers().collect();
		assert_eq!(enabled, vec![PackageManager::Npm]);
	}

	#[test]
	fn enabled_package_managers_returns_cargo_when_enabled() {
		let config = Config::with_package_manager(PackageManager::Cargo);
		let enabled: Vec<_> = config.enabled_package_managers().collect();
		assert_eq!(enabled, vec![PackageManager::Cargo]);
	}

	#[test]
	fn enabled_package_managers_returns_both_when_both_enabled() {
		let mut config = Config::default();
		config.npm.enabled = true;
		config.cargo.enabled = true;
		let enabled: Vec<_> = config.enabled_package_managers().collect();
		assert_eq!(enabled, vec![PackageManager::Npm, PackageManager::Cargo]);
	}

	#[test]
	fn config_serializes_with_sections() {
		let config = Config::with_package_manager(PackageManager::Npm);
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
	fn config_roundtrip() {
		let dir = temp_dir();

		for pm in [PackageManager::Npm, PackageManager::Cargo] {
			let config = Config::with_package_manager(pm);
			create(dir.path(), &config).unwrap();
			let loaded = load(dir.path()).unwrap();
			let enabled: Vec<_> = loaded.enabled_package_managers().collect();
			assert_eq!(enabled, vec![pm]);
		}
	}
}
