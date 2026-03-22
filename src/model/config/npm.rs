//! npm package manager configuration.

use serde::{Deserialize, Serialize};

use crate::path::AbsolutePath;

/// Access level for scoped npm packages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NpmAccess {
	/// Package is publicly accessible.
	Public,
	/// Package is restricted to organisation members.
	#[default]
	Restricted,
}

impl NpmAccess {
	/// Returns the string value used in npm CLI arguments.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Public => "public",
			Self::Restricted => "restricted",
		}
	}
}

/// Configuration for npm package manager.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpmConfig {
	/// Whether this package manager is enabled for the project.
	#[serde(default)]
	pub enabled: bool,
	/// Optional path to the package manager root, relative to the git root.
	///
	/// When set, the package manager will look for its manifest files in this
	/// subdirectory instead of the git repository root.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub path: Option<String>,
	/// Optional custom command to update the lock file after version bumps.
	///
	/// When set, this command will be executed to update the lock file. Otherwise,
	/// the package manager adapter will auto-detect the lock file type.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub lock_command: Option<String>,
	/// Access level for scoped packages.
	///
	/// Only used when publishing scoped packages (e.g., @scope/package).
	/// If not specified, defaults to [`NpmAccess::Restricted`].
	#[serde(default, skip_serializing_if = "Option::is_none")]
	access: Option<NpmAccess>,
}

impl NpmConfig {
	/// Creates a new enabled npm configuration.
	pub fn enabled() -> Self {
		Self {
			enabled: true,
			..Default::default()
		}
	}

	/// Returns the access level for scoped packages.
	///
	/// Defaults to [`NpmAccess::Restricted`] when not set.
	pub fn access(&self) -> NpmAccess {
		self.access.unwrap_or_default()
	}

	/// Sets the access level for scoped packages (builder pattern).
	pub fn with_access(mut self, access: NpmAccess) -> Self {
		self.access = Some(access);
		self
	}

	/// Sets the package manager root path (builder pattern).
	pub fn with_path(mut self, path: String) -> Self {
		self.path = Some(path);
		self
	}

	/// Sets the custom lock file command (builder pattern).
	pub fn with_lock_command(mut self, lock_command: String) -> Self {
		self.lock_command = Some(lock_command);
		self
	}

	/// Returns the resolved root directory for this package manager.
	///
	/// If a `path` is configured, returns `adapter_root` joined with that path.
	/// Otherwise, returns a copy of `adapter_root`.
	pub(crate) fn resolve_root(
		&self,
		git_workdir: &AbsolutePath,
		fs: &dyn crate::filesystem::Filesystem,
	) -> anyhow::Result<AbsolutePath> {
		super::resolve_root(&self.path, git_workdir, fs)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn npm_config_defaults_to_disabled() {
		let config = NpmConfig::default();
		assert!(!config.enabled);
		assert_eq!(config.path, None);
		assert_eq!(config.lock_command, None);
		assert_eq!(config.access(), NpmAccess::Restricted);
	}

	#[test]
	fn npm_config_enabled_creates_enabled_config() {
		let config = NpmConfig::enabled();
		assert!(config.enabled);
		assert_eq!(config.path, None);
		assert_eq!(config.lock_command, None);
		assert_eq!(config.access(), NpmAccess::Restricted);
	}

	#[test]
	fn npm_config_resolve_root_without_path() {
		let config = NpmConfig {
			enabled: true,
			path: None,
			lock_command: None,
			access: None,
		};
		let dir = tempfile::tempdir().unwrap();
		let git_workdir = AbsolutePath::new(dir.path()).unwrap();
		let resolved = config
			.resolve_root(&git_workdir, &crate::filesystem::LocalFilesystem)
			.unwrap();
		assert_eq!(resolved, git_workdir);
	}

	#[test]
	fn npm_config_resolve_root_with_path() {
		let dir = tempfile::tempdir().unwrap();
		let subdir = dir.path().join("frontend");
		std::fs::create_dir(&subdir).unwrap();
		let config = NpmConfig {
			enabled: true,
			path: Some("frontend".to_string()),
			lock_command: None,
			access: None,
		};
		let git_workdir = AbsolutePath::new(dir.path()).unwrap();
		let resolved = config
			.resolve_root(&git_workdir, &crate::filesystem::LocalFilesystem)
			.unwrap();
		assert_eq!(*resolved, *AbsolutePath::new(&subdir).unwrap());
	}

	#[test]
	fn npm_config_resolve_root_rejects_traversal() {
		let outer = tempfile::tempdir().unwrap();
		let repo = outer.path().join("repo");
		std::fs::create_dir(&repo).unwrap();
		let escape_dir = outer.path().join("escape");
		std::fs::create_dir(&escape_dir).unwrap();
		let config = NpmConfig {
			enabled: true,
			path: Some("../escape".to_string()),
			lock_command: None,
			access: None,
		};
		let git_workdir = AbsolutePath::new(&repo).unwrap();
		let result = config.resolve_root(&git_workdir, &crate::filesystem::LocalFilesystem);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("escapes repository root")
		);
	}
}
