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

/// Controls how far the git lifecycle proceeds after a release.
///
/// Each step implies all previous steps. For example, `Push` implies `Commit` + `Tag` + `Push`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GitStep {
	/// Only create a commit — do not create any tags or push.
	Commit,
	/// Create a commit and tags — do not push. This is the default.
	#[default]
	Tag,
	/// Create a commit, tags, and push to origin.
	Push,
}

impl GitStep {
	/// Returns `true` if the lifecycle should create a commit.
	///
	/// Always returns `true` since every step includes committing. This method
	/// exists for symmetry with [`should_tag`] and [`should_push`], and to make
	/// the orchestrator's intent explicit at the call site if a no-commit step
	/// is ever added in the future.
	pub fn should_commit(self) -> bool {
		true
	}

	/// Returns `true` if the lifecycle should create git tags.
	pub fn should_tag(self) -> bool {
		matches!(self, GitStep::Tag | GitStep::Push)
	}

	/// Returns `true` if the lifecycle should push to origin.
	pub fn should_push(self) -> bool {
		matches!(self, GitStep::Push)
	}
}

/// Configuration for the optional git lifecycle automation.
///
/// When `enabled` is `true`, Chronicle will automatically create a commit,
/// optionally tag, and optionally push after a successful `release`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GitConfig {
	/// Whether git lifecycle automation is enabled.
	///
	/// `None` means the key was absent from the config file; callers should
	/// treat `None` the same as `Some(false)` unless a derived default applies
	/// (e.g. `[github].enabled = true` implies `Some(true)`).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub enabled: Option<bool>,
	/// How far the git lifecycle should proceed.
	///
	/// Defaults to [`GitStep::Tag`] (commit + tag locally, no push).
	pub run_until: GitStep,
	/// Tag name format to use when creating git tags.
	///
	/// Defaults to [`TagFormat::Auto`].
	pub tag_format: TagFormat,
	/// Additional files to unconditionally stage before committing, relative to
	/// the git root. Staging an unmodified file is a no-op in git, so it is safe
	/// to list files here even when they may not have changed.
	///
	/// This is useful when a custom `lock_command` is configured and Chronicle
	/// cannot determine which lock file the command writes.
	///
	/// Defaults to an empty list.
	pub extra_files: Vec<String>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn git_config_defaults() {
		let config = GitConfig::default();
		assert_eq!(config.enabled, None);
		assert_eq!(config.run_until, GitStep::Tag);
		assert_eq!(config.tag_format, TagFormat::Auto);
	}

	#[test]
	fn git_step_commit_only_should_commit() {
		assert!(GitStep::Commit.should_commit());
		assert!(!GitStep::Commit.should_tag());
		assert!(!GitStep::Commit.should_push());
	}

	#[test]
	fn git_step_tag_includes_commit_and_tag() {
		assert!(GitStep::Tag.should_commit());
		assert!(GitStep::Tag.should_tag());
		assert!(!GitStep::Tag.should_push());
	}

	#[test]
	fn git_step_push_includes_all() {
		assert!(GitStep::Push.should_commit());
		assert!(GitStep::Push.should_tag());
		assert!(GitStep::Push.should_push());
	}

	#[test]
	fn git_config_deserializes_defaults_when_empty() {
		let config: GitConfig = toml::from_str("").unwrap();
		assert_eq!(config.enabled, None);
		assert_eq!(config.run_until, GitStep::Tag);
		assert_eq!(config.tag_format, TagFormat::Auto);
	}

	#[test]
	fn git_config_deserializes_enabled_true() {
		let config: GitConfig = toml::from_str("enabled = true").unwrap();
		assert_eq!(config.enabled, Some(true));
	}

	#[test]
	fn git_config_deserializes_enabled_false() {
		let config: GitConfig = toml::from_str("enabled = false").unwrap();
		assert_eq!(config.enabled, Some(false));
	}

	#[test]
	fn git_config_deserializes_run_until_commit() {
		let config: GitConfig = toml::from_str("run_until = \"commit\"").unwrap();
		assert_eq!(config.run_until, GitStep::Commit);
	}

	#[test]
	fn git_config_deserializes_run_until_push() {
		let config: GitConfig = toml::from_str("run_until = \"push\"").unwrap();
		assert_eq!(config.run_until, GitStep::Push);
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
	fn git_config_roundtrip() {
		let config = GitConfig {
			enabled: Some(true),
			run_until: GitStep::Push,
			tag_format: TagFormat::Prefixed,
			extra_files: vec!["custom.lock".to_string()],
		};
		let toml_str = toml::to_string(&config).unwrap();
		let deserialized: GitConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config, deserialized);
	}

	#[test]
	fn git_config_serializes_enabled_true() {
		let config = GitConfig {
			enabled: Some(true),
			..Default::default()
		};
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
}
