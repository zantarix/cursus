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
	pub(crate) async fn resolve_root(
		&self,
		git_workdir: &AbsolutePath,
		fs: &dyn crate::filesystem::Filesystem,
	) -> anyhow::Result<AbsolutePath> {
		super::resolve_root(&self.path, git_workdir, fs).await
	}
}
