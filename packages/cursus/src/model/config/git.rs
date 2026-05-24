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

/// Controls whether cursus routes commits through the GitHub Git Data API for signing.
///
/// When enabled and a GitHub token is available, `cursus prepare` creates commits
/// via the REST API rather than the local `git` binary. GitHub fills the committer
/// identity with the App's bot account and signs the commit with the web-flow GPG
/// key, producing a Verified commit with no long-lived key custody (ADR-050).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SignedCommitsMode {
	/// Enable API commits when `GITHUB_ACTIONS=true` and a token is available.
	#[default]
	Auto,
	/// Enable API commits whenever a token is available, regardless of CI environment.
	///
	/// This mode is experimental and has not been validated outside GitHub Actions.
	Force,
	/// Always use the local `git` binary; never route commits through the API.
	Off,
}

fn is_default_signed_commits_mode(m: &SignedCommitsMode) -> bool {
	matches!(m, SignedCommitsMode::Auto)
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
	pub(crate) enabled: Option<bool>,
	/// Git automation strategy.
	///
	/// `None` means the key was absent from the config file; the runtime default
	/// is derived: `Branch` when `[github].enabled = true`, otherwise `Push`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) strategy: Option<Strategy>,
	/// Prefix used to name release branches in the `branch` strategy.
	///
	/// The release branch name is `{prefix}{current_branch}`, defaulting to `cursus-release/`
	/// (e.g., if on `main`, the release branch is `cursus-release/main`).
	/// A detached HEAD has no current branch to compose against, so `prepare`
	/// rejects it upstream rather than substituting a fallback name.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) release_branch_prefix: Option<String>,
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
	pub(crate) prepare_commit_message: Option<String>,
	/// Whether to route the prepare commit through the GitHub Git Data API.
	///
	/// Defaults to [`SignedCommitsMode::Auto`] (omitted from serialized config).
	#[serde(default, skip_serializing_if = "is_default_signed_commits_mode")]
	pub signed_commits: SignedCommitsMode,
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
	pub(crate) publish_private_packages: Vec<String>,
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
	/// called by [`Config::load`] once `[git]` and the forge sections are known.
	///
	/// - If `forge_enabled` is `true` and `enabled` was not explicitly set,
	///   enables git lifecycle automation.
	/// - If `strategy` was not explicitly set, derives it from `forge_enabled`:
	///   [`Strategy::Branch`] when a forge is enabled, [`Strategy::Push`] otherwise.
	pub(super) fn resolve_defaults(&mut self, forge_enabled: bool) {
		if forge_enabled && self.enabled.is_none() {
			self.enabled = Some(true);
		}
		if self.strategy.is_none() {
			self.strategy = Some(if forge_enabled {
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
