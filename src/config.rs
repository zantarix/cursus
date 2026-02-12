use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Supported package managers for project configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
	/// Node Package Manager (npm).
	Npm,
	/// Rust's Cargo package manager.
	Cargo,
}

/// Chronicle configuration for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
	/// The package manager used by this project.
	pub package_manager: PackageManager,
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
		let config = Config {
			package_manager: PackageManager::Cargo,
		};
		create(dir.path(), &config).unwrap();
		assert!(exists(dir.path()));
	}

	#[test]
	fn create_creates_config_file() {
		let dir = temp_dir();
		let config = Config {
			package_manager: PackageManager::Npm,
		};
		let path = create(dir.path(), &config).unwrap();
		assert!(path.exists());
		assert_eq!(path, dir.path().join(".chronicle/config.toml"));
	}

	#[test]
	fn create_creates_directory_if_needed() {
		let dir = temp_dir();
		let config = Config {
			package_manager: PackageManager::Cargo,
		};
		create(dir.path(), &config).unwrap();
		assert!(dir.path().join(".chronicle").is_dir());
	}

	#[test]
	fn load_reads_config_file() {
		let dir = temp_dir();
		let config = Config {
			package_manager: PackageManager::Npm,
		};
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
	fn load_fails_on_missing_fields() {
		let dir = temp_dir();
		let config_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&config_dir).unwrap();
		std::fs::write(config_dir.join("config.toml"), "other_field = 123").unwrap();

		let result = load(dir.path());
		assert!(result.is_err());
	}

	#[test]
	fn package_manager_serializes_lowercase() {
		let config = Config {
			package_manager: PackageManager::Npm,
		};
		let toml = toml::to_string(&config).unwrap();
		assert!(toml.contains("npm"));

		let config = Config {
			package_manager: PackageManager::Cargo,
		};
		let toml = toml::to_string(&config).unwrap();
		assert!(toml.contains("cargo"));
	}

	#[test]
	fn package_manager_deserializes_lowercase() {
		let config: Config = toml::from_str("package_manager = \"npm\"").unwrap();
		assert_eq!(config.package_manager, PackageManager::Npm);

		let config: Config = toml::from_str("package_manager = \"cargo\"").unwrap();
		assert_eq!(config.package_manager, PackageManager::Cargo);
	}

	#[test]
	fn config_roundtrip() {
		let dir = temp_dir();

		for pm in [PackageManager::Npm, PackageManager::Cargo] {
			let config = Config {
				package_manager: pm,
			};
			create(dir.path(), &config).unwrap();
			let loaded = load(dir.path()).unwrap();
			assert_eq!(loaded.package_manager, pm);
		}
	}
}
