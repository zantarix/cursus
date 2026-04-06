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
	owner: Option<String>,
	/// GitHub repository name.
	///
	/// If not set, Cursus will attempt to detect it from the git remote URL.
	#[serde(skip_serializing_if = "Option::is_none")]
	repo: Option<String>,
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
	pull_request_title: Option<String>,
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn github_config_defaults() {
		let config = GitHubConfig::default();
		assert!(!config.enabled);
		assert_eq!(config.owner(), None);
		assert_eq!(config.repo(), None);
		assert!(config.build_command.is_empty());
		assert!(config.artifacts.is_empty());
	}

	#[test]
	fn github_config_deserializes_empty() {
		let config: GitHubConfig = toml::from_str("").unwrap();
		assert!(!config.enabled);
		assert_eq!(config.owner(), None);
		assert_eq!(config.repo(), None);
	}

	#[test]
	fn github_config_deserializes_enabled_only() {
		let config: GitHubConfig = toml::from_str("enabled = true").unwrap();
		assert!(config.enabled);
		assert_eq!(config.owner(), None);
		assert_eq!(config.repo(), None);
	}

	#[test]
	fn github_config_deserializes_all_fields() {
		let toml_str = r#"
enabled = true
owner = "acme"
repo = "my-app"
build_command = "cargo build --release"
[artifacts.my-app]
"linux-amd64" = "target/release/app"
"#;
		let config: GitHubConfig = toml::from_str(toml_str).unwrap();
		assert!(config.enabled);
		assert_eq!(config.owner(), Some("acme"));
		assert_eq!(config.repo(), Some("my-app"));
		assert_eq!(config.build_command, "cargo build --release");
		assert_eq!(
			config
				.artifacts
				.get("my-app")
				.and_then(|m| m.get("linux-amd64"))
				.map(|s| s.as_str()),
			Some("target/release/app")
		);
	}

	#[test]
	fn github_config_rejects_unknown_fields() {
		let result: Result<GitHubConfig, _> = toml::from_str("unknown_field = true");
		assert!(result.is_err(), "Expected error for unknown field");
	}

	#[test]
	fn github_config_rejects_old_flat_artifacts_format() {
		// Pre-ADR-044 configs had `[artifacts]` with string values; verify they produce a clear error.
		let toml_str = r#"
[artifacts]
"linux-amd64" = "target/release/app"
"#;
		let result: Result<GitHubConfig, _> = toml::from_str(toml_str);
		assert!(
			result.is_err(),
			"Old flat artifact format should fail to parse"
		);
		let err_msg = result.unwrap_err().to_string();
		assert!(
			err_msg.contains("invalid type"),
			"Expected a type error for old format, got: {err_msg}"
		);
	}

	#[test]
	fn github_config_roundtrip() {
		let mut pkg_artifacts = BTreeMap::new();
		pkg_artifacts.insert("linux".to_string(), "target/app".to_string());
		let mut artifacts = BTreeMap::new();
		artifacts.insert("my-pkg".to_string(), pkg_artifacts);
		let config = GitHubConfig {
			enabled: true,
			build_command: "make release".to_string(),
			artifacts,
			..Default::default()
		}
		.with_owner("owner".to_string())
		.with_repo("repo".to_string());
		let toml_str = toml::to_string(&config).unwrap();
		let deserialized: GitHubConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config, deserialized);
	}

	#[test]
	fn github_config_serializes_skips_none_and_empty() {
		let config = GitHubConfig::default();
		let toml_str = toml::to_string(&config).unwrap();
		assert!(!toml_str.contains("owner"), "None owner should be omitted");
		assert!(!toml_str.contains("repo"), "None repo should be omitted");
		assert!(
			!toml_str.contains("build_command"),
			"Empty build_command should be omitted"
		);
		assert!(
			!toml_str.contains("artifacts"),
			"Empty artifacts should be omitted"
		);
	}

	#[test]
	fn github_config_serializes_some_owner() {
		let config = GitHubConfig::default().with_owner("myorg".to_string());
		let toml_str = toml::to_string(&config).unwrap();
		assert!(toml_str.contains("owner = \"myorg\""));
	}

	#[test]
	fn github_config_serializes_build_command() {
		let config = GitHubConfig {
			build_command: "make all".to_string(),
			..Default::default()
		};
		let toml_str = toml::to_string(&config).unwrap();
		assert!(toml_str.contains("build_command = \"make all\""));
	}

	#[test]
	fn github_config_pull_request_title_defaults_to_constant() {
		let config = GitHubConfig::default();
		assert_eq!(config.pull_request_title(), "Release updates");
	}

	#[test]
	fn github_config_deserializes_pull_request_title() {
		let config: GitHubConfig = toml::from_str("pull_request_title = \"Release PR\"").unwrap();
		assert_eq!(config.pull_request_title(), "Release PR");
	}

	#[test]
	fn github_config_serializes_omits_pull_request_title_when_none() {
		let config = GitHubConfig::default();
		let toml_str = toml::to_string(&config).unwrap();
		assert!(
			!toml_str.contains("pull_request_title"),
			"None pull_request_title should be omitted, got: {toml_str}"
		);
	}

	#[test]
	fn github_config_serializes_pull_request_title_when_set() {
		let config = GitHubConfig::default().with_pull_request_title("My Release".to_string());
		let toml_str = toml::to_string(&config).unwrap();
		assert!(toml_str.contains("pull_request_title = \"My Release\""));
	}
}
