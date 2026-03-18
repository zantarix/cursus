//! Cargo package manager adapter.

use std::path::{Path, PathBuf};

use anyhow::Context;
use semver::Version;
use serde::Deserialize;

use log::warn;

use super::{PackageManagerAdapter, ProjectInfo, PublishOutcome};
use crate::model::config::CargoConfig;
use crate::path::AbsolutePath;

/// Adapter for Cargo-based Rust projects.
///
/// Supports both single-crate repositories and workspaces.
#[derive(Debug)]
pub struct CargoAdapter {
	/// Configuration for this package manager.
	config: CargoConfig,
	/// Package manager root path.
	adapter_root: AbsolutePath,
	/// Environment for executing cargo commands.
	env: crate::Env,
}

impl CargoAdapter {
	/// Creates a new Cargo adapter with the given configuration.
	pub fn new(config: CargoConfig, adapter_root: AbsolutePath, env: crate::Env) -> Self {
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

/// Represents the relevant fields from Cargo.toml.
#[derive(Debug, Deserialize)]
struct CargoToml {
	package: Option<Package>,
	workspace: Option<Workspace>,
	dependencies: Option<std::collections::HashMap<String, toml::Value>>,
	#[serde(rename = "dev-dependencies")]
	dev_dependencies: Option<std::collections::HashMap<String, toml::Value>>,
	#[serde(rename = "build-dependencies")]
	build_dependencies: Option<std::collections::HashMap<String, toml::Value>>,
}

/// The [package] section of Cargo.toml.
#[derive(Debug, Deserialize)]
struct Package {
	name: String,
	version: Option<String>,
	publish: Option<PublishField>,
}

/// The publish field can be either a boolean or an array of registry names.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PublishField {
	Bool(bool),
	Registries(Vec<String>),
}

/// The [workspace] section of Cargo.toml.
#[derive(Debug, Deserialize)]
struct Workspace {
	members: Option<Vec<String>>,
}

/// Reads and parses a Cargo.toml file from a directory.
///
/// Returns `Ok(None)` if the file doesn't exist, `Ok(Some(cargo))` if parsed
/// successfully, or an error if the file exists but cannot be parsed.
fn read_cargo_toml(dir: &Path) -> anyhow::Result<Option<CargoToml>> {
	let path = dir.join("Cargo.toml");
	if !path.exists() {
		return Ok(None);
	}
	let contents = std::fs::read_to_string(&path)
		.with_context(|| format!("Failed to read {}", path.display()))?;
	let cargo: CargoToml =
		toml::from_str(&contents).with_context(|| format!("Failed to parse {}", path.display()))?;
	Ok(Some(cargo))
}

/// Extracts project metadata from a parsed Cargo.toml.
///
/// Returns version, publishable status, and dependency names.
fn extract_project_metadata(
	cargo: &CargoToml,
	package: &Package,
) -> anyhow::Result<(Version, bool, Vec<String>)> {
	// Extract version
	let version_str = package
		.version
		.as_deref()
		.context("Missing version in package section")?;
	let version = version_str
		.parse::<Version>()
		.with_context(|| format!("Invalid semver version: {version_str}"))?;

	// Determine if publishable
	let publishable = match &package.publish {
		Some(PublishField::Bool(false)) => false,
		Some(PublishField::Registries(registries)) if registries.is_empty() => false,
		_ => true,
	};

	// Collect dependency names from all dependency sections
	let mut dependency_names = Vec::new();
	for deps_map in [
		&cargo.dependencies,
		&cargo.dev_dependencies,
		&cargo.build_dependencies,
	]
	.into_iter()
	.flatten()
	{
		dependency_names.extend(deps_map.keys().cloned());
	}

	Ok((version, publishable, dependency_names))
}

/// Attempts to create a ProjectInfo from a workspace member directory.
///
/// Returns `Ok(None)` if the path is not a valid crate (not a directory or no Cargo.toml).
fn read_workspace_member(member_path: &Path) -> anyhow::Result<Option<ProjectInfo>> {
	if !member_path.is_dir() {
		return Ok(None);
	}

	let Some(cargo) = read_cargo_toml(member_path)? else {
		return Ok(None);
	};

	let Some(ref package) = cargo.package else {
		// Virtual manifest (workspace-only Cargo.toml without [package])
		return Ok(None);
	};

	let path = AbsolutePath::new(member_path.to_path_buf()).with_context(|| {
		format!(
			"workspace member path is not absolute: {}",
			member_path.display()
		)
	})?;

	let manifest_path = member_path.join("Cargo.toml");
	let (version, publishable, dependency_names) = extract_project_metadata(&cargo, package)
		.with_context(|| {
			format!(
				"Failed to extract metadata from {}",
				manifest_path.display()
			)
		})?;

	Ok(Some(ProjectInfo {
		name: package.name.clone(),
		path,
		version,
		publishable,
		dependency_names,
		publishconfig_provenance: None,
	}))
}

/// Expands a workspace member glob pattern and returns all matching projects.
///
/// Globs are resolved relative to `pm_root`. Paths in the returned
/// [`ProjectInfo`] are absolute paths to each member directory. Only paths
/// that remain within `pm_root` are returned; paths that escape via `..` or
/// symlinks are rejected with an error.
fn expand_member_pattern(
	pm_root: &AbsolutePath,
	pattern: &str,
) -> anyhow::Result<Vec<ProjectInfo>> {
	pm_root
		.safe_glob(pattern)?
		.into_iter()
		.map(|member_path| read_workspace_member(&member_path))
		.filter_map(Result::transpose)
		.collect()
}

/// Updates the version in a `toml_edit::Item` representing a Cargo dependency.
///
/// The item may be:
/// - A string (`"1.0.0"` or `"^1.0.0"`): the string is replaced preserving any prefix.
/// - A table with a `version` key (`{ version = "1.0.0", features = [...] }`): the
///   `version` key is updated. If the table has no `version` key (e.g. a path-only
///   dependency like `{ path = "../foo" }`), the item is left unchanged.
///
/// Returns `true` if the item was modified.
fn update_dep_item_version(item: &mut toml_edit::Item, new_version: &str) -> bool {
	if let Some(table) = item.as_table_like_mut() {
		// Only update if a version key already exists; don't inject one into path-only deps.
		let Some(old_version) = table.get("version").and_then(|v| v.as_str()) else {
			return false;
		};
		let prefix = super::semver_range_prefix(old_version).to_string();
		table.insert(
			"version",
			toml_edit::value(format!("{prefix}{new_version}")),
		);
		true
	} else if let Some(old_str) = item.as_str() {
		let prefix = super::semver_range_prefix(old_str).to_string();
		*item = toml_edit::value(format!("{prefix}{new_version}"));
		true
	} else {
		false
	}
}

/// Updates the version of a named dependency in `[workspace.dependencies]`.
///
/// Reads the Cargo.toml at `workspace_toml_path`, finds the entry under
/// `workspace.dependencies`, updates its version, and writes the file back.
/// Returns `true` if the file was modified.
fn update_workspace_dep(
	workspace_toml_path: &Path,
	dependency_name: &str,
	new_version: &str,
	dry_run: bool,
) -> anyhow::Result<bool> {
	if !workspace_toml_path.exists() {
		return Ok(false);
	}
	let contents = std::fs::read_to_string(workspace_toml_path)
		.with_context(|| format!("Failed to read {}", workspace_toml_path.display()))?;
	let mut doc = contents
		.parse::<toml_edit::DocumentMut>()
		.with_context(|| format!("Failed to parse {}", workspace_toml_path.display()))?;

	let workspace_dep = doc
		.get_mut("workspace")
		.and_then(|ws| ws.get_mut("dependencies"))
		.and_then(|deps| deps.get_mut(dependency_name));

	if let Some(dep_item) = workspace_dep
		&& update_dep_item_version(dep_item, new_version)
	{
		if !dry_run {
			std::fs::write(workspace_toml_path, doc.to_string())
				.with_context(|| format!("Failed to write {}", workspace_toml_path.display()))?;
		}
		return Ok(true);
	}
	Ok(false)
}

/// Updates the version of a named dependency in a member Cargo.toml.
///
/// Scans `[dependencies]`, `[dev-dependencies]`, and `[build-dependencies]`.
/// Entries with `workspace = true` are skipped (those are managed via the
/// workspace root). Writes the file if any entry was modified (skipped when
/// `dry_run` is `true`). Returns `true` if the file was (or would be) modified.
fn update_member_dep(
	member_toml_path: &Path,
	dependency_name: &str,
	new_version: &str,
	dry_run: bool,
) -> anyhow::Result<bool> {
	if !member_toml_path.exists() {
		return Ok(false);
	}
	let contents = std::fs::read_to_string(member_toml_path)
		.with_context(|| format!("Failed to read {}", member_toml_path.display()))?;
	let mut doc = contents
		.parse::<toml_edit::DocumentMut>()
		.with_context(|| format!("Failed to parse {}", member_toml_path.display()))?;

	let mut changed = false;
	for section_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
		let Some(dep_item) = doc
			.get_mut(section_name)
			.and_then(|s| s.get_mut(dependency_name))
		else {
			continue;
		};

		// Skip entries that inherit from the workspace
		if dep_item.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
			continue;
		}

		if update_dep_item_version(dep_item, new_version) {
			changed = true;
		}
	}

	if changed && !dry_run {
		std::fs::write(member_toml_path, doc.to_string())
			.with_context(|| format!("Failed to write {}", member_toml_path.display()))?;
	}
	Ok(changed)
}

/// Builds a `ProjectInfo` for the root Cargo package.
///
/// Used for both the single-crate case and the root package in a workspace.
fn build_cargo_root_project_info(
	root_cargo: &CargoToml,
	package: &Package,
	pm_root: &AbsolutePath,
	root_manifest_path: &Path,
) -> anyhow::Result<ProjectInfo> {
	let (version, publishable, dependency_names) = extract_project_metadata(root_cargo, package)
		.with_context(|| {
			format!(
				"Failed to extract metadata from {}",
				root_manifest_path.display()
			)
		})?;
	Ok(ProjectInfo {
		name: package.name.clone(),
		path: pm_root.clone(),
		version,
		publishable,
		dependency_names,
		publishconfig_provenance: None,
	})
}

impl PackageManagerAdapter for CargoAdapter {
	fn write_version(
		&self,
		project: &ProjectInfo,
		version: &Version,
		dry_run: bool,
	) -> anyhow::Result<()> {
		let manifest_path = project.path.join("Cargo.toml");
		let contents = std::fs::read_to_string(&manifest_path)
			.with_context(|| format!("Failed to read {}", manifest_path.display()))?;
		let mut doc = contents
			.parse::<toml_edit::DocumentMut>()
			.with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
		let package = doc
			.get_mut("package")
			.and_then(|p| p.as_table_like_mut())
			.with_context(|| format!("No [package] table in {}", manifest_path.display()))?;
		package.insert("version", toml_edit::value(version.to_string()));
		if !dry_run {
			std::fs::write(&manifest_path, doc.to_string())
				.with_context(|| format!("Failed to write {}", manifest_path.display()))?;
		}
		Ok(())
	}

	fn enumerate_projects(&self) -> anyhow::Result<Vec<ProjectInfo>> {
		let pm_root = self.resolve_root()?;
		let Some(root_cargo) = read_cargo_toml(&pm_root)? else {
			return Ok(Vec::new());
		};

		let root_manifest_path = pm_root.join("Cargo.toml");

		// Check for workspace members
		let workspace_members = root_cargo
			.workspace
			.as_ref()
			.and_then(|ws| ws.members.as_ref())
			.filter(|members| !members.is_empty());

		let Some(members) = workspace_members else {
			// Single crate repository
			let Some(ref package) = root_cargo.package else {
				// Virtual manifest with no members - nothing to enumerate
				return Ok(Vec::new());
			};
			let info =
				build_cargo_root_project_info(&root_cargo, package, &pm_root, &root_manifest_path)?;
			return Ok(vec![info]);
		};

		// Workspace with members
		let mut projects: Vec<ProjectInfo> = members
			.iter()
			.map(|pattern| expand_member_pattern(&pm_root, pattern))
			.collect::<anyhow::Result<Vec<_>>>()?
			.into_iter()
			.flatten()
			.collect();

		// Include root package if it exists (some workspaces have a root crate too)
		if let Some(ref package) = root_cargo.package {
			let info =
				build_cargo_root_project_info(&root_cargo, package, &pm_root, &root_manifest_path)?;
			projects.insert(0, info);
		}

		// Sort by path for consistent ordering
		projects.sort_by(|a, b| a.path.cmp(&b.path));

		Ok(projects)
	}

	fn update_lock_file(&self) -> anyhow::Result<Option<std::path::PathBuf>> {
		// Resolve the lock file path unconditionally — this is known regardless of dry-run.
		let workspace_root = self.resolve_root()?;
		let lock_path = workspace_root.join("Cargo.lock");

		// run_mut is a no-op when DryRunCommandRunner is active, so this is always safe to call.
		let output = self
			.env
			.run_mut("cargo", &["update", "--workspace"], &workspace_root)
			.with_context(|| {
				format!(
					"Failed to execute cargo update --workspace in {}",
					workspace_root.display()
				)
			})?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			anyhow::bail!(
				"cargo update --workspace failed in {}: {}",
				workspace_root.display(),
				stderr
			);
		}

		Ok(Some(lock_path))
	}

	fn publish(&self, project: &ProjectInfo) -> anyhow::Result<PublishOutcome> {
		if !self.env.cargo_registry_token_present() {
			warn!(
				"{}: CARGO_REGISTRY_TOKEN is not set; publish may fail if no other \
				 authentication is configured",
				project.name
			);
		}

		let manifest_path = project.path.join("Cargo.toml");
		let manifest_str = manifest_path.to_string_lossy();

		let output = self
			.env
			.run_mut(
				"cargo",
				&["publish", "--manifest-path", &manifest_str],
				&self.adapter_root,
			)
			.with_context(|| {
				format!(
					"Failed to execute cargo publish for {}",
					manifest_path.display()
				)
			})?;

		if output.status.success() {
			return Ok(PublishOutcome::Published);
		}

		// Check if the failure is because the version already exists
		let stderr = String::from_utf8_lossy(&output.stderr);
		if stderr.contains("is already uploaded") || stderr.contains("already exists") {
			return Ok(PublishOutcome::AlreadyPublished);
		}

		// Some other error
		anyhow::bail!(
			"cargo publish failed for {}: {}",
			manifest_path.display(),
			stderr
		);
	}

	fn registry_name(&self) -> &str {
		"crates.io"
	}

	fn manifest_filename(&self) -> &str {
		"Cargo.toml"
	}

	fn update_dependency_version(
		&self,
		project: &ProjectInfo,
		dependency_name: &str,
		new_version: &Version,
		dry_run: bool,
	) -> anyhow::Result<Vec<PathBuf>> {
		let pm_root = self.resolve_root()?;
		let version_str = new_version.to_string();
		let mut modified = Vec::new();

		let workspace_toml_path = pm_root.join("Cargo.toml");
		if update_workspace_dep(&workspace_toml_path, dependency_name, &version_str, dry_run)? {
			modified.push(workspace_toml_path.clone());
		}

		// Skip member update when the member IS the workspace root (already handled above)
		let member_toml_path = project.path.join("Cargo.toml");
		if member_toml_path != workspace_toml_path
			&& update_member_dep(&member_toml_path, dependency_name, &version_str, dry_run)?
		{
			modified.push(member_toml_path);
		}

		Ok(modified)
	}
}

#[cfg(test)]
mod tests;
