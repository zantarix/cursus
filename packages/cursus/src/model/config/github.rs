//! GitHub Releases configuration types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Configuration for opt-in GitHub Releases creation after publish.
///
/// When `enabled` is `true`, Cursus will create a GitHub Release for each
/// published package after the publish step completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GitHubConfig {
	/// Whether GitHub Releases creation is enabled.
	///
	/// Defaults to `false`. When set to `true`, also implies `[git].enabled = true`
	/// unless `[git].enabled` is explicitly set.
	pub enabled: bool,
	/// GitHub repository owner (user or organisation name).
	///
	/// If not set, Cursus will attempt to detect it from the git remote URL.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) owner: Option<String>,
	/// GitHub repository name.
	///
	/// If not set, Cursus will attempt to detect it from the git remote URL.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) repo: Option<String>,
	/// Optional shell command to build release artifacts before uploading.
	///
	/// Run in the git root directory. Defaults to empty (no build step).
	#[serde(skip_serializing_if = "String::is_empty")]
	pub build_command: String,
	/// Per-package artifact maps: package name → (display name → file path relative to git root).
	///
	/// Each package's entries are uploaded as assets on its GitHub Release. Packages without an
	/// entry receive no artifacts. Defaults to empty (no assets for any package).
	#[serde(skip_serializing_if = "BTreeMap::is_empty")]
	pub artifacts: BTreeMap<String, BTreeMap<String, String>>,
	/// Title to use for automatically created pull requests in the `branch` git strategy.
	///
	/// Defaults to `"Release updates"` when not set.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) pull_request_title: Option<String>,
}

impl GitHubConfig {
	/// Returns a [`GitHubConfig`] with `enabled` set to `true`.
	pub fn enabled_config() -> Self {
		Self {
			enabled: true,
			..Default::default()
		}
	}

	/// Returns the GitHub repository owner, or `None` for auto-detection.
	pub fn owner(&self) -> Option<&str> {
		self.owner.as_deref()
	}

	/// Returns the GitHub repository name, or `None` for auto-detection.
	pub fn repo(&self) -> Option<&str> {
		self.repo.as_deref()
	}

	/// Returns the pull request title, defaulting to `"Release updates"`.
	pub fn pull_request_title(&self) -> &str {
		self.pull_request_title
			.as_deref()
			.unwrap_or("Release updates")
	}

	/// Sets the repository owner (builder pattern).
	pub fn with_owner(mut self, owner: String) -> Self {
		self.owner = Some(owner);
		self
	}

	/// Sets the repository name (builder pattern).
	pub fn with_repo(mut self, repo: String) -> Self {
		self.repo = Some(repo);
		self
	}

	/// Sets the pull request title (builder pattern).
	pub fn with_pull_request_title(mut self, title: String) -> Self {
		self.pull_request_title = Some(title);
		self
	}
}
