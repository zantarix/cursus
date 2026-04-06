//! Git lifecycle configuration types.

use serde::{Deserialize, Serialize};

/// Controls which tag name format is used when creating git tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TagFormat {
	/// Automatically choose format based on the number of projects in the workspace.
	///
	/// Uses `v{version}` for single-package repos and `{package}@{version}` for monorepos.
	#[default]
	Auto,
	/// Always use `{package}@{version}` format, regardless of workspace size.
	Prefixed,
	/// Always use `v{version}` format, regardless of workspace size.
	Simple,
}

impl TagFormat {
	/// Formats a git tag name for a single release.
	///
	/// The format depends on the variant and `is_multi_package`:
	/// - [`TagFormat::Auto`]: `v{version}` for single-package, `{package}@{version}` for monorepo
	/// - [`TagFormat::Prefixed`]: always `{package}@{version}`
	/// - [`TagFormat::Simple`]: always `v{version}`
	pub fn tag(
		self,
		package_name: &str,
		version: &semver::Version,
		is_multi_package: bool,
	) -> String {
		match self {
			TagFormat::Auto => {
				if is_multi_package {
					format!("{package_name}@{version}")
				} else {
					format!("v{version}")
				}
			}
			TagFormat::Prefixed => format!("{package_name}@{version}"),
			TagFormat::Simple => format!("v{version}"),
		}
	}
}

/// Controls which git strategy is used for release automation.
///
/// `Push` commits and pushes directly to the current branch.
/// `Branch` creates a new release branch, commits, and pushes it (suitable for PR workflows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
	/// Commit and push directly to the current branch.
	Push,
	/// Create a release branch, commit, push it, then return to the original branch.
	Branch,
}

/// Configuration for the optional git lifecycle automation.
///
/// When `enabled` is `true`, Cursus will automatically create a commit
/// and optionally push after a successful `release`, and create tags after `publish`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GitConfig {
	/// Whether git lifecycle automation is enabled.
	///
	/// `None` means the key was absent from the config file; callers should
	/// treat `None` the same as `Some(false)` unless a derived default applies
	/// (e.g. `[github].enabled = true` implies `Some(true)`).
	#[serde(skip_serializing_if = "Option::is_none")]
	enabled: Option<bool>,
	/// Git automation strategy.
	///
	/// `None` means the key was absent from the config file; the runtime default
	/// is derived: `Branch` when `[github].enabled = true`, otherwise `Push`.
	#[serde(skip_serializing_if = "Option::is_none")]
	strategy: Option<Strategy>,
	/// Prefix used to name release branches in the `branch` strategy.
	///
	/// The release branch name is `{prefix}{current_branch}`, defaulting to `cursus-release/`
	/// (e.g., if on `main`, the release branch is `cursus-release/main`).
	/// When the current branch cannot be determined (detached HEAD), `detached` is used.
	#[serde(skip_serializing_if = "Option::is_none")]
	release_branch_prefix: Option<String>,
	/// Tag name format to use when creating git tags.
	///
	/// Defaults to [`TagFormat::Auto`].
	pub tag_format: TagFormat,
	/// Additional files to unconditionally stage before committing, relative to
	/// the git root. Staging an unmodified file is a no-op in git, so it is safe
	/// to list files here even when they may not have changed.
	///
	/// This is useful when a custom `lock_command` is configured and Cursus
	/// cannot determine which lock file the command writes.
	///
	/// Defaults to an empty list.
	pub extra_files: Vec<String>,
	/// The commit message used for the prepare commit.
	///
	/// Defaults to `"ci(release): version packages"` when not set.
	#[serde(skip_serializing_if = "Option::is_none")]
	prepare_commit_message: Option<String>,
	/// Private package names that should receive git tags and GitHub Releases during
	/// `cursus publish`, without registry publication.
	///
	/// When a package is listed here and is marked private by its upstream manifest
	/// (`"private": true` in npm, `publish = false` in Cargo), Cursus creates a git
	/// tag and optional GitHub Release for it — but does not attempt to publish it to
	/// any registry.
	///
	/// Defaults to an empty list (all private packages are silently skipped, per ADR-007).
	#[serde(skip_serializing_if = "Vec::is_empty")]
	publish_private_packages: Vec<String>,
}

impl GitConfig {
	/// Returns whether git lifecycle automation is enabled.
	///
	/// Returns `false` when absent from config unless a derived default has been applied
	/// via [`resolve_defaults`].
	pub fn enabled(&self) -> bool {
		self.enabled.unwrap_or(false)
	}

	/// Returns the git automation strategy.
	///
	/// Returns [`Strategy::Push`] when absent from config unless a derived default
	/// has been applied via [`resolve_defaults`].
	pub fn strategy(&self) -> Strategy {
		self.strategy.unwrap_or(Strategy::Push)
	}

	/// Returns the release branch prefix.
	///
	/// Returns `"cursus-release/"` when not set in config.
	pub fn release_branch_prefix(&self) -> &str {
		self.release_branch_prefix
			.as_deref()
			.unwrap_or("cursus-release/")
	}

	/// Applies cross-config derived defaults after deserialization.
	///
	/// This must be called after loading config from disk, not during in-code
	/// construction (builder methods already express intent directly). It is
	/// called by [`Config::load`] once both `[git]` and `[github]` sections are
	/// known.
	///
	/// - If `github_enabled` is `true` and `enabled` was not explicitly set,
	///   enables git lifecycle automation.
	/// - If `strategy` was not explicitly set, derives it from `github_enabled`:
	///   [`Strategy::Branch`] when GitHub is enabled, [`Strategy::Push`] otherwise.
	pub(super) fn resolve_defaults(&mut self, github_enabled: bool) {
		if github_enabled && self.enabled.is_none() {
			self.enabled = Some(true);
		}
		if self.strategy.is_none() {
			self.strategy = Some(if github_enabled {
				Strategy::Branch
			} else {
				Strategy::Push
			});
		}
	}

	/// Returns a [`GitConfig`] with `enabled` set to `true`.
	pub fn enabled_config() -> Self {
		Self {
			enabled: Some(true),
			..Default::default()
		}
	}

	/// Sets the git automation strategy (builder pattern).
	pub fn with_strategy(mut self, strategy: Strategy) -> Self {
		self.strategy = Some(strategy);
		self
	}

	/// Sets the release branch prefix (builder pattern).
	pub fn with_release_branch_prefix(mut self, prefix: String) -> Self {
		self.release_branch_prefix = Some(prefix);
		self
	}

	/// Sets the tag format (builder pattern).
	pub fn with_tag_format(mut self, tag_format: TagFormat) -> Self {
		self.tag_format = tag_format;
		self
	}

	/// Sets the extra files list (builder pattern).
	pub fn with_extra_files(mut self, extra_files: Vec<String>) -> Self {
		self.extra_files = extra_files;
		self
	}

	/// Returns the commit message used for the prepare commit.
	///
	/// Returns `"ci(release): version packages"` when not set in config.
	pub fn prepare_commit_message(&self) -> &str {
		self.prepare_commit_message
			.as_deref()
			.unwrap_or("ci(release): version packages")
	}

	/// Sets the prepare commit message (builder pattern).
	pub fn with_prepare_commit_message(mut self, message: String) -> Self {
		self.prepare_commit_message = Some(message);
		self
	}

	/// Returns the list of private package names that should receive git tags and GitHub
	/// Releases during `cursus publish`.
	pub fn publish_private_packages(&self) -> &[String] {
		&self.publish_private_packages
	}

	/// Sets the publish_private_packages list (builder pattern).
	pub fn with_publish_private_packages(mut self, packages: Vec<String>) -> Self {
		self.publish_private_packages = packages;
		self
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn git_config_defaults() {
		let config = GitConfig::default();
		assert!(!config.enabled());
		assert_eq!(config.strategy(), Strategy::Push);
		assert_eq!(config.tag_format, TagFormat::Auto);
	}

	#[test]
	fn git_config_deserializes_defaults_when_empty() {
		let config: GitConfig = toml::from_str("").unwrap();
		assert!(!config.enabled());
		assert_eq!(config.strategy(), Strategy::Push);
		assert_eq!(config.tag_format, TagFormat::Auto);
	}

	#[test]
	fn git_config_deserializes_enabled_true() {
		let config: GitConfig = toml::from_str("enabled = true").unwrap();
		assert!(config.enabled());
	}

	#[test]
	fn git_config_deserializes_enabled_false() {
		let config: GitConfig = toml::from_str("enabled = false").unwrap();
		assert!(!config.enabled());
	}

	#[test]
	fn git_config_deserializes_strategy_push() {
		let config: GitConfig = toml::from_str("strategy = \"push\"").unwrap();
		assert_eq!(config.strategy(), Strategy::Push);
	}

	#[test]
	fn git_config_deserializes_strategy_branch() {
		let config: GitConfig = toml::from_str("strategy = \"branch\"").unwrap();
		assert_eq!(config.strategy(), Strategy::Branch);
	}

	#[test]
	fn git_config_deserializes_release_branch_prefix() {
		let config: GitConfig = toml::from_str("release_branch_prefix = \"releases/\"").unwrap();
		assert_eq!(config.release_branch_prefix(), "releases/");
	}

	#[test]
	fn git_config_deserializes_tag_format_prefixed() {
		let config: GitConfig = toml::from_str("tag_format = \"prefixed\"").unwrap();
		assert_eq!(config.tag_format, TagFormat::Prefixed);
	}

	#[test]
	fn git_config_deserializes_tag_format_simple() {
		let config: GitConfig = toml::from_str("tag_format = \"simple\"").unwrap();
		assert_eq!(config.tag_format, TagFormat::Simple);
	}

	#[test]
	fn git_config_deserializes_tag_format_auto() {
		let config: GitConfig = toml::from_str("tag_format = \"auto\"").unwrap();
		assert_eq!(config.tag_format, TagFormat::Auto);
	}

	#[test]
	fn git_config_extra_files_defaults_to_empty() {
		let config = GitConfig::default();
		assert!(config.extra_files.is_empty());
	}

	#[test]
	fn git_config_deserializes_extra_files() {
		let config: GitConfig =
			toml::from_str("extra_files = [\"custom.lock\", \"dist/manifest.json\"]").unwrap();
		assert_eq!(
			config.extra_files,
			vec!["custom.lock", "dist/manifest.json"]
		);
	}

	#[test]
	fn git_config_rejects_unknown_fields() {
		let result: Result<GitConfig, _> = toml::from_str("unknown_field = true");
		assert!(result.is_err(), "Expected error for unknown field");
	}

	#[test]
	fn git_config_rejects_old_run_until_field() {
		// run_until was removed; configs with this field must fail to parse.
		let result: Result<GitConfig, _> = toml::from_str("run_until = \"push\"");
		assert!(
			result.is_err(),
			"Expected error for removed run_until field"
		);
	}

	#[test]
	fn git_config_roundtrip() {
		let config = GitConfig::enabled_config()
			.with_strategy(Strategy::Branch)
			.with_release_branch_prefix("release/".to_string())
			.with_tag_format(TagFormat::Prefixed)
			.with_extra_files(vec!["custom.lock".to_string()])
			.with_prepare_commit_message("chore: bump versions".to_string())
			.with_publish_private_packages(vec!["my-action".to_string()]);
		let toml_str = toml::to_string(&config).unwrap();
		let deserialized: GitConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config, deserialized);
	}

	#[test]
	fn git_config_publish_private_packages_defaults_to_empty() {
		let config = GitConfig::default();
		assert!(config.publish_private_packages().is_empty());
	}

	#[test]
	fn git_config_deserializes_publish_private_packages() {
		let config: GitConfig =
			toml::from_str("publish_private_packages = [\"my-action\", \"other-action\"]").unwrap();
		assert_eq!(
			config.publish_private_packages(),
			&["my-action", "other-action"]
		);
	}

	#[test]
	fn git_config_publish_private_packages_omitted_when_empty() {
		let config = GitConfig::default();
		let toml_str = toml::to_string(&config).unwrap();
		assert!(
			!toml_str.contains("publish_private_packages"),
			"Empty list should not be serialized, got: {toml_str}"
		);
	}

	#[test]
	fn git_config_publish_private_packages_serialized_when_set() {
		let config =
			GitConfig::default().with_publish_private_packages(vec!["my-action".to_string()]);
		let toml_str = toml::to_string(&config).unwrap();
		assert!(
			toml_str.contains("publish_private_packages"),
			"Non-empty list should be serialized, got: {toml_str}"
		);
	}

	#[test]
	fn git_config_with_publish_private_packages_returns_modified_self() {
		let pkgs = vec!["my-action".to_string()];
		let config = GitConfig::default().with_publish_private_packages(pkgs.clone());
		assert_eq!(config.publish_private_packages(), pkgs.as_slice());
	}

	#[test]
	fn git_config_serializes_enabled_true() {
		let config = GitConfig::enabled_config();
		let toml_str = toml::to_string(&config).unwrap();
		assert!(toml_str.contains("enabled = true"));
	}

	#[test]
	fn git_config_serializes_omits_enabled_when_none() {
		let config = GitConfig::default();
		let toml_str = toml::to_string(&config).unwrap();
		assert!(
			!toml_str.contains("enabled"),
			"None enabled should be omitted"
		);
	}

	#[test]
	fn git_config_serializes_omits_strategy_when_none() {
		let config = GitConfig::default();
		let toml_str = toml::to_string(&config).unwrap();
		assert!(
			!toml_str.contains("strategy"),
			"None strategy should be omitted"
		);
	}

	#[test]
	fn git_config_serializes_omits_release_branch_prefix_when_none() {
		let config = GitConfig::default();
		let toml_str = toml::to_string(&config).unwrap();
		assert!(
			!toml_str.contains("release_branch_prefix"),
			"None release_branch_prefix should be omitted"
		);
	}

	#[test]
	fn git_config_enabled_config_returns_enabled() {
		let config = GitConfig::enabled_config();
		assert!(config.enabled());
	}

	#[test]
	fn git_config_resolve_defaults_sets_enabled_when_github_enabled() {
		let mut config = GitConfig::default();
		config.resolve_defaults(true);
		assert!(config.enabled());
	}

	#[test]
	fn git_config_resolve_defaults_does_not_override_explicit_enabled_false() {
		let mut config = GitConfig::default();
		config.enabled = Some(false);
		config.resolve_defaults(true);
		assert!(!config.enabled());
	}

	#[test]
	fn git_config_resolve_defaults_sets_branch_strategy_when_github_enabled() {
		let mut config = GitConfig::default();
		config.resolve_defaults(true);
		assert_eq!(config.strategy(), Strategy::Branch);
	}

	#[test]
	fn git_config_resolve_defaults_sets_push_strategy_when_github_disabled() {
		let mut config = GitConfig::default();
		config.resolve_defaults(false);
		assert_eq!(config.strategy(), Strategy::Push);
	}

	#[test]
	fn git_config_resolve_defaults_does_not_override_explicit_strategy() {
		let mut config = GitConfig::default().with_strategy(Strategy::Push);
		config.resolve_defaults(true);
		assert_eq!(config.strategy(), Strategy::Push);
	}

	#[test]
	fn git_config_release_branch_prefix_defaults_to_constant() {
		let config = GitConfig::default();
		assert_eq!(config.release_branch_prefix(), "cursus-release/");
	}

	#[test]
	fn git_config_with_release_branch_prefix_overrides_default() {
		let config = GitConfig::default().with_release_branch_prefix("release/".to_string());
		assert_eq!(config.release_branch_prefix(), "release/");
	}

	#[test]
	fn git_config_with_tag_format_returns_modified_self() {
		let config = GitConfig::default().with_tag_format(TagFormat::Prefixed);
		assert_eq!(
			config.tag_format,
			TagFormat::Prefixed,
			"with_tag_format should set tag_format on self"
		);
	}

	#[test]
	fn git_config_with_extra_files_returns_modified_self() {
		let files = vec!["custom.lock".to_string(), "dist/manifest.json".to_string()];
		let config = GitConfig::default().with_extra_files(files.clone());
		assert_eq!(
			config.extra_files, files,
			"with_extra_files should set extra_files on self"
		);
	}

	#[test]
	fn prepare_commit_message_defaults_to_constant() {
		let config = GitConfig::default();
		assert_eq!(
			config.prepare_commit_message(),
			"ci(release): version packages"
		);
	}

	#[test]
	fn prepare_commit_message_respects_config_value() {
		let config =
			GitConfig::default().with_prepare_commit_message("chore: bump versions".to_string());
		assert_eq!(config.prepare_commit_message(), "chore: bump versions");
	}

	#[test]
	fn prepare_commit_message_serializes_when_set() {
		let config =
			GitConfig::default().with_prepare_commit_message("chore: bump versions".to_string());
		let toml_str = toml::to_string(&config).unwrap();
		assert!(
			toml_str.contains("prepare_commit_message = \"chore: bump versions\""),
			"Custom message should be serialized, got: {toml_str}"
		);
	}

	#[test]
	fn prepare_commit_message_omitted_when_not_set() {
		let config = GitConfig::default();
		let toml_str = toml::to_string(&config).unwrap();
		assert!(
			!toml_str.contains("prepare_commit_message"),
			"Default message should not be serialized, got: {toml_str}"
		);
	}

	#[test]
	fn prepare_commit_message_deserializes() {
		let config: GitConfig = toml::from_str("prepare_commit_message = \"ci: release\"").unwrap();
		assert_eq!(config.prepare_commit_message(), "ci: release");
	}

	#[test]
	fn tag_format_auto_single_package() {
		let version = semver::Version::new(1, 2, 3);
		assert_eq!(TagFormat::Auto.tag("my-pkg", &version, false), "v1.2.3");
	}

	#[test]
	fn tag_format_auto_multi_package() {
		let version = semver::Version::new(1, 2, 3);
		assert_eq!(
			TagFormat::Auto.tag("my-pkg", &version, true),
			"my-pkg@1.2.3"
		);
	}

	#[test]
	fn tag_format_prefixed_single_package() {
		let version = semver::Version::new(1, 2, 3);
		assert_eq!(
			TagFormat::Prefixed.tag("my-pkg", &version, false),
			"my-pkg@1.2.3"
		);
	}

	#[test]
	fn tag_format_prefixed_multi_package() {
		let version = semver::Version::new(1, 2, 3);
		assert_eq!(
			TagFormat::Prefixed.tag("my-pkg", &version, true),
			"my-pkg@1.2.3"
		);
	}

	#[test]
	fn tag_format_simple_single_package() {
		let version = semver::Version::new(1, 2, 3);
		assert_eq!(TagFormat::Simple.tag("my-pkg", &version, false), "v1.2.3");
	}

	#[test]
	fn tag_format_simple_multi_package() {
		let version = semver::Version::new(1, 2, 3);
		assert_eq!(TagFormat::Simple.tag("my-pkg", &version, true), "v1.2.3");
	}
}
