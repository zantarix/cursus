//! Cursus configuration types and persistence.

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
fn resolve_root(
	path: &Option<String>,
	git_workdir: &AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<AbsolutePath> {
	match path {
		Some(p) => git_workdir.subpath(p, fs).with_context(|| {
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

/// Cursus configuration for a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
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
	/// Git repository root path.
	///
	/// Always `Some` when constructed via [`Config::new`] or [`load`].
	/// `None` only when deserialized directly from TOML (which skips this field).
	#[serde(skip)]
	git_workdir: Option<AbsolutePath>,
	/// Runtime environment (command runner, GitHub client, etc.).
	///
	/// Always `Some` when constructed via [`load`].
	/// `None` only when deserialized directly from TOML or constructed via [`Config::new`]
	/// without a subsequent [`Config::with_env`] call.
	#[serde(skip)]
	env: Option<crate::Env>,
}

impl Config {
	/// Creates a new config with all package managers disabled.
	pub fn new(git_workdir: &AbsolutePath) -> Self {
		Self {
			global: GlobalConfig::default(),
			npm: NpmConfig::default(),
			cargo: CargoConfig::default(),
			git: GitConfig::default(),
			github: GitHubConfig::default(),
			linked_versions: LinkedVersionsConfig::default(),
			prepare: PrepareConfig::default(),
			git_workdir: Some(git_workdir.clone()),
			env: None,
		}
	}

	/// Sets global configuration (builder pattern).
	pub fn with_global(mut self, config: GlobalConfig) -> Self {
		self.global = config;
		self
	}

	/// Sets npm configuration (builder pattern).
	pub fn with_npm(mut self, config: NpmConfig) -> Self {
		self.npm = config;
		self
	}

	/// Sets cargo configuration (builder pattern).
	pub fn with_cargo(mut self, config: CargoConfig) -> Self {
		self.cargo = config;
		self
	}

	/// Sets git lifecycle configuration (builder pattern).
	pub fn with_git(mut self, config: GitConfig) -> Self {
		self.git = config;
		self
	}

	/// Sets GitHub Releases configuration (builder pattern).
	pub fn with_github(mut self, config: GitHubConfig) -> Self {
		self.github = config;
		self
	}

	/// Sets linked package versions configuration (builder pattern).
	pub fn with_linked_versions(mut self, config: LinkedVersionsConfig) -> Self {
		self.linked_versions = config;
		self
	}

	/// Sets prepare command configuration (builder pattern).
	pub fn with_prepare(mut self, config: PrepareConfig) -> Self {
		self.prepare = config;
		self
	}

	/// Sets the runtime environment (builder pattern).
	///
	/// Required for [`save`][Self::save] to access the filesystem.
	pub fn with_env(mut self, env: crate::Env) -> Self {
		self.env = Some(env);
		self
	}

	/// Returns the runtime environment, if set.
	///
	/// Returns `None` only when `Config` was not constructed via [`load`] and
	/// [`Config::with_env`] has not been called.
	pub fn env(&self) -> Option<&crate::Env> {
		self.env.as_ref()
	}

	/// Returns the git repository root path, if set.
	///
	/// Returns `None` only when `Config` was deserialized directly from TOML without
	/// going through [`load`]. All production code paths set this via [`Config::new`]
	/// or [`load`].
	pub fn git_workdir(&self) -> Option<&AbsolutePath> {
		self.git_workdir.as_ref()
	}

	/// Returns an iterator over all enabled package managers.
	pub fn enabled_package_managers(&self) -> impl Iterator<Item = PackageManager> {
		let mut managers = Vec::new();
		if self.npm.enabled {
			managers.push(PackageManager::Npm);
		}
		if self.cargo.enabled {
			managers.push(PackageManager::Cargo);
		}
		managers.into_iter()
	}

	/// Creates package manager adapters for all enabled package managers.
	///
	/// Returns a vector of adapter instances wrapped in `Arc` for shared ownership.
	///
	/// # Errors
	///
	/// Returns an error if this `Config` was not constructed via [`load`] or
	/// [`Config::with_env`] has not been called.
	pub fn create_adapters(&self) -> anyhow::Result<Vec<Arc<dyn PackageManagerAdapter>>> {
		let workdir = self.git_workdir.as_ref().context(
			"git_workdir not set — Config must be constructed via Config::new() or config::load()",
		)?;
		let env = self
			.env
			.as_ref()
			.context("env not set — call Config::with_env() or use config::load()")?;
		Ok(self
			.enabled_package_managers()
			.map(|pm| -> Arc<dyn PackageManagerAdapter> {
				match pm {
					PackageManager::Npm => Arc::new(NpmAdapter::new(
						self.npm.clone(),
						workdir.clone(),
						env.clone(),
					)),
					PackageManager::Cargo => Arc::new(CargoAdapter::new(
						self.cargo.clone(),
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
	pub fn load_projects_for_adapters(
		&self,
		adapters: &[Arc<dyn PackageManagerAdapter>],
	) -> anyhow::Result<Vec<Project>> {
		let all_projects = package_manager::enumerate_projects(adapters.to_vec())?;

		// Compile all ignore patterns upfront so we fail fast on invalid syntax.
		let ignore_patterns = self
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
		for (matched, raw) in pattern_matched.iter().zip(self.global.ignore.iter()) {
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
	pub fn load_projects(&self) -> anyhow::Result<Vec<Project>> {
		let adapters = self.create_adapters()?;
		self.load_projects_for_adapters(&adapters)
	}

	/// Saves the configuration to `.cursus/config.toml`.
	///
	/// Creates the `.cursus` directory if it doesn't exist.
	///
	/// # Errors
	///
	/// Returns an error if the directory cannot be created or the file cannot be written.
	pub fn save(&self) -> anyhow::Result<PathBuf> {
		let workdir = self.git_workdir.as_ref().context(
			"git_workdir not set — Config must be constructed via Config::new() or config::load()",
		)?;
		let env = self
			.env
			.as_ref()
			.context("env not set — Config must be constructed via config::load()")?;
		let fs = env.fs();
		let config_path = config_path(workdir);
		let parent = workdir.child(".cursus");
		fs.create_dir_all(&parent)
			.with_context(|| format!("Failed to create directory: {}", parent.display()))?;
		let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
		fs.write(&config_path, contents.as_bytes())
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
pub fn exists(git_workdir: &AbsolutePath, fs: &dyn crate::filesystem::Filesystem) -> bool {
	fs.exists(&config_path(git_workdir))
}

fn load_impl(git_workdir: &AbsolutePath, env: &crate::Env) -> anyhow::Result<Config> {
	if !exists(git_workdir, env.fs()) {
		bail!("No configuration found. Run 'cursus init' to create one.");
	}

	let path = config_path(git_workdir);
	let contents = env
		.fs()
		.read_to_string(&path)
		.with_context(|| format!("Failed to read config file: {}", path.display()))?;
	let mut config: Config =
		toml::from_str(&contents).with_context(|| "Failed to parse config.toml")?;

	// Validate that at least one package manager is enabled
	if config.enabled_package_managers().next().is_none() {
		bail!("Configuration must have at least one package manager enabled");
	}

	// Apply cross-config derived defaults (git.enabled, git.strategy).
	config.git.resolve_defaults(config.github.enabled);

	// Set the git root and environment
	config.git_workdir = Some(git_workdir.clone());
	config.env = Some(env.clone());

	Ok(config)
}

/// Loads the Cursus configuration from the repository.
///
/// Reads and parses `.cursus/config.toml` from the given git root.
///
/// # Errors
///
/// Returns an error if the config file cannot be read or parsed.
#[cfg(feature = "test-support")]
pub fn load(git_workdir: &AbsolutePath, env: &crate::Env) -> anyhow::Result<Config> {
	load_impl(git_workdir, env)
}

/// Loads the Cursus configuration from the repository.
///
/// Reads and parses `.cursus/config.toml` from the given git root.
///
/// # Errors
///
/// Returns an error if the config file cannot be read or parsed.
#[cfg(not(feature = "test-support"))]
pub(crate) fn load(git_workdir: &AbsolutePath, env: &crate::Env) -> anyhow::Result<Config> {
	load_impl(git_workdir, env)
}

#[cfg(test)]
mod tests;
