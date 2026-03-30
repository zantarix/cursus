//! Cursus configuration types and persistence.

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

mod cargo;
mod git;
mod github;
mod linked_versions;
mod npm;
mod prepare;
mod template;

pub use cargo::CargoConfig;
pub use git::{GitConfig, Strategy, TagFormat};
pub use github::GitHubConfig;
pub use linked_versions::{LinkedVersionGroup, LinkedVersionsConfig};
pub use npm::{NpmAccess, NpmConfig};
pub use prepare::{DependencyBump, PrepareConfig};
pub(crate) use template::render_init_template;

use crate::package_manager::{self, CargoAdapter, NpmAdapter, PackageManagerAdapter, Project};
use crate::path::AbsolutePath;

/// Resolves an optional sub-path relative to `git_workdir`.
///
/// Used by package manager config types to locate their workspace root.
/// Returns `git_workdir` unchanged when `path` is `None`.
async fn resolve_root(
	path: &Option<String>,
	git_workdir: &AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<AbsolutePath> {
	match path {
		Some(p) => git_workdir.subpath(p, fs).await.with_context(|| {
			format!("resolve_root: path '{p}' does not exist or escapes repository root")
		}),
		None => Ok(git_workdir.clone()),
	}
}

/// Global configuration settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GlobalConfig {
	/// Disable warnings about circular dependencies in monorepos.
	pub disable_dependency_cycle_warnings: bool,
	/// Glob patterns matching package names to exclude from enumeration.
	///
	/// Any project whose name matches at least one pattern is silently dropped
	/// before the project list is returned to callers.  Standard glob syntax
	/// (e.g. `"example-*"`, `"internal-tool"`) is supported via the `glob` crate.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub ignore: Vec<String>,
}

/// Supported package managers for project configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
	/// Node Package Manager (npm).
	Npm,
	/// Rust's Cargo package manager.
	Cargo,
}

/// TOML-persisted configuration fields.
///
/// This is the on-disk representation of the Cursus configuration.
/// Runtime-only state (the execution environment) lives on [`Config`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ConfigData {
	/// Global configuration settings.
	#[serde(default)]
	pub global: GlobalConfig,
	/// Configuration for npm package manager.
	#[serde(default)]
	pub npm: NpmConfig,
	/// Configuration for Cargo package manager.
	#[serde(default)]
	pub cargo: CargoConfig,
	/// Git lifecycle automation configuration.
	#[serde(default)]
	pub git: GitConfig,
	/// GitHub Releases configuration.
	#[serde(default)]
	pub github: GitHubConfig,
	/// Linked package versions configuration.
	#[serde(default, rename = "linked-versions")]
	pub linked_versions: LinkedVersionsConfig,
	/// Prepare command configuration.
	#[serde(default)]
	pub prepare: PrepareConfig,
}

/// Cursus configuration for a repository.
///
/// Contains only TOML-persisted data — the runtime execution environment
/// ([`Env`][crate::Env]) is passed separately by callers.
///
/// Field access to the persisted data is provided via [`Deref`] to
/// [`ConfigData`], so `config.npm`, `config.cargo`, etc. work directly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Config {
	/// The TOML-persisted configuration data.
	pub(crate) data: ConfigData,
}

impl Deref for Config {
	type Target = ConfigData;

	fn deref(&self) -> &ConfigData {
		&self.data
	}
}

impl DerefMut for Config {
	fn deref_mut(&mut self) -> &mut ConfigData {
		&mut self.data
	}
}

impl Config {
	/// Creates a new config with all package managers disabled.
	pub fn new() -> Self {
		Self {
			data: ConfigData::default(),
		}
	}

	/// Sets global configuration (builder pattern).
	pub fn with_global(mut self, config: GlobalConfig) -> Self {
		self.data.global = config;
		self
	}

	/// Sets npm configuration (builder pattern).
	pub fn with_npm(mut self, config: NpmConfig) -> Self {
		self.data.npm = config;
		self
	}

	/// Sets cargo configuration (builder pattern).
	pub fn with_cargo(mut self, config: CargoConfig) -> Self {
		self.data.cargo = config;
		self
	}

	/// Sets git lifecycle configuration (builder pattern).
	pub fn with_git(mut self, config: GitConfig) -> Self {
		self.data.git = config;
		self
	}

	/// Sets GitHub Releases configuration (builder pattern).
	pub fn with_github(mut self, config: GitHubConfig) -> Self {
		self.data.github = config;
		self
	}

	/// Sets linked package versions configuration (builder pattern).
	pub fn with_linked_versions(mut self, config: LinkedVersionsConfig) -> Self {
		self.data.linked_versions = config;
		self
	}

	/// Sets prepare command configuration (builder pattern).
	pub fn with_prepare(mut self, config: PrepareConfig) -> Self {
		self.data.prepare = config;
		self
	}

	/// Returns an iterator over all enabled package managers.
	pub fn enabled_package_managers(&self) -> impl Iterator<Item = PackageManager> {
		let mut managers = Vec::new();
		if self.data.npm.enabled {
			managers.push(PackageManager::Npm);
		}
		if self.data.cargo.enabled {
			managers.push(PackageManager::Cargo);
		}
		managers.into_iter()
	}

	/// Creates package manager adapters for all enabled package managers.
	///
	/// Returns a vector of adapter instances wrapped in `Arc` for shared ownership.
	pub fn create_adapters(
		&self,
		env: &crate::Env,
	) -> anyhow::Result<Vec<Arc<dyn PackageManagerAdapter>>> {
		let workdir = env.git().path();
		Ok(self
			.enabled_package_managers()
			.map(|pm| -> Arc<dyn PackageManagerAdapter> {
				match pm {
					PackageManager::Npm => Arc::new(NpmAdapter::new(
						self.data.npm.clone(),
						workdir.clone(),
						env.clone(),
					)),
					PackageManager::Cargo => Arc::new(CargoAdapter::new(
						self.data.cargo.clone(),
						workdir.clone(),
						env.clone(),
					)),
				}
			})
			.collect())
	}

	/// Loads all projects for the given adapters.
	///
	/// Enumerates all projects from the provided adapters, then filters out any
	/// project whose name matches a pattern in `global.ignore`.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - Projects cannot be enumerated
	/// - An ignore pattern is not valid glob syntax
	/// - No projects are found after filtering
	pub async fn load_projects_for_adapters(
		&self,
		adapters: &[Arc<dyn PackageManagerAdapter>],
	) -> anyhow::Result<Vec<Project>> {
		let all_projects = package_manager::enumerate_projects(adapters.to_vec()).await?;

		// Validate that workspace-version-inherited projects are covered by linked versions.
		validate_workspace_version_linking(&all_projects, &self.data.linked_versions)?;

		// Compile all ignore patterns upfront so we fail fast on invalid syntax.
		let ignore_patterns = self
			.data
			.global
			.ignore
			.iter()
			.map(|p| {
				glob::Pattern::new(p).with_context(|| format!("Invalid ignore glob pattern: {p:?}"))
			})
			.collect::<anyhow::Result<Vec<_>>>()?;

		// Determine which patterns match at least one project (for warning purposes).
		let pattern_matched: Vec<bool> = ignore_patterns
			.iter()
			.map(|pat| all_projects.iter().any(|p| pat.matches(p.name())))
			.collect();

		// Filter out any project whose name matches an ignore pattern.
		let projects: Vec<Project> = all_projects
			.iter()
			.filter(|project| {
				!ignore_patterns
					.iter()
					.any(|pat| pat.matches(project.name()))
			})
			.cloned()
			.collect();

		// Warn about ignore patterns that matched nothing.
		for (matched, raw) in pattern_matched.iter().zip(self.data.global.ignore.iter()) {
			if !matched {
				log::warn!("Ignore pattern {raw:?} did not match any project");
			}
		}

		if projects.is_empty() {
			if all_projects.is_empty() {
				bail!(
					"No projects found. Check that your package manager configuration is correct."
				);
			} else {
				bail!(
					"All {} project(s) were excluded by [global].ignore patterns. \
					 Check that your ignore patterns are not too broad.",
					all_projects.len()
				);
			}
		}

		Ok(projects)
	}

	/// Loads all projects using the configuration.
	///
	/// Builds package manager adapters and enumerates all projects.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - Projects cannot be enumerated
	/// - No projects are found
	pub async fn load_projects(&self, env: &crate::Env) -> anyhow::Result<Vec<Project>> {
		let adapters = self.create_adapters(env)?;
		self.load_projects_for_adapters(&adapters).await
	}

	/// Saves the configuration to `.cursus/config.toml`.
	///
	/// Creates the `.cursus` directory if it doesn't exist.
	///
	/// # Errors
	///
	/// Returns an error if the directory cannot be created or the file cannot be written.
	pub async fn save(
		&self,
		fs: &dyn crate::filesystem::Filesystem,
		git_root: &AbsolutePath,
	) -> anyhow::Result<PathBuf> {
		let config_path = config_path(git_root);
		let parent = git_root.child(".cursus");
		fs.create_dir_all(&parent)
			.await
			.with_context(|| format!("Failed to create directory: {}", parent.display()))?;
		let contents = toml::to_string_pretty(&self.data).context("Failed to serialize config")?;
		fs.write(&config_path, contents.as_bytes())
			.await
			.with_context(|| format!("Failed to create config: {}", config_path.display()))?;
		Ok(config_path.into_path_buf())
	}
}

fn config_path(git_workdir: &AbsolutePath) -> AbsolutePath {
	git_workdir.child(".cursus/config.toml")
}

/// Checks if a Cursus configuration file exists in the repository.
///
/// Returns `true` if `.cursus/config.toml` exists at the given git root.
pub async fn exists(
	fs: &dyn crate::filesystem::Filesystem,
	git_root: &AbsolutePath,
) -> anyhow::Result<bool> {
	fs.exists(&config_path(git_root)).await
}

/// Loads the Cursus configuration from the repository.
///
/// Reads and parses `.cursus/config.toml` from the given git root.
/// Returns `Ok(None)` when no configuration file exists.
///
/// # Errors
///
/// Returns an error if the config file cannot be read or parsed.
pub async fn load(
	fs: &dyn crate::filesystem::Filesystem,
	git_root: &AbsolutePath,
) -> anyhow::Result<Option<Config>> {
	if !fs.exists(&config_path(git_root)).await? {
		return Ok(None);
	}

	let path = config_path(git_root);
	let contents = fs
		.read_to_string(&path)
		.await
		.with_context(|| format!("Failed to read config file: {}", path.display()))?;
	let data: ConfigData =
		toml::from_str(&contents).with_context(|| "Failed to parse config.toml")?;

	let mut config = Config { data };

	// Validate that at least one package manager is enabled
	if config.enabled_package_managers().next().is_none() {
		bail!("Configuration must have at least one package manager enabled");
	}

	// Apply cross-config derived defaults (git.enabled, git.strategy).
	config.data.git.resolve_defaults(config.data.github.enabled);

	Ok(Some(config))
}

/// Validates that projects inheriting workspace versions are properly covered
/// by linked-versions configuration.
///
/// When a project uses `version.workspace = true`, writing its version updates
/// the workspace root's `[workspace.package].version`, which affects all other
/// projects that inherit from it. This is only safe when all such projects are
/// guaranteed to receive the same version — i.e. they are in the same linked
/// group (or global linking is enabled).
///
/// # Errors
///
/// Returns an error if any projects inherit a workspace version but are not all
/// covered by the same linked-versions group.
fn validate_workspace_version_linking(
	projects: &[package_manager::Project],
	linked: &LinkedVersionsConfig,
) -> anyhow::Result<()> {
	let ws_projects: Vec<&str> = projects
		.iter()
		.filter(|p| p.workspace_version())
		.map(|p| p.name())
		.collect();

	if ws_projects.is_empty() {
		return Ok(());
	}

	// Global linking covers all projects — always safe.
	if linked.is_global() {
		return Ok(());
	}

	// Resolve groups to check if all workspace-version projects are in the same group.
	let all_names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let groups = linked.resolve_groups(&all_names)?;

	// Find which group each workspace-version project belongs to.
	let mut group_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();
	let mut unlinked: Vec<&str> = Vec::new();

	for name in &ws_projects {
		let group_idx = groups.iter().position(|g| g.iter().any(|n| n == name));
		match group_idx {
			Some(idx) => {
				group_indices.insert(idx);
			}
			None => unlinked.push(name),
		}
	}

	if !unlinked.is_empty() {
		bail!(
			"The following packages use version.workspace = true but are not in any \
			 linked-versions group: {}. All packages sharing a workspace version must \
			 be in the same linked-versions group (or use [linked-versions] enabled = true \
			 for global linking).",
			unlinked.join(", ")
		);
	}

	if group_indices.len() > 1 {
		bail!(
			"Packages using version.workspace = true are spread across {} different \
			 linked-versions groups. All packages sharing a workspace version must be \
			 in the same linked-versions group.",
			group_indices.len()
		);
	}

	Ok(())
}

#[cfg(test)]
mod tests;
