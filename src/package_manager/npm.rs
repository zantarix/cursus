//! npm package manager adapter.

use std::path::{Path, PathBuf};

use anyhow::Context;
use glob::glob;
use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstInputValue, CstRootNode};
use log::warn;
use semver::Version;
use serde::{Deserialize, Serialize};

use super::{PackageManagerAdapter, ProjectInfo, PublishOutcome};
use crate::path::AbsolutePath;

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
	/// Access level for scoped packages ("public" or "restricted").
	///
	/// Only used when publishing scoped packages (e.g., @scope/package).
	/// If not specified, defaults to "restricted" for scoped packages.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub access: Option<String>,
}

impl NpmConfig {
	/// Creates a new enabled npm configuration.
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
	fn resolve_root(&self, git_workdir: &AbsolutePath) -> anyhow::Result<AbsolutePath> {
		match &self.path {
			Some(path) => AbsolutePath::new(git_workdir.join(path))
				.with_context(|| format!("resolve_root: invalid path '{path}'")),
			None => Ok(git_workdir.clone()),
		}
	}
}

/// Adapter for npm-based projects.
///
/// Supports both single-package repositories and monorepos using npm/yarn/pnpm workspaces.
#[derive(Debug)]
pub struct NpmAdapter {
	/// Configuration for this package manager.
	config: NpmConfig,
	/// Package manager root path.
	adapter_root: AbsolutePath,
	/// Environment for executing npm/pnpm/yarn commands.
	env: crate::Env,
}

impl NpmAdapter {
	/// Creates a new npm adapter with the given configuration.
	pub fn new(config: NpmConfig, adapter_root: AbsolutePath, env: crate::Env) -> Self {
		Self {
			config,
			adapter_root,
			env,
		}
	}

	/// Returns the resolved root directory for this package manager.
	fn resolve_root(&self) -> anyhow::Result<AbsolutePath> {
		self.config.resolve_root(&self.adapter_root)
	}
}

/// Represents the relevant fields from package.json.
#[derive(Debug, Deserialize)]
struct PackageJson {
	name: Option<String>,
	workspaces: Option<Workspaces>,
	version: Option<String>,
	private: Option<bool>,
	dependencies: Option<std::collections::HashMap<String, serde_json::Value>>,
	#[serde(rename = "devDependencies")]
	dev_dependencies: Option<std::collections::HashMap<String, serde_json::Value>>,
	#[serde(rename = "peerDependencies")]
	peer_dependencies: Option<std::collections::HashMap<String, serde_json::Value>>,
	#[serde(rename = "optionalDependencies")]
	optional_dependencies: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// Workspaces can be either an array of globs or an object with a packages field.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Workspaces {
	/// Array of glob patterns (npm/yarn style).
	Array(Vec<String>),
	/// Object with packages field (some yarn configurations).
	Object { packages: Vec<String> },
}

impl Workspaces {
	/// Returns the workspace patterns as a slice.
	fn patterns(&self) -> &[String] {
		match self {
			Workspaces::Array(patterns) => patterns,
			Workspaces::Object { packages } => packages,
		}
	}
}

/// Represents the pnpm-workspace.yaml file structure.
#[derive(Debug, Deserialize)]
struct PnpmWorkspace {
	packages: Option<Vec<String>>,
}

/// Reads and parses the package.json file from a directory.
///
/// Returns `Ok(None)` if the file doesn't exist, `Ok(Some(package))` if parsed
/// successfully, or an error if the file exists but cannot be parsed.
fn read_package_json(dir: &Path) -> anyhow::Result<Option<PackageJson>> {
	let path = dir.join("package.json");
	if !path.exists() {
		return Ok(None);
	}
	let contents = std::fs::read_to_string(&path)
		.with_context(|| format!("Failed to read {}", path.display()))?;
	let package: PackageJson = serde_json::from_str(&contents)
		.with_context(|| format!("Failed to parse {}", path.display()))?;
	Ok(Some(package))
}

/// Reads and parses a pnpm-workspace.yaml file if it exists.
///
/// Returns `Ok(None)` if the file doesn't exist, `Ok(Some(workspace))` if parsed
/// successfully, or an error if the file exists but cannot be parsed.
fn read_pnpm_workspace(git_workdir: &Path) -> anyhow::Result<Option<PnpmWorkspace>> {
	let path = git_workdir.join("pnpm-workspace.yaml");
	if !path.exists() {
		return Ok(None);
	}
	let contents = std::fs::read_to_string(&path)
		.with_context(|| format!("Failed to read {}", path.display()))?;
	let workspace: PnpmWorkspace = serde_saphyr::from_str(&contents)
		.with_context(|| format!("Failed to parse {}", path.display()))?;
	Ok(Some(workspace))
}

/// Returns workspace patterns, preferring pnpm workspace patterns over package.json.
///
/// pnpm-workspace.yaml takes precedence over package.json workspaces.
fn get_workspace_patterns(
	pnpm_workspace: Option<&PnpmWorkspace>,
	package_json: &PackageJson,
) -> Option<Vec<String>> {
	// Check pnpm workspace first (takes precedence)
	if let Some(pnpm) = pnpm_workspace
		&& let Some(packages) = &pnpm.packages
		&& !packages.is_empty()
	{
		return Some(packages.clone());
	}

	// Fall back to package.json workspaces
	package_json
		.workspaces
		.as_ref()
		.map(|ws| ws.patterns().to_vec())
}

/// Extracts project metadata from a parsed package.json.
///
/// Returns version, publishable status, and dependency names.
fn extract_project_metadata(package: &PackageJson) -> anyhow::Result<(Version, bool, Vec<String>)> {
	// Extract version
	let version_str = package
		.version
		.as_deref()
		.context("Missing version in package.json")?;
	let version = version_str
		.parse::<Version>()
		.with_context(|| format!("Invalid semver version: {version_str}"))?;

	// Determine if publishable (private: true means not publishable)
	let publishable = !package.private.unwrap_or(false);

	// Collect dependency names from all dependency sections
	let mut dependency_names = Vec::new();
	for deps_map in [
		&package.dependencies,
		&package.dev_dependencies,
		&package.peer_dependencies,
		&package.optional_dependencies,
	]
	.into_iter()
	.flatten()
	{
		dependency_names.extend(deps_map.keys().cloned());
	}

	Ok((version, publishable, dependency_names))
}

/// Attempts to create a ProjectInfo from a workspace directory path.
///
/// Returns `Ok(None)` if the path is not a valid workspace (not a directory or no package.json).
fn read_workspace_project(workspace_path: &Path) -> anyhow::Result<Option<ProjectInfo>> {
	if !workspace_path.is_dir() {
		return Ok(None);
	}

	let Some(package) = read_package_json(workspace_path)? else {
		return Ok(None);
	};

	let name = package.name.clone().with_context(|| {
		let manifest_path = workspace_path.join("package.json");
		format!("Missing name in {}", manifest_path.display())
	})?;
	let path = AbsolutePath::new(workspace_path.to_path_buf()).with_context(|| {
		format!(
			"workspace path is not absolute: {}",
			workspace_path.display()
		)
	})?;

	let (version, publishable, dependency_names) = extract_project_metadata(&package)
		.with_context(|| {
			let manifest_path = workspace_path.join("package.json");
			format!(
				"Failed to extract metadata from {}",
				manifest_path.display()
			)
		})?;

	Ok(Some(ProjectInfo {
		name,
		path,
		version,
		publishable,
		dependency_names,
	}))
}

/// Expands a workspace glob pattern and returns all matching projects.
///
/// Globs are resolved relative to `pm_root`. Paths in the returned
/// [`ProjectInfo`] are absolute paths to each workspace directory.
fn expand_workspace_pattern(pm_root: &Path, pattern: &str) -> anyhow::Result<Vec<ProjectInfo>> {
	let full_pattern = pm_root.join(pattern);
	let pattern_str = full_pattern
		.to_str()
		.context("Invalid UTF-8 in workspace pattern")?;

	glob(pattern_str)
		.with_context(|| format!("Invalid glob pattern: {}", pattern))?
		.map(|entry| {
			let workspace_path = entry
				.with_context(|| format!("Failed to read glob entry for pattern: {}", pattern))?;
			read_workspace_project(&workspace_path)
		})
		.filter_map(Result::transpose)
		.collect()
}

impl PackageManagerAdapter for NpmAdapter {
	fn write_version(&self, project: &ProjectInfo, version: &Version) -> anyhow::Result<()> {
		let manifest_path = project.path.join("package.json");
		let contents = std::fs::read_to_string(&manifest_path)
			.with_context(|| format!("Failed to read {}", manifest_path.display()))?;
		let root = CstRootNode::parse(&contents, &ParseOptions::default())
			.with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
		let obj = root
			.object_value()
			.with_context(|| format!("Root is not an object in {}", manifest_path.display()))?;
		let prop = obj
			.get("version")
			.with_context(|| format!("Missing 'version' field in {}", manifest_path.display()))?;
		prop.set_value(CstInputValue::String(version.to_string()));
		// Ensure the file always ends with exactly one newline.
		let output = format!("{}\n", root.to_string().trim_end_matches('\n'));
		std::fs::write(&manifest_path, output)
			.with_context(|| format!("Failed to write {}", manifest_path.display()))?;
		Ok(())
	}

	fn enumerate_projects(&self) -> anyhow::Result<Vec<ProjectInfo>> {
		let pm_root = self.resolve_root()?;
		let Some(root_package) = read_package_json(&pm_root)? else {
			return Ok(Vec::new());
		};
		let pnpm_workspace = read_pnpm_workspace(&pm_root)?;

		let root_manifest_path = pm_root.join("package.json");

		let Some(workspace_patterns) =
			get_workspace_patterns(pnpm_workspace.as_ref(), &root_package)
		else {
			// Single package repository
			let name = root_package
				.name
				.clone()
				.with_context(|| format!("Missing name in {}", root_manifest_path.display()))?;
			let (version, publishable, dependency_names) = extract_project_metadata(&root_package)
				.with_context(|| {
					format!(
						"Failed to extract metadata from {}",
						root_manifest_path.display()
					)
				})?;
			return Ok(vec![ProjectInfo {
				name,
				path: pm_root.clone(),
				version,
				publishable,
				dependency_names,
			}]);
		};

		// Monorepo with workspaces - include root project first
		let root_name = root_package
			.name
			.clone()
			.with_context(|| format!("Missing name in {}", root_manifest_path.display()))?;
		let (version, publishable, dependency_names) = extract_project_metadata(&root_package)
			.with_context(|| {
				format!(
					"Failed to extract metadata from {}",
					root_manifest_path.display()
				)
			})?;
		let root_project = ProjectInfo {
			name: root_name,
			path: pm_root.clone(),
			version,
			publishable,
			dependency_names,
		};

		let mut projects: Vec<ProjectInfo> = std::iter::once(root_project)
			.chain(
				workspace_patterns
					.iter()
					.map(|pattern| expand_workspace_pattern(&pm_root, pattern))
					.collect::<anyhow::Result<Vec<_>>>()?
					.into_iter()
					.flatten(),
			)
			.collect();

		// Sort by path for consistent ordering
		projects.sort_by(|a, b| a.path.cmp(&b.path));

		Ok(projects)
	}

	fn lock_file_path(&self) -> Option<std::path::PathBuf> {
		// Custom commands write to an unknown location — report None.
		if self.config.lock_command.is_some() {
			return None;
		}
		let workspace_root = self.resolve_root().ok()?;
		for name in ["package-lock.json", "pnpm-lock.yaml", "yarn.lock"] {
			let path = workspace_root.join(name);
			if path.exists() {
				return Some(path);
			}
		}
		None
	}

	fn update_lock_file(&self) -> anyhow::Result<Option<std::path::PathBuf>> {
		let workspace_root = self.resolve_root()?;

		// If a custom lock command is configured, execute it via the shell (ADR-011).
		// We can't know which file the custom command writes, so return None.
		if let Some(ref lock_command) = self.config.lock_command {
			if lock_command.trim().is_empty() {
				anyhow::bail!("lock_command is empty");
			}

			let output = self
				.env
				.run_shell(lock_command, &workspace_root)
				.with_context(|| {
					format!(
						"Failed to execute lock command '{}' in {}",
						lock_command,
						workspace_root.display()
					)
				})?;

			if !output.status.success() {
				let stderr = String::from_utf8_lossy(&output.stderr);
				anyhow::bail!(
					"Lock command '{}' failed in {}: {}",
					lock_command,
					workspace_root.display(),
					stderr
				);
			}

			return Ok(None);
		}

		// Auto-detect lock file and run appropriate command
		if workspace_root.join("package-lock.json").exists() {
			let output = self
				.env
				.run("npm", &["install", "--package-lock-only"], &workspace_root)
				.with_context(|| {
					format!(
						"Failed to execute npm install --package-lock-only in {}",
						workspace_root.display()
					)
				})?;

			if !output.status.success() {
				let stderr = String::from_utf8_lossy(&output.stderr);
				anyhow::bail!(
					"npm install --package-lock-only failed in {}: {}",
					workspace_root.display(),
					stderr
				);
			}

			Ok(Some(workspace_root.join("package-lock.json")))
		} else if workspace_root.join("pnpm-lock.yaml").exists() {
			let output = self
				.env
				.run("pnpm", &["install", "--lockfile-only"], &workspace_root)
				.with_context(|| {
					format!(
						"Failed to execute pnpm install --lockfile-only in {}",
						workspace_root.display()
					)
				})?;

			if !output.status.success() {
				let stderr = String::from_utf8_lossy(&output.stderr);
				anyhow::bail!(
					"pnpm install --lockfile-only failed in {}: {}",
					workspace_root.display(),
					stderr
				);
			}

			Ok(Some(workspace_root.join("pnpm-lock.yaml")))
		} else if workspace_root.join("yarn.lock").exists() {
			let output = self
				.env
				.run(
					"yarn",
					&["install", "--mode", "update-lockfile"],
					&workspace_root,
				)
				.with_context(|| {
					format!(
						"Failed to execute yarn install --mode update-lockfile in {}",
						workspace_root.display()
					)
				})?;

			if !output.status.success() {
				let stderr = String::from_utf8_lossy(&output.stderr);
				anyhow::bail!(
					"yarn install --mode update-lockfile failed in {}: {}",
					workspace_root.display(),
					stderr
				);
			}

			Ok(Some(workspace_root.join("yarn.lock")))
		} else {
			// No lock file found - no-op
			Ok(None)
		}
	}

	fn publish(&self, project: &ProjectInfo) -> anyhow::Result<PublishOutcome> {
		let project_dir = project.path.clone();

		let mut args = vec!["publish"];

		// For scoped packages, add --access flag
		let access_owned;
		if project.name.starts_with('@') {
			access_owned = self
				.config
				.access
				.clone()
				.unwrap_or_else(|| "restricted".to_string());
			args.push("--access");
			args.push(&access_owned);
		}

		let output = self
			.env
			.run("npm", &args, &project_dir)
			.with_context(|| format!("Failed to execute npm publish for {}", project.name))?;

		if output.status.success() {
			return Ok(PublishOutcome::Published);
		}

		// Check if the failure is because the version already exists
		let stderr = String::from_utf8_lossy(&output.stderr);
		if stderr.contains("EPUBLISHCONFLICT")
			|| stderr.contains("cannot publish over the previously published")
		{
			return Ok(PublishOutcome::AlreadyPublished);
		}

		// Some other error
		anyhow::bail!("npm publish failed for {}: {}", project.name, stderr);
	}

	fn registry_name(&self) -> &str {
		"npm"
	}

	fn manifest_filename(&self) -> &str {
		"package.json"
	}

	fn update_dependency_version(
		&self,
		project: &ProjectInfo,
		dependency_name: &str,
		new_version: &Version,
	) -> anyhow::Result<Vec<PathBuf>> {
		let manifest_path = project.path.join("package.json");
		if !manifest_path.exists() {
			return Ok(Vec::new());
		}

		let contents = std::fs::read_to_string(&manifest_path)
			.with_context(|| format!("Failed to read {}", manifest_path.display()))?;
		let root = CstRootNode::parse(&contents, &ParseOptions::default())
			.with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
		let obj = root
			.object_value()
			.with_context(|| format!("Root is not an object in {}", manifest_path.display()))?;

		let sections = [
			"dependencies",
			"devDependencies",
			"peerDependencies",
			"optionalDependencies",
		];
		let mut modified = false;

		for section in &sections {
			let Some(section_obj) = obj
				.get(section)
				.and_then(|p| p.value())
				.and_then(|v| v.as_object())
			else {
				continue;
			};
			let Some(dep_prop) = section_obj.get(dependency_name) else {
				continue;
			};
			let Some(current_value) = dep_prop
				.value()
				.and_then(|v| v.as_string_lit())
				.and_then(|s| s.decoded_value().ok())
			else {
				warn!(
					"non-string value for dependency '{}' in {}, skipping",
					dependency_name,
					manifest_path.display()
				);
				continue;
			};

			if current_value.starts_with("workspace:") {
				warn!(
					"skipping workspace: protocol dependency '{}' in {}",
					dependency_name,
					manifest_path.display()
				);
				continue;
			}

			let prefix = super::semver_range_prefix(&current_value).to_string();
			let new_dep_value = format!("{prefix}{new_version}");
			dep_prop.set_value(CstInputValue::String(new_dep_value));
			modified = true;
		}

		if modified {
			let output = format!("{}\n", root.to_string().trim_end_matches('\n'));
			std::fs::write(&manifest_path, output)
				.with_context(|| format!("Failed to write {}", manifest_path.display()))?;
			return Ok(vec![manifest_path]);
		}

		Ok(Vec::new())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;

	fn write_package_json(dir: &Path, content: &str) {
		std::fs::write(dir.join("package.json"), content).unwrap();
	}

	/// Creates a `NpmAdapter` backed by a recording runner with the given exit code.
	fn recording_adapter_default(config: NpmConfig, dir: &Path, exit_code: i32) -> NpmAdapter {
		let env = crate::Env::new(
			Arc::new(RecordingCommandRunner::new(exit_code)) as Arc<dyn CommandRunner>
		);
		NpmAdapter::new(config, crate::path::AbsolutePath::new(dir).unwrap(), env)
	}

	/// Creates a `NpmAdapter` backed by a recording runner for inspection.
	fn recording_adapter(
		config: NpmConfig,
		dir: &Path,
		runner: Arc<RecordingCommandRunner>,
	) -> NpmAdapter {
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		NpmAdapter::new(config, crate::path::AbsolutePath::new(dir).unwrap(), env)
	}

	/// Helper to enumerate projects using the adapter with no configured path.
	fn enumerate(dir: &Path) -> anyhow::Result<Vec<ProjectInfo>> {
		recording_adapter_default(NpmConfig::default(), dir, 0).enumerate_projects()
	}

	/// Helper to enumerate projects using the adapter with a configured path.
	fn enumerate_with_path(dir: &Path, path: &str) -> anyhow::Result<Vec<ProjectInfo>> {
		recording_adapter_default(
			NpmConfig {
				enabled: true,
				path: Some(path.to_string()),
				..Default::default()
			},
			dir,
			0,
		)
		.enumerate_projects()
	}

	#[test]
	fn enumerate_returns_empty_when_no_package_json() {
		let dir = temp_dir();
		let projects = enumerate(dir.path()).unwrap();
		assert!(projects.is_empty());
	}

	#[test]
	fn enumerate_single_package() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "my-app");
		assert_eq!(projects[0].path.as_path(), dir.path());
	}

	#[test]
	fn enumerate_single_package_without_name_fails() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"version": "0.1.0"}"#);

		let result = enumerate(dir.path());
		assert!(result.is_err());
		let err_msg = result.unwrap_err().to_string();
		assert!(err_msg.contains("Missing name in"));
		assert!(err_msg.contains("package.json"));
	}

	#[test]
	fn enumerate_workspaces_array() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "monorepo", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
		);

		// Create workspace packages
		let pkg_a = dir.path().join("packages/pkg-a");
		let pkg_b = dir.path().join("packages/pkg-b");
		std::fs::create_dir_all(&pkg_a).unwrap();
		std::fs::create_dir_all(&pkg_b).unwrap();
		write_package_json(&pkg_a, r#"{"name": "@scope/pkg-a", "version": "0.1.0"}"#);
		write_package_json(&pkg_b, r#"{"name": "@scope/pkg-b", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 3);
		// Root project first (shorter absolute path sorts first)
		assert_eq!(projects[0].name, "monorepo");
		assert_eq!(projects[0].path.as_path(), dir.path());
		assert_eq!(projects[1].name, "@scope/pkg-a");
		assert_eq!(
			projects[1].path.as_path(),
			dir.path().join("packages/pkg-a")
		);
		assert_eq!(projects[2].name, "@scope/pkg-b");
		assert_eq!(
			projects[2].path.as_path(),
			dir.path().join("packages/pkg-b")
		);
	}

	#[test]
	fn enumerate_workspaces_object() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "root", "version": "0.1.0", "workspaces": {"packages": ["packages/*"]}}"#,
		);

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "root");
		assert_eq!(projects[0].path.as_path(), dir.path());
		assert_eq!(projects[1].name, "my-pkg");
	}

	#[test]
	fn enumerate_workspace_without_root_name_fails() {
		let dir = temp_dir();
		// Root package without a name field
		write_package_json(
			dir.path(),
			r#"{"version": "0.1.0", "workspaces": ["packages/*"]}"#,
		);

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

		let result = enumerate(dir.path());
		assert!(result.is_err());
		let err_msg = result.unwrap_err().to_string();
		assert!(err_msg.contains("Missing name in"));
		assert!(err_msg.contains("package.json"));
	}

	#[test]
	fn enumerate_multiple_workspace_patterns() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "monorepo", "version": "0.1.0", "workspaces": ["packages/*", "apps/*"]}"#,
		);

		let pkg = dir.path().join("packages/lib");
		let app = dir.path().join("apps/web");
		std::fs::create_dir_all(&pkg).unwrap();
		std::fs::create_dir_all(&app).unwrap();
		write_package_json(&pkg, r#"{"name": "lib", "version": "0.1.0"}"#);
		write_package_json(&app, r#"{"name": "web", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 3);
		// Root first (shorter absolute path), then sorted by path
		assert_eq!(projects[0].name, "monorepo");
		assert_eq!(projects[0].path.as_path(), dir.path());
		assert_eq!(projects[1].name, "web");
		assert_eq!(projects[1].path.as_path(), dir.path().join("apps/web"));
		assert_eq!(projects[2].name, "lib");
		assert_eq!(projects[2].path.as_path(), dir.path().join("packages/lib"));
	}

	#[test]
	fn enumerate_skips_directories_without_package_json() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
		);

		let pkg = dir.path().join("packages/valid");
		let no_pkg = dir.path().join("packages/no-package-json");
		std::fs::create_dir_all(&pkg).unwrap();
		std::fs::create_dir_all(&no_pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "valid", "version": "0.1.0"}"#);
		// no_pkg has no package.json

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "root");
		assert_eq!(projects[1].name, "valid");
	}

	#[test]
	fn enumerate_skips_files_matching_glob() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
		);

		std::fs::create_dir_all(dir.path().join("packages")).unwrap();
		// Create a file instead of directory
		std::fs::write(dir.path().join("packages/not-a-dir"), "").unwrap();

		let projects = enumerate(dir.path()).unwrap();

		// Only root project, no workspace packages
		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "root");
	}

	#[test]
	fn enumerate_handles_nested_workspaces() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*/subpackages/*"]}"#,
		);

		let nested = dir.path().join("packages/group/subpackages/nested-pkg");
		std::fs::create_dir_all(&nested).unwrap();
		write_package_json(&nested, r#"{"name": "nested-pkg", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "root");
		assert_eq!(projects[1].name, "nested-pkg");
		assert_eq!(
			projects[1].path.as_path(),
			dir.path().join("packages/group/subpackages/nested-pkg")
		);
	}

	#[test]
	fn enumerate_fails_on_invalid_package_json() {
		let dir = temp_dir();
		write_package_json(dir.path(), "not valid json");

		let result = enumerate(dir.path());

		assert!(result.is_err());
	}

	#[test]
	fn enumerate_fails_on_invalid_workspace_package_json() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"version": "0.1.0", "workspaces": ["packages/*"]}"#,
		);

		let pkg = dir.path().join("packages/bad");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, "invalid json");

		let result = enumerate(dir.path());

		assert!(result.is_err());
	}

	#[test]
	fn new_creates_adapter() {
		let dir = temp_dir();
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		// Should work without panicking
		let _ = adapter.enumerate_projects();
	}

	#[test]
	fn workspaces_patterns_array() {
		let ws = Workspaces::Array(vec!["a/*".to_string(), "b/*".to_string()]);
		assert_eq!(ws.patterns(), &["a/*", "b/*"]);
	}

	#[test]
	fn workspaces_patterns_object() {
		let ws = Workspaces::Object {
			packages: vec!["pkg/*".to_string()],
		};
		assert_eq!(ws.patterns(), &["pkg/*"]);
	}

	fn write_pnpm_workspace(dir: &Path, content: &str) {
		std::fs::write(dir.join("pnpm-workspace.yaml"), content).unwrap();
	}

	#[test]
	fn enumerate_pnpm_workspace() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "pnpm-monorepo", "version": "0.1.0"}"#,
		);
		write_pnpm_workspace(dir.path(), "packages:\n  - 'packages/*'\n");

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "pnpm-monorepo");
		assert_eq!(projects[0].path.as_path(), dir.path());
		assert_eq!(projects[1].name, "my-pkg");
		assert_eq!(
			projects[1].path.as_path(),
			dir.path().join("packages/my-pkg")
		);
	}

	#[test]
	fn enumerate_pnpm_workspace_multiple_patterns() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "root", "version": "0.1.0"}"#);
		write_pnpm_workspace(dir.path(), "packages:\n  - 'packages/*'\n  - 'apps/*'\n");

		let pkg = dir.path().join("packages/lib");
		let app = dir.path().join("apps/web");
		std::fs::create_dir_all(&pkg).unwrap();
		std::fs::create_dir_all(&app).unwrap();
		write_package_json(&pkg, r#"{"name": "lib", "version": "0.1.0"}"#);
		write_package_json(&app, r#"{"name": "web", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 3);
		assert_eq!(projects[0].name, "root");
		assert_eq!(projects[1].name, "web");
		assert_eq!(projects[2].name, "lib");
	}

	#[test]
	fn enumerate_pnpm_workspace_takes_precedence_over_package_json() {
		let dir = temp_dir();
		// package.json has workspaces pointing to different location
		write_package_json(
			dir.path(),
			r#"{"name": "root", "version": "0.1.0", "workspaces": ["other/*"]}"#,
		);
		// pnpm-workspace.yaml should take precedence
		write_pnpm_workspace(dir.path(), "packages:\n  - 'packages/*'\n");

		let pkg = dir.path().join("packages/from-pnpm");
		let other = dir.path().join("other/from-npm");
		std::fs::create_dir_all(&pkg).unwrap();
		std::fs::create_dir_all(&other).unwrap();
		write_package_json(&pkg, r#"{"name": "from-pnpm", "version": "0.1.0"}"#);
		write_package_json(&other, r#"{"name": "from-npm", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "root");
		// Should find from-pnpm (pnpm-workspace.yaml), not from-npm (package.json workspaces)
		assert_eq!(projects[1].name, "from-pnpm");
	}

	#[test]
	fn enumerate_pnpm_workspace_empty_packages_falls_back_to_package_json() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
		);
		// pnpm-workspace.yaml with empty packages
		write_pnpm_workspace(dir.path(), "packages: []\n");

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "root");
		assert_eq!(projects[1].name, "my-pkg");
	}

	#[test]
	fn enumerate_pnpm_workspace_invalid_yaml_returns_error() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
		);
		// Invalid YAML
		write_pnpm_workspace(dir.path(), "not: valid: yaml: [[");

		let result = enumerate(dir.path());

		assert!(result.is_err());
	}

	#[test]
	fn enumerate_pnpm_workspace_without_packages_field() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
		);
		// pnpm-workspace.yaml without packages field
		write_pnpm_workspace(dir.path(), "other_field: true\n");

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg", "version": "0.1.0"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "root");
		assert_eq!(projects[1].name, "my-pkg");
	}

	#[test]
	fn enumerate_single_package_in_subfolder() {
		let dir = temp_dir();
		let subfolder = dir.path().join("frontend");
		std::fs::create_dir_all(&subfolder).unwrap();
		write_package_json(&subfolder, r#"{"name": "my-app", "version": "0.1.0"}"#);

		let projects = enumerate_with_path(dir.path(), "frontend").unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "my-app");
		assert_eq!(projects[0].path.as_path(), dir.path().join("frontend"));
	}

	#[test]
	fn enumerate_workspace_in_subfolder() {
		let dir = temp_dir();
		let subfolder = dir.path().join("frontend");
		std::fs::create_dir_all(&subfolder).unwrap();
		write_package_json(
			&subfolder,
			r#"{"name": "monorepo", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
		);

		let pkg_a = subfolder.join("packages/pkg-a");
		let pkg_b = subfolder.join("packages/pkg-b");
		std::fs::create_dir_all(&pkg_a).unwrap();
		std::fs::create_dir_all(&pkg_b).unwrap();
		write_package_json(&pkg_a, r#"{"name": "@scope/pkg-a", "version": "0.1.0"}"#);
		write_package_json(&pkg_b, r#"{"name": "@scope/pkg-b", "version": "0.1.0"}"#);

		let projects = enumerate_with_path(dir.path(), "frontend").unwrap();

		assert_eq!(projects.len(), 3);
		assert_eq!(projects[0].name, "monorepo");
		assert_eq!(projects[0].path.as_path(), dir.path().join("frontend"));
		assert_eq!(projects[1].name, "@scope/pkg-a");
		assert_eq!(
			projects[1].path.as_path(),
			dir.path().join("frontend/packages/pkg-a")
		);
		assert_eq!(projects[2].name, "@scope/pkg-b");
		assert_eq!(
			projects[2].path.as_path(),
			dir.path().join("frontend/packages/pkg-b")
		);
	}

	#[test]
	fn enumerate_returns_empty_when_subfolder_missing() {
		let dir = temp_dir();
		let projects = enumerate_with_path(dir.path(), "nonexistent").unwrap();
		assert!(projects.is_empty());
	}

	fn project_info(dir: &Path, name: &str, path: &str) -> ProjectInfo {
		ProjectInfo::for_test(name, AbsolutePath::new(dir.join(path)).unwrap())
	}

	#[test]
	fn enumerate_includes_version() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.2.3"}"#);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].version.to_string(), "1.2.3");
	}

	#[test]
	fn enumerate_missing_version_fails() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app"}"#);
		let result = enumerate(dir.path());
		assert!(result.is_err());
	}

	#[test]
	fn enumerate_invalid_semver_fails() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "my-app", "version": "not-a-version"}"#,
		);
		let result = enumerate(dir.path());
		assert!(result.is_err());
	}

	#[test]
	fn write_version_file_not_found() {
		let dir = temp_dir();
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let info = project_info(dir.path(), "my-app", "");
		let version: semver::Version = "1.0.0".parse().unwrap();
		let result = adapter.write_version(&info, &version);
		assert!(result.is_err());
	}

	#[test]
	fn write_version_invalid_json() {
		let dir = temp_dir();
		write_package_json(dir.path(), "not valid json");
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let info = project_info(dir.path(), "my-app", "");
		let version: semver::Version = "1.0.0".parse().unwrap();
		let result = adapter.write_version(&info, &version);
		assert!(result.is_err());
	}

	#[test]
	fn write_version_updates_package_json() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let info = project_info(dir.path(), "my-app", "");
		let new_version: semver::Version = "2.0.0".parse().unwrap();
		adapter.write_version(&info, &new_version).unwrap();

		let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
		assert!(
			contents.contains("\"2.0.0\""),
			"Should contain new version, got: {contents}"
		);
		assert!(contents.ends_with('\n'), "Should end with newline");
	}

	#[test]
	fn write_version_roundtrip() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "0.1.0"}"#);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let info = project_info(dir.path(), "my-app", "");

		let new_v: semver::Version = "0.2.0".parse().unwrap();
		adapter.write_version(&info, &new_v).unwrap();

		// Re-enumerate to verify the write
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].version.to_string(), "0.2.0");
	}

	#[test]
	fn write_version_only_updates_package_version_not_dependencies() {
		let dir = temp_dir();
		// "1.0.0" appears as both the package version and as a dependency version.
		let json = "{\n  \"name\": \"my-app\",\n  \"version\": \"1.0.0\",\n  \"dependencies\": {\n    \"some-lib\": \"1.0.0\"\n  }\n}\n";
		write_package_json(dir.path(), json);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let info = project_info(dir.path(), "my-app", "");
		let new_version: semver::Version = "2.0.0".parse().unwrap();
		adapter.write_version(&info, &new_version).unwrap();

		let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
		assert!(
			contents.contains("\"version\": \"2.0.0\""),
			"Package version should be updated, got: {contents}"
		);
		assert!(
			contents.contains("\"some-lib\": \"1.0.0\""),
			"Dependency version should be unchanged, got: {contents}"
		);
	}

	#[test]
	fn write_version_preserves_tab_indent() {
		let dir = temp_dir();
		let tab_json = "{\n\t\"name\": \"my-app\",\n\t\"version\": \"1.0.0\"\n}\n";
		write_package_json(dir.path(), tab_json);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let info = project_info(dir.path(), "my-app", "");
		let new_version: semver::Version = "2.0.0".parse().unwrap();
		adapter.write_version(&info, &new_version).unwrap();

		let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
		assert!(
			contents.contains("\"2.0.0\""),
			"Should contain new version, got: {contents}"
		);
		assert!(
			contents.contains("\t\"version\""),
			"Should preserve tab indentation, got: {contents}"
		);
		assert!(
			!contents.contains("  \"version\""),
			"Should not have space indentation, got: {contents}"
		);
	}

	#[test]
	fn write_version_preserves_four_space_indent() {
		let dir = temp_dir();
		let four_space_json = "{\n    \"name\": \"my-app\",\n    \"version\": \"1.0.0\"\n}\n";
		write_package_json(dir.path(), four_space_json);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let info = project_info(dir.path(), "my-app", "");
		let new_version: semver::Version = "2.0.0".parse().unwrap();
		adapter.write_version(&info, &new_version).unwrap();

		let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
		assert!(
			contents.contains("\"2.0.0\""),
			"Should contain new version, got: {contents}"
		);
		assert!(
			contents.contains("    \"version\""),
			"Should preserve 4-space indentation, got: {contents}"
		);
	}

	#[test]
	fn write_version_preserves_key_order() {
		let dir = temp_dir();
		// Keys are in non-alphabetical order: name, version, description.
		// Alphabetical order would be: description, name, version.
		let json = "{\n  \"name\": \"my-app\",\n  \"version\": \"1.0.0\",\n  \"description\": \"A test\"\n}\n";
		write_package_json(dir.path(), json);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let info = project_info(dir.path(), "my-app", "");
		let new_version: semver::Version = "2.0.0".parse().unwrap();
		adapter.write_version(&info, &new_version).unwrap();

		let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
		let name_pos = contents.find("\"name\"").unwrap();
		let version_pos = contents.find("\"version\"").unwrap();
		let desc_pos = contents.find("\"description\"").unwrap();
		assert!(
			name_pos < version_pos && version_pos < desc_pos,
			"Key order not preserved: {contents}"
		);
		assert!(contents.contains("\"2.0.0\""));
	}

	#[test]
	fn update_lock_file_no_op_when_no_lock_file() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);

		// Should succeed and return None when there is no lock file
		assert_eq!(adapter.update_lock_file().unwrap(), None);
	}

	#[test]
	fn update_lock_file_custom_command_empty_fails() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let adapter = recording_adapter_default(
			NpmConfig {
				enabled: true,
				path: None,
				lock_command: Some("".to_string()),
				access: None,
			},
			dir.path(),
			0,
		);

		let result = adapter.update_lock_file();
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("lock_command is empty")
		);
	}

	#[test]
	fn update_lock_file_custom_command_nonexistent_fails() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let runner =
			Arc::new(RecordingCommandRunner::new(1).with_stderr(b"command not found".to_vec()));
		let adapter = recording_adapter(
			NpmConfig {
				enabled: true,
				path: None,
				lock_command: Some("nonexistent-command-12345".to_string()),
				access: None,
			},
			dir.path(),
			runner,
		);

		let result = adapter.update_lock_file();
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("Lock command"));
	}

	#[test]
	fn npm_config_defaults_to_disabled() {
		let config = NpmConfig::default();
		assert!(!config.enabled);
		assert_eq!(config.path, None);
		assert_eq!(config.lock_command, None);
		assert_eq!(config.access, None);
	}

	#[test]
	fn npm_config_enabled_creates_enabled_config() {
		let config = NpmConfig::enabled();
		assert!(config.enabled);
		assert_eq!(config.path, None);
		assert_eq!(config.lock_command, None);
		assert_eq!(config.access, None);
	}

	#[test]
	fn npm_config_resolve_root_without_path() {
		let config = NpmConfig {
			enabled: true,
			path: None,
			lock_command: None,
			access: None,
		};
		let git_workdir = AbsolutePath::new("/repo").unwrap();
		let resolved = config.resolve_root(&git_workdir).unwrap();
		assert_eq!(resolved, git_workdir);
	}

	#[test]
	fn npm_config_resolve_root_with_path() {
		let config = NpmConfig {
			enabled: true,
			path: Some("frontend".to_string()),
			lock_command: None,
			access: None,
		};
		let git_workdir = AbsolutePath::new("/repo").unwrap();
		let resolved = config.resolve_root(&git_workdir).unwrap();
		assert_eq!(*resolved, *AbsolutePath::new("/repo/frontend").unwrap());
	}

	#[test]
	fn update_lock_file_custom_command_with_exit_code_fails() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let runner =
			Arc::new(RecordingCommandRunner::new(1).with_stderr(b"exit status 1".to_vec()));
		let adapter = recording_adapter(
			NpmConfig {
				enabled: true,
				path: None,
				lock_command: Some("false".to_string()),
				access: None,
			},
			dir.path(),
			runner,
		);

		let result = adapter.update_lock_file();
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("Lock command"));
	}

	#[test]
	fn update_lock_file_custom_command_succeeds() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let adapter = recording_adapter_default(
			NpmConfig {
				enabled: true,
				path: None,
				lock_command: Some("true".to_string()),
				access: None,
			},
			dir.path(),
			0,
		);

		// Custom command succeeds but returns None (we don't know which file it wrote)
		assert_eq!(adapter.update_lock_file().unwrap(), None);
	}

	#[test]
	fn update_lock_file_no_lock_file_returns_none() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		// No lock file present — update_lock_file should return Ok(None)
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		assert_eq!(adapter.update_lock_file().unwrap(), None);
	}

	#[test]
	fn update_lock_file_npm_passes_correct_args() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
		std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();

		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));

		let result = adapter.update_lock_file();
		assert_eq!(result.unwrap(), Some(dir.path().join("package-lock.json")));

		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "npm");
		assert_eq!(invocations[0].args, ["install", "--package-lock-only"]);
	}

	#[test]
	fn update_lock_file_npm_failure_propagates() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
		std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();

		let runner = Arc::new(RecordingCommandRunner::new(1).with_stderr(b"npm error".to_vec()));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
		assert!(adapter.update_lock_file().is_err());
	}

	#[test]
	fn update_lock_file_pnpm_passes_correct_args() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
		std::fs::write(
			dir.path().join("pnpm-lock.yaml"),
			"lockfileVersion: '6.0'\n",
		)
		.unwrap();

		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));

		let result = adapter.update_lock_file();
		assert_eq!(result.unwrap(), Some(dir.path().join("pnpm-lock.yaml")));

		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "pnpm");
		assert_eq!(invocations[0].args, ["install", "--lockfile-only"]);
	}

	#[test]
	fn update_lock_file_pnpm_failure_propagates() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
		std::fs::write(
			dir.path().join("pnpm-lock.yaml"),
			"lockfileVersion: '6.0'\n",
		)
		.unwrap();

		let runner = Arc::new(RecordingCommandRunner::new(1).with_stderr(b"pnpm error".to_vec()));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
		assert!(adapter.update_lock_file().is_err());
	}

	#[test]
	fn update_lock_file_yarn_passes_correct_args() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
		std::fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();

		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));

		let result = adapter.update_lock_file();
		assert_eq!(result.unwrap(), Some(dir.path().join("yarn.lock")));

		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "yarn");
		assert_eq!(
			invocations[0].args,
			["install", "--mode", "update-lockfile"]
		);
	}

	#[test]
	fn update_lock_file_yarn_failure_propagates() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
		std::fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();

		let runner = Arc::new(RecordingCommandRunner::new(1).with_stderr(b"yarn error".to_vec()));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
		assert!(adapter.update_lock_file().is_err());
	}

	#[test]
	fn enumerate_includes_publishable_status() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert!(
			projects[0].publishable,
			"Package without private field should be publishable"
		);
	}

	#[test]
	fn enumerate_publishable_false_for_private_true() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "my-app", "version": "1.0.0", "private": true}"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert!(
			!projects[0].publishable,
			"Package with private: true should not be publishable"
		);
	}

	#[test]
	fn enumerate_publishable_true_for_private_false() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "my-app", "version": "1.0.0", "private": false}"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert!(
			projects[0].publishable,
			"Package with private: false should be publishable"
		);
	}

	#[test]
	fn enumerate_includes_dependency_names() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{
				"name": "my-app",
				"version": "1.0.0",
				"dependencies": {
					"react": "^18.0.0",
					"lodash": "^4.17.21"
				},
				"devDependencies": {
					"jest": "^29.0.0"
				},
				"peerDependencies": {
					"typescript": "^5.0.0"
				}
			}"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].dependency_names.len(), 4);
		assert!(projects[0].dependency_names.contains(&"react".to_string()));
		assert!(projects[0].dependency_names.contains(&"lodash".to_string()));
		assert!(projects[0].dependency_names.contains(&"jest".to_string()));
		assert!(
			projects[0]
				.dependency_names
				.contains(&"typescript".to_string())
		);
	}

	// --- publish() tests ---

	#[test]
	fn publish_success_returns_published() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
		let info = project_info(dir.path(), "my-app", "");
		assert_eq!(adapter.publish(&info).unwrap(), PublishOutcome::Published);
	}

	#[test]
	fn publish_epublishconflict_returns_already_published() {
		let dir = temp_dir();
		let runner = Arc::new(
			RecordingCommandRunner::new(1).with_stderr(b"npm error code EPUBLISHCONFLICT".to_vec()),
		);
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
		let info = project_info(dir.path(), "my-app", "");
		assert_eq!(
			adapter.publish(&info).unwrap(),
			PublishOutcome::AlreadyPublished
		);
	}

	#[test]
	fn publish_cannot_publish_over_returns_already_published() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(1).with_stderr(
			b"npm error cannot publish over the previously published version".to_vec(),
		));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
		let info = project_info(dir.path(), "my-app", "");
		assert_eq!(
			adapter.publish(&info).unwrap(),
			PublishOutcome::AlreadyPublished
		);
	}

	#[test]
	fn publish_other_failure_returns_error() {
		let dir = temp_dir();
		let runner = Arc::new(
			RecordingCommandRunner::new(1).with_stderr(b"npm error 403 Forbidden".to_vec()),
		);
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
		let info = project_info(dir.path(), "my-app", "");
		assert!(adapter.publish(&info).is_err());
	}

	#[test]
	fn publish_scoped_package_uses_restricted_access_by_default() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));
		let info = project_info(dir.path(), "@scope/my-pkg", "");
		adapter.publish(&info).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		let args = &invocations[0].args;
		assert!(
			args.contains(&"--access".to_string()),
			"Expected --access flag"
		);
		assert!(
			args.contains(&"restricted".to_string()),
			"Expected restricted access"
		);
	}

	#[test]
	fn publish_scoped_package_respects_custom_access() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter = recording_adapter(
			NpmConfig {
				enabled: true,
				path: None,
				lock_command: None,
				access: Some("public".to_string()),
			},
			dir.path(),
			Arc::clone(&runner),
		);
		let info = project_info(dir.path(), "@scope/my-pkg", "");
		adapter.publish(&info).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		let args = &invocations[0].args;
		assert!(args.contains(&"--access".to_string()));
		assert!(args.contains(&"public".to_string()));
	}

	#[test]
	fn publish_non_scoped_package_omits_access_flag() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter = recording_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));
		let info = project_info(dir.path(), "my-app", "");
		adapter.publish(&info).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		let args = &invocations[0].args;
		assert!(
			!args.contains(&"--access".to_string()),
			"Non-scoped package should not have --access flag"
		);
	}

	// --- update_lock_file shell execution tests (ADR-011) ---

	#[test]
	fn update_lock_file_custom_command_uses_shell_execution() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter = recording_adapter(
			NpmConfig {
				enabled: true,
				path: None,
				lock_command: Some("custom-lock-cmd --flag".to_string()),
				access: None,
			},
			dir.path(),
			Arc::clone(&runner),
		);
		adapter.update_lock_file().unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(
			invocations[0].is_shell,
			"Custom lock_command should use shell execution"
		);
		assert_eq!(invocations[0].args[1], "custom-lock-cmd --flag");
	}

	#[test]
	fn update_lock_file_custom_command_failure_propagates() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let runner =
			Arc::new(RecordingCommandRunner::new(1).with_stderr(b"command not found".to_vec()));
		let adapter = recording_adapter(
			NpmConfig {
				enabled: true,
				path: None,
				lock_command: Some("bad-cmd".to_string()),
				access: None,
			},
			dir.path(),
			runner,
		);
		let result = adapter.update_lock_file();
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("Lock command"));
	}

	// lock_file_path tests

	#[test]
	fn lock_file_path_returns_none_when_lock_command_set() {
		let dir = temp_dir();
		let adapter = recording_adapter_default(
			NpmConfig {
				enabled: true,
				path: None,
				lock_command: Some("my-lock-cmd".to_string()),
				access: None,
			},
			dir.path(),
			0,
		);
		assert_eq!(adapter.lock_file_path(), None);
	}

	#[test]
	fn lock_file_path_returns_none_when_no_lock_file_exists() {
		let dir = temp_dir();
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		assert_eq!(adapter.lock_file_path(), None);
	}

	#[test]
	fn lock_file_path_returns_package_lock_json_when_present() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		assert_eq!(
			adapter.lock_file_path(),
			Some(dir.path().join("package-lock.json"))
		);
	}

	#[test]
	fn lock_file_path_returns_pnpm_lock_yaml_when_present() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		assert_eq!(
			adapter.lock_file_path(),
			Some(dir.path().join("pnpm-lock.yaml"))
		);
	}

	#[test]
	fn lock_file_path_returns_yarn_lock_when_present() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("yarn.lock"), "").unwrap();
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		assert_eq!(adapter.lock_file_path(), Some(dir.path().join("yarn.lock")));
	}

	#[test]
	fn registry_name_is_npm() {
		let dir = temp_dir();
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		assert_eq!(adapter.registry_name(), "npm");
	}

	#[test]
	fn manifest_filename_is_package_json() {
		let dir = temp_dir();
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		assert_eq!(adapter.manifest_filename(), "package.json");
	}

	// --- update_dependency_version tests ---

	fn make_project_with_deps(dir: &Path, deps_json: &str) -> ProjectInfo {
		let content =
			format!(r#"{{"name": "pkg-a", "version": "0.1.0", "dependencies": {deps_json}}}"#);
		write_package_json(dir, &content);
		project_info(dir, "pkg-a", "")
	}

	#[test]
	fn update_dep_version_missing_manifest_returns_empty() {
		let dir = temp_dir();
		// No package.json written
		let info = project_info(dir.path(), "pkg-a", "");
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "2.0.0".parse().unwrap();
		let modified = adapter
			.update_dependency_version(&info, "pkg-b", &new_version)
			.unwrap();
		assert!(modified.is_empty());
	}

	#[test]
	fn update_dep_version_invalid_json_returns_error() {
		let dir = temp_dir();
		write_package_json(dir.path(), "not valid json {{{{");
		let info = project_info(dir.path(), "pkg-a", "");
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "2.0.0".parse().unwrap();
		let result = adapter.update_dependency_version(&info, "pkg-b", &new_version);
		assert!(result.is_err());
	}

	#[test]
	fn update_dep_version_preserves_caret() {
		let dir = temp_dir();
		let info = make_project_with_deps(dir.path(), r#"{"pkg-b": "^1.0.0"}"#);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "2.0.0".parse().unwrap();

		let modified = adapter
			.update_dependency_version(&info, "pkg-b", &new_version)
			.unwrap();

		assert_eq!(modified.len(), 1);
		let content = std::fs::read_to_string(&modified[0]).unwrap();
		assert!(content.contains("\"pkg-b\": \"^2.0.0\""), "got: {content}");
	}

	#[test]
	fn update_dep_version_preserves_tilde() {
		let dir = temp_dir();
		let info = make_project_with_deps(dir.path(), r#"{"pkg-b": "~1.2.0"}"#);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "1.3.0".parse().unwrap();

		let modified = adapter
			.update_dependency_version(&info, "pkg-b", &new_version)
			.unwrap();

		assert_eq!(modified.len(), 1);
		let content = std::fs::read_to_string(&modified[0]).unwrap();
		assert!(content.contains("\"pkg-b\": \"~1.3.0\""), "got: {content}");
	}

	#[test]
	fn update_dep_version_exact_no_prefix() {
		let dir = temp_dir();
		let info = make_project_with_deps(dir.path(), r#"{"pkg-b": "1.0.0"}"#);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "2.0.0".parse().unwrap();

		let modified = adapter
			.update_dependency_version(&info, "pkg-b", &new_version)
			.unwrap();

		assert_eq!(modified.len(), 1);
		let content = std::fs::read_to_string(&modified[0]).unwrap();
		assert!(content.contains("\"pkg-b\": \"2.0.0\""), "got: {content}");
	}

	#[test]
	fn update_dep_version_workspace_protocol_prints_warning() {
		let dir = temp_dir();
		let info = make_project_with_deps(dir.path(), r#"{"pkg-b": "workspace:*"}"#);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "2.0.0".parse().unwrap();

		// Should not error, should return empty (skipped)
		let modified = adapter
			.update_dependency_version(&info, "pkg-b", &new_version)
			.unwrap();

		assert!(modified.is_empty(), "workspace: deps should be skipped");
	}

	#[test]
	fn update_dep_version_not_found_returns_empty() {
		let dir = temp_dir();
		let info = make_project_with_deps(dir.path(), r#"{"other-dep": "1.0.0"}"#);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "2.0.0".parse().unwrap();

		let modified = adapter
			.update_dependency_version(&info, "nonexistent", &new_version)
			.unwrap();

		assert!(modified.is_empty());
	}

	#[test]
	fn update_dep_version_in_dev_dependencies() {
		let dir = temp_dir();
		let content =
			r#"{"name": "pkg-a", "version": "0.1.0", "devDependencies": {"pkg-b": "^1.0.0"}}"#;
		write_package_json(dir.path(), content);
		let info = project_info(dir.path(), "pkg-a", "");
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "2.0.0".parse().unwrap();

		let modified = adapter
			.update_dependency_version(&info, "pkg-b", &new_version)
			.unwrap();

		assert_eq!(modified.len(), 1);
		let updated = std::fs::read_to_string(&modified[0]).unwrap();
		assert!(updated.contains("\"pkg-b\": \"^2.0.0\""), "got: {updated}");
	}

	#[test]
	fn semver_range_prefix_caret() {
		assert_eq!(super::super::semver_range_prefix("^1.0.0"), "^");
	}

	#[test]
	fn semver_range_prefix_tilde() {
		assert_eq!(super::super::semver_range_prefix("~1.2.0"), "~");
	}

	#[test]
	fn semver_range_prefix_empty() {
		assert_eq!(super::super::semver_range_prefix("1.0.0"), "");
	}

	#[test]
	fn update_dep_version_in_peer_dependencies() {
		let dir = temp_dir();
		let content =
			r#"{"name": "pkg-a", "version": "0.1.0", "peerDependencies": {"pkg-b": "^1.0.0"}}"#;
		write_package_json(dir.path(), content);
		let info = project_info(dir.path(), "pkg-a", "");
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "2.0.0".parse().unwrap();

		let modified = adapter
			.update_dependency_version(&info, "pkg-b", &new_version)
			.unwrap();

		assert_eq!(modified.len(), 1);
		let updated = std::fs::read_to_string(&modified[0]).unwrap();
		assert!(updated.contains("\"pkg-b\": \"^2.0.0\""), "got: {updated}");
	}

	#[test]
	fn update_dep_version_in_optional_dependencies() {
		let dir = temp_dir();
		let content =
			r#"{"name": "pkg-a", "version": "0.1.0", "optionalDependencies": {"pkg-b": "~1.0.0"}}"#;
		write_package_json(dir.path(), content);
		let info = project_info(dir.path(), "pkg-a", "");
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		let new_version: Version = "2.0.0".parse().unwrap();

		let modified = adapter
			.update_dependency_version(&info, "pkg-b", &new_version)
			.unwrap();

		assert_eq!(modified.len(), 1);
		let updated = std::fs::read_to_string(&modified[0]).unwrap();
		assert!(updated.contains("\"pkg-b\": \"~2.0.0\""), "got: {updated}");
	}

	#[test]
	fn update_dep_version_in_root_workspace_project() {
		// The root package.json in an npm workspace can depend on workspace members.
		// project.path is the absolute dir path for the root, so update_dependency_version
		// should write to dir/package.json.
		let dir = temp_dir();
		let content = r#"{
  "name": "my-monorepo",
  "version": "1.0.0",
  "private": true,
  "workspaces": ["packages/*"],
  "dependencies": {
    "pkg-b": "^0.2.0"
  }
}"#;
		write_package_json(dir.path(), content);
		let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
		// Root project path is the absolute dir path
		let info = project_info(dir.path(), "my-monorepo", "");
		let new_version: Version = "0.3.0".parse().unwrap();

		let modified = adapter
			.update_dependency_version(&info, "pkg-b", &new_version)
			.unwrap();

		assert_eq!(modified.len(), 1);
		assert_eq!(modified[0], dir.path().join("package.json"));
		let updated = std::fs::read_to_string(&modified[0]).unwrap();
		assert!(updated.contains("\"pkg-b\": \"^0.3.0\""), "got: {updated}");
	}
}
