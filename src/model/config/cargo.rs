//! Cargo package manager configuration.

use serde::{Deserialize, Serialize};

use crate::path::AbsolutePath;

/// Configuration for Cargo package manager.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoConfig {
	/// Whether this package manager is enabled for the project.
	#[serde(default)]
	pub enabled: bool,
	/// Optional path to the package manager root, relative to the git root.
	///
	/// When set, the package manager will look for its manifest files in this
	/// subdirectory instead of the git repository root.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub path: Option<String>,
}

impl CargoConfig {
	/// Creates a new enabled cargo configuration.
	pub fn enabled() -> Self {
		Self {
			enabled: true,
			..Default::default()
		}
	}

	/// Returns the resolved root directory for this package manager.
	///
	/// If a `path` is configured, returns `adapter_root` joined with that path.
	/// Otherwise, returns a copy of `adapter_root`.
	pub(crate) fn resolve_root(&self, git_workdir: &AbsolutePath) -> anyhow::Result<AbsolutePath> {
		super::resolve_root(&self.path, git_workdir)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn cargo_config_defaults_to_disabled() {
		let config = CargoConfig::default();
		assert!(!config.enabled);
		assert_eq!(config.path, None);
	}

	#[test]
	fn cargo_config_enabled_creates_enabled_config() {
		let config = CargoConfig::enabled();
		assert!(config.enabled);
		assert_eq!(config.path, None);
	}

	#[test]
	fn cargo_config_resolve_root_without_path() {
		let config = CargoConfig {
			enabled: true,
			path: None,
		};
		let dir = tempfile::tempdir().unwrap();
		let git_workdir = AbsolutePath::new(dir.path()).unwrap();
		let resolved = config.resolve_root(&git_workdir).unwrap();
		assert_eq!(resolved, git_workdir);
	}

	#[test]
	fn cargo_config_resolve_root_with_path() {
		let dir = tempfile::tempdir().unwrap();
		let subdir = dir.path().join("rust-workspace");
		std::fs::create_dir(&subdir).unwrap();
		let config = CargoConfig {
			enabled: true,
			path: Some("rust-workspace".to_string()),
		};
		let git_workdir = AbsolutePath::new(dir.path()).unwrap();
		let resolved = config.resolve_root(&git_workdir).unwrap();
		assert_eq!(*resolved, *AbsolutePath::new(&subdir).unwrap());
	}

	#[test]
	fn cargo_config_resolve_root_rejects_traversal() {
		let outer = tempfile::tempdir().unwrap();
		let repo = outer.path().join("repo");
		std::fs::create_dir(&repo).unwrap();
		let config = CargoConfig {
			enabled: true,
			path: Some("../escape".to_string()),
		};
		let escape_dir = outer.path().join("escape");
		std::fs::create_dir(&escape_dir).unwrap();
		let git_workdir = AbsolutePath::new(&repo).unwrap();
		let result = config.resolve_root(&git_workdir);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("escapes repository root")
		);
	}
}
