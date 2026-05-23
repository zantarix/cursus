//! GitLab releases configuration types.
//!
//! Mirrors the structural shape of [`super::github::GitHubConfig`] but uses
//! GitLab's native vocabulary in every key name: `group` rather than `owner`,
//! `project` rather than `repo`, `merge_request_title` rather than
//! `pull_request_title`. The two sections coexist in `.cursus/config.toml`;
//! cross-section validation between `[github]` and `[gitlab]` is the
//! responsibility of ADR-059.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Configuration for opt-in GitLab releases creation after publish.
///
/// When `enabled` is `true`, Cursus will create a release on GitLab for each
/// published package after the publish step completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GitLabConfig {
	/// Whether GitLab releases creation is enabled.
	///
	/// Defaults to `false`. When set to `true`, also implies `[git].enabled = true`
	/// unless `[git].enabled` is explicitly set.
	pub enabled: bool,
	/// GitLab namespace (user or group) that owns the project.
	///
	/// Subgroup paths are supported (e.g. `acme/subgroup`). If not set,
	/// Cursus will attempt to detect it from the git remote URL.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) group: Option<String>,
	/// GitLab project name (the final segment of the project path).
	///
	/// If not set, Cursus will attempt to detect it from the git remote URL.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) project: Option<String>,
	/// Base of the GitLab instance.
	///
	/// Empty or absent resolves to `gitlab.com`. Accepts either a fully
	/// qualified URL (e.g. `https://gitlab.example.com`) or a bare hostname
	/// (e.g. `gitlab.example.com`) — any leading `https://` / `http://`
	/// scheme and trailing `/` are stripped at the binary boundary. GitLab
	/// CI overrides this with `CI_API_V4_URL` when the binary boundary
	/// detects a CI environment.
	#[serde(skip_serializing_if = "String::is_empty")]
	pub host: String,
	/// Optional shell command to build release artifacts before uploading.
	///
	/// Run in the git root directory. Defaults to empty (no build step).
	#[serde(skip_serializing_if = "String::is_empty")]
	pub build_command: String,
	/// Per-package artifact maps: package name → (display name → file path relative to git root).
	///
	/// Each package's entries are uploaded to the GitLab project's Generic Package Registry
	/// and attached to the release as asset links. Packages without an entry receive no
	/// artifacts. Defaults to empty (no assets for any package).
	#[serde(skip_serializing_if = "BTreeMap::is_empty")]
	pub artifacts: BTreeMap<String, BTreeMap<String, String>>,
	/// Title to use for automatically created merge requests in the `branch` git strategy.
	///
	/// Defaults to `"Release updates"` when not set.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) merge_request_title: Option<String>,
}

impl GitLabConfig {
	/// Returns a [`GitLabConfig`] with `enabled` set to `true`.
	pub fn enabled_config() -> Self {
		Self {
			enabled: true,
			..Default::default()
		}
	}

	/// Returns the GitLab group/namespace, or `None` for auto-detection.
	pub fn group(&self) -> Option<&str> {
		self.group.as_deref()
	}

	/// Returns the GitLab project name, or `None` for auto-detection.
	pub fn project(&self) -> Option<&str> {
		self.project.as_deref()
	}

	/// Returns the merge request title, defaulting to `"Release updates"`.
	pub fn merge_request_title(&self) -> &str {
		self.merge_request_title
			.as_deref()
			.unwrap_or("Release updates")
	}

	/// Sets the group/namespace (builder pattern).
	pub fn with_group(mut self, group: String) -> Self {
		self.group = Some(group);
		self
	}

	/// Sets the project name (builder pattern).
	pub fn with_project(mut self, project: String) -> Self {
		self.project = Some(project);
		self
	}

	/// Sets the host (builder pattern).
	pub fn with_host(mut self, host: String) -> Self {
		self.host = host;
		self
	}

	/// Sets the merge request title (builder pattern).
	pub fn with_merge_request_title(mut self, title: String) -> Self {
		self.merge_request_title = Some(title);
		self
	}
}
