//! npm package manager adapter.

use std::path::{Path, PathBuf};

use anyhow::Context;
use async_trait::async_trait;
use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstInputValue, CstObject, CstRootNode};
use log::warn;
use semver::Version;
use serde::Deserialize;

use super::{PackageManagerAdapter, ProjectInfo, PublishOutcome};
use crate::model::config::{NpmAccess, NpmConfig};
use crate::path::AbsolutePath;

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
	async fn resolve_root(&self) -> anyhow::Result<AbsolutePath> {
		self.config
			.resolve_root(&self.adapter_root, self.env.fs())
			.await
	}
}

/// Represents the `publishConfig` field of package.json.
#[derive(Debug, Deserialize)]
struct PublishConfig {
	provenance: Option<bool>,
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
	#[serde(rename = "publishConfig")]
	publish_config: Option<PublishConfig>,
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
async fn read_package_json(
	dir: &AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<Option<PackageJson>> {
	let path = dir.child("package.json");
	if !fs.exists(&path).await? {
		return Ok(None);
	}
	let contents = fs
		.read_to_string(&path)
		.await
		.with_context(|| format!("Failed to read {}", path.display()))?;
	let package: PackageJson = serde_json::from_str(&contents)
		.with_context(|| format!("Failed to parse {}", path.display()))?;
	Ok(Some(package))
}

/// Reads and parses a pnpm-workspace.yaml file if it exists.
///
/// Returns `Ok(None)` if the file doesn't exist, `Ok(Some(workspace))` if parsed
/// successfully, or an error if the file exists but cannot be parsed.
async fn read_pnpm_workspace(
	dir: &AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<Option<PnpmWorkspace>> {
	let path = dir.child("pnpm-workspace.yaml");
	if !fs.exists(&path).await? {
		return Ok(None);
	}
	let contents = fs
		.read_to_string(&path)
		.await
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
/// Returns version, publishable status, dependency names, and provenance setting.
fn extract_project_metadata(
	package: &PackageJson,
) -> anyhow::Result<(Version, bool, Vec<String>, Option<bool>)> {
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

	let provenance = package.publish_config.as_ref().and_then(|pc| pc.provenance);

	Ok((version, publishable, dependency_names, provenance))
}

/// Attempts to create a ProjectInfo from a workspace directory path.
///
/// Returns `Ok(None)` if the path is not a valid workspace (not a directory or no package.json).
async fn read_workspace_project(
	workspace_path: &AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<Option<ProjectInfo>> {
	if !fs.is_dir(workspace_path).await? {
		return Ok(None);
	}

	let Some(package) = read_package_json(workspace_path, fs).await? else {
		return Ok(None);
	};

	let name = package.name.clone().with_context(|| {
		let manifest_path = workspace_path.child("package.json");
		format!("Missing name in {}", manifest_path.display())
	})?;
	let path = workspace_path.clone();

	let (version, publishable, dependency_names, publishconfig_provenance) =
		extract_project_metadata(&package).with_context(|| {
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
		publishconfig_provenance,
		workspace_version: false,
	}))
}

/// Expands a workspace glob pattern and returns all matching projects.
///
/// Globs are resolved relative to `pm_root`. Paths in the returned
/// [`ProjectInfo`] are absolute paths to each workspace directory. Only paths
/// that remain within `pm_root` are returned; paths that escape via `..` or
/// symlinks are rejected with an error.
async fn expand_workspace_pattern(
	pm_root: &AbsolutePath,
	pattern: &str,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<Vec<ProjectInfo>> {
	let paths = pm_root.safe_glob(pattern, fs).await?;
	let mut projects = Vec::new();
	for workspace_path in paths {
		if let Some(info) = read_workspace_project(&workspace_path, fs).await? {
			projects.push(info);
		}
	}
	Ok(projects)
}

/// Builds a `ProjectInfo` for an npm root package.
///
/// Used for both the single-package case and the root package in a monorepo workspace.
fn build_npm_root_project_info(
	package: &PackageJson,
	path: AbsolutePath,
	manifest_path: &Path,
) -> anyhow::Result<ProjectInfo> {
	let name = package
		.name
		.clone()
		.with_context(|| format!("Missing name in {}", manifest_path.display()))?;
	let (version, publishable, dependency_names, publishconfig_provenance) =
		extract_project_metadata(package).with_context(|| {
			format!(
				"Failed to extract metadata from {}",
				manifest_path.display()
			)
		})?;
	Ok(ProjectInfo {
		name,
		path,
		version,
		publishable,
		dependency_names,
		publishconfig_provenance,
		workspace_version: false,
	})
}

/// Runs a lock-file update command for a specific package manager tool.
///
/// Returns `Ok(None)` when `lock_filename` does not exist in `workspace_root`
/// (meaning this tool is not in use). Returns `Ok(Some(path))` on success.
///
/// # Errors
///
/// Returns an error if the command fails.
async fn run_lock_update(
	env: &crate::Env,
	program: &str,
	args: &[&str],
	workspace_root: &AbsolutePath,
	lock_filename: &str,
) -> anyhow::Result<Option<PathBuf>> {
	let lock_path = workspace_root.child(lock_filename);
	if !env.fs().exists(&lock_path).await? {
		return Ok(None);
	}
	let output = env
		.run_mut(program, args, workspace_root)
		.await
		.with_context(|| {
			format!(
				"Failed to execute {} {} in {}",
				program,
				args.join(" "),
				workspace_root.display()
			)
		})?;
	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		anyhow::bail!(
			"{} {} failed in {}: {}",
			program,
			args.join(" "),
			workspace_root.display(),
			stderr
		);
	}
	Ok(Some(lock_path.into_path_buf()))
}

/// Detects the installed yarn major version by running `yarn --version`.
///
/// Returns the major version as a `u32`, or an error if the version cannot be
/// determined.
async fn yarn_major_version(
	env: &crate::Env,
	workspace_root: &AbsolutePath,
) -> anyhow::Result<u32> {
	let output = env
		.run("yarn", &["--version"], workspace_root)
		.await
		.context("Failed to run 'yarn --version'")?;
	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		anyhow::bail!("'yarn --version' failed: {stderr}");
	}
	let stdout = String::from_utf8_lossy(&output.stdout);
	let major = stdout
		.trim()
		.split('.')
		.next()
		.context("Empty output from 'yarn --version'")?
		.parse::<u32>()
		.context("Failed to parse yarn major version")?;
	Ok(major)
}

/// Updates the yarn lock file, selecting the correct flags for the installed
/// yarn version.
///
/// - Yarn Berry (v2+): `yarn install --mode update-lockfile` — lock-only, no
///   scripts.
/// - Yarn Classic (v1): `yarn install --ignore-scripts` — full install but
///   scripts are suppressed. Classic silently ignores `--mode`, so it must be
///   detected explicitly.
///
/// Returns `Ok(None)` when `yarn.lock` does not exist in `workspace_root`.
async fn run_yarn_lock_update(
	env: &crate::Env,
	workspace_root: &AbsolutePath,
) -> anyhow::Result<Option<PathBuf>> {
	let lock_path = workspace_root.child("yarn.lock");
	if !env.fs().exists(&lock_path).await? {
		return Ok(None);
	}
	let major = yarn_major_version(env, workspace_root).await?;
	let args: &[&str] = if major >= 2 {
		&["install", "--mode", "update-lockfile"]
	} else {
		&["install", "--ignore-scripts"]
	};
	let output = env
		.run_mut("yarn", args, workspace_root)
		.await
		.with_context(|| {
			format!(
				"Failed to execute yarn {} in {}",
				args.join(" "),
				workspace_root.display()
			)
		})?;
	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		anyhow::bail!(
			"yarn {} failed in {}: {}",
			args.join(" "),
			workspace_root.display(),
			stderr
		);
	}
	Ok(Some(lock_path.into_path_buf()))
}

/// Executes a custom lock file command via the shell.
///
/// Returns an error if the command fails. Used when `lock_command` is set in config.
async fn run_custom_lock_command(
	env: &crate::Env,
	lock_command: &str,
	workspace_root: &AbsolutePath,
) -> anyhow::Result<()> {
	if lock_command.trim().is_empty() {
		anyhow::bail!("lock_command is empty");
	}
	let status = env
		.run_streaming(lock_command, workspace_root)
		.await
		.with_context(|| {
			format!(
				"Failed to execute lock command '{}' in {}",
				lock_command,
				workspace_root.display()
			)
		})?;
	if !status.success() {
		anyhow::bail!(
			"Lock command '{}' failed in {} with status {status}",
			lock_command,
			workspace_root.display(),
		);
	}
	Ok(())
}

/// Attempts to update `dependency_name` in `section` of `obj` to `new_version`.
///
/// Returns `true` if the dependency was found and updated.
fn update_dep_in_section(
	obj: &CstObject,
	section: &str,
	dependency_name: &str,
	new_version: &Version,
	manifest_path: &Path,
) -> bool {
	let Some(section_obj) = obj
		.get(section)
		.and_then(|p| p.value())
		.and_then(|v| v.as_object())
	else {
		return false;
	};
	let Some(dep_prop) = section_obj.get(dependency_name) else {
		return false;
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
		return false;
	};
	if current_value.starts_with("workspace:") {
		warn!(
			"skipping workspace: protocol dependency '{}' in {}",
			dependency_name,
			manifest_path.display()
		);
		return false;
	}
	let prefix = super::semver_range_prefix(&current_value).to_string();
	dep_prop.set_value(CstInputValue::String(format!("{prefix}{new_version}")));
	true
}

#[async_trait]
impl PackageManagerAdapter for NpmAdapter {
	async fn write_version(
		&self,
		project: &ProjectInfo,
		version: &Version,
		dry_run: bool,
	) -> anyhow::Result<Vec<PathBuf>> {
		let manifest_path = project.path.child("package.json");
		let contents = self
			.env
			.fs()
			.read_to_string(&manifest_path)
			.await
			.with_context(|| format!("Failed to read {}", manifest_path.display()))?;
		// Compute the output string in a block so the non-Send CstRootNode is
		// dropped before the next `.await`.
		let output = {
			let root = CstRootNode::parse(&contents, &ParseOptions::default())
				.with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
			let obj = root
				.object_value()
				.with_context(|| format!("Root is not an object in {}", manifest_path.display()))?;
			let prop = obj.get("version").with_context(|| {
				format!("Missing 'version' field in {}", manifest_path.display())
			})?;
			prop.set_value(CstInputValue::String(version.to_string()));
			// Ensure the file always ends with exactly one newline.
			format!("{}\n", root.to_string().trim_end_matches('\n'))
		};
		if !dry_run {
			self.env
				.fs()
				.write(&manifest_path, output.as_bytes())
				.await
				.with_context(|| format!("Failed to write {}", manifest_path.display()))?;
		}
		Ok(vec![manifest_path.into_path_buf()])
	}

	async fn enumerate_projects(&self) -> anyhow::Result<Vec<ProjectInfo>> {
		let pm_root = self.resolve_root().await?;
		let fs = self.env.fs();
		let Some(root_package) = read_package_json(&pm_root, fs).await? else {
			return Ok(Vec::new());
		};
		let pnpm_workspace = read_pnpm_workspace(&pm_root, fs).await?;

		let root_manifest_path = pm_root.child("package.json");

		let Some(workspace_patterns) =
			get_workspace_patterns(pnpm_workspace.as_ref(), &root_package)
		else {
			// Single package repository
			let info =
				build_npm_root_project_info(&root_package, pm_root.clone(), &root_manifest_path)?;
			return Ok(vec![info]);
		};

		// Monorepo with workspaces - include root project first
		let root_project =
			build_npm_root_project_info(&root_package, pm_root.clone(), &root_manifest_path)?;

		let mut projects: Vec<ProjectInfo> = vec![root_project];
		for pattern in &workspace_patterns {
			let workspace_projects =
				expand_workspace_pattern(&pm_root, pattern, self.env.fs()).await?;
			projects.extend(workspace_projects);
		}

		// Sort by path for consistent ordering
		projects.sort_by(|a, b| a.path.cmp(&b.path));

		Ok(projects)
	}

	async fn update_lock_file(&self) -> anyhow::Result<Option<std::path::PathBuf>> {
		let workspace_root = self.resolve_root().await?;
		// If a custom lock command is configured, execute it via the shell (ADR-011).
		// We can't know which file the custom command writes, so return None.
		if let Some(ref lock_command) = self.config.lock_command {
			run_custom_lock_command(&self.env, lock_command, &workspace_root).await?;
			return Ok(None);
		}
		// Auto-detect lock file and run appropriate command.
		if let Some(path) = run_lock_update(
			&self.env,
			"npm",
			&["install", "--package-lock-only", "--ignore-scripts"],
			&workspace_root,
			"package-lock.json",
		)
		.await?
		{
			return Ok(Some(path));
		}
		if let Some(path) = run_lock_update(
			&self.env,
			"pnpm",
			&["install", "--lockfile-only", "--ignore-scripts"],
			&workspace_root,
			"pnpm-lock.yaml",
		)
		.await?
		{
			return Ok(Some(path));
		}
		run_yarn_lock_update(&self.env, &workspace_root).await
	}

	async fn publish(&self, project: &ProjectInfo) -> anyhow::Result<PublishOutcome> {
		let project_dir = project.path.clone();
		let oidc = self.env.oidc_environment();
		let node_auth = self.env.node_auth_token_present();
		let access = self.config.access();

		// Warn when NODE_AUTH_TOKEN overrides OIDC trusted publishing.
		if oidc && node_auth {
			warn!(
				"{}: NODE_AUTH_TOKEN is set in an OIDC-capable CI environment; the token will \
				 take precedence over OIDC trusted publishing",
				project.name
			);
		}
		// Warn when no authentication is configured at all.
		if !oidc && !node_auth {
			warn!(
				"{}: no npm authentication detected (no OIDC environment, no NODE_AUTH_TOKEN); \
				 publish is likely to fail",
				project.name
			);
		}
		// Warn when provenance attestation is not configured for a public package in OIDC.
		if oidc && access == NpmAccess::Public && project.publishconfig_provenance != Some(true) {
			warn!(
				"{}: publishConfig.provenance is not set to true; consider adding it to \
				 package.json for explicit provenance attestations",
				project.name
			);
		}

		let mut args = vec!["publish"];

		// For scoped packages, add --access flag
		if project.name.starts_with('@') {
			args.push("--access");
			args.push(access.as_str());
		}

		// run_mut is a no-op when DryRunCommandRunner is active.
		let output = self
			.env
			.run_mut("npm", &args, &project_dir)
			.await
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

	async fn registry_name(&self) -> &str {
		"npm"
	}

	async fn manifest_filename(&self) -> &str {
		"package.json"
	}

	async fn update_dependency_version(
		&self,
		project: &ProjectInfo,
		dependency_name: &str,
		new_version: &Version,
		dry_run: bool,
	) -> anyhow::Result<Vec<PathBuf>> {
		let fs = self.env.fs();
		let manifest_path = project.path.child("package.json");
		if !fs.exists(&manifest_path).await? {
			return Ok(Vec::new());
		}

		let contents = fs
			.read_to_string(&manifest_path)
			.await
			.with_context(|| format!("Failed to read {}", manifest_path.display()))?;
		// Compute the output in a block so the non-Send CstRootNode is dropped
		// before the next `.await`.
		let output = {
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
			for s in &sections {
				modified |=
					update_dep_in_section(&obj, s, dependency_name, new_version, &manifest_path);
			}
			if modified {
				Some(format!("{}\n", root.to_string().trim_end_matches('\n')))
			} else {
				None
			}
		};

		if let Some(output) = output {
			if !dry_run {
				fs.write(&manifest_path, output.as_bytes())
					.await
					.with_context(|| format!("Failed to write {}", manifest_path.display()))?;
			}
			return Ok(vec![manifest_path.into_path_buf()]);
		}

		Ok(Vec::new())
	}
}

#[cfg(test)]
mod tests;
