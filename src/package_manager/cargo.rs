//! Cargo package manager adapter.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use glob::glob;
use semver::Version;
use serde::{Deserialize, Serialize};

use super::{PackageManagerAdapter, ProjectInfo, PublishOutcome};
use crate::command::CommandRunner;

/// Configuration for Cargo package manager.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoConfig {
	/// Whether this package manager is enabled for the project.
	#[serde(default)]
	pub enabled: bool,
	/// Optional path to the package manager root, relative to the git root.
	///
	/// When set, the package manager will look for its manifest files in this
	/// subdirectory instead of the git repository root.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub path: Option<String>,
}

impl CargoConfig {
	/// Creates a new enabled cargo configuration.
	pub fn enabled() -> Self {
		Self {
			enabled: true,
			..Default::default()
		}
	}

	/// Returns the resolved root directory for this package manager.
	///
	/// If a `path` is configured, returns `git_workdir` joined with that path.
	/// Otherwise, returns a copy of `git_workdir`.
	fn resolve_root(&self, git_workdir: &Path) -> PathBuf {
		match &self.path {
			Some(path) => git_workdir.join(path),
			None => git_workdir.to_path_buf(),
		}
	}
}

/// Adapter for Cargo-based Rust projects.
///
/// Supports both single-crate repositories and workspaces.
#[derive(Debug)]
pub struct CargoAdapter {
	/// Configuration for this package manager.
	config: CargoConfig,
	/// Git repository root path.
	git_workdir: PathBuf,
	/// Command runner for executing cargo commands.
	runner: Arc<dyn CommandRunner>,
}

impl CargoAdapter {
	/// Creates a new Cargo adapter with the given configuration.
	pub fn new(config: CargoConfig, git_workdir: PathBuf, runner: Arc<dyn CommandRunner>) -> Self {
		Self {
			config,
			git_workdir,
			runner,
		}
	}

	/// Returns the resolved root directory for this package manager.
	fn resolve_root(&self) -> PathBuf {
		self.config.resolve_root(&self.git_workdir)
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
fn read_workspace_member(
	git_workdir: &Path,
	member_path: &Path,
) -> anyhow::Result<Option<ProjectInfo>> {
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

	let path = member_path
		.strip_prefix(git_workdir)
		.context("Member path is not under git root")?
		.to_path_buf();

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
	}))
}

/// Expands a workspace member glob pattern and returns all matching projects.
///
/// Globs are resolved relative to `pm_root`, but paths in the returned
/// [`ProjectInfo`] are stripped relative to `git_workdir`.
fn expand_member_pattern(
	git_workdir: &Path,
	pm_root: &Path,
	pattern: &str,
) -> anyhow::Result<Vec<ProjectInfo>> {
	let full_pattern = pm_root.join(pattern);
	let pattern_str = full_pattern
		.to_str()
		.context("Invalid UTF-8 in workspace member pattern")?;

	glob(pattern_str)
		.with_context(|| format!("Invalid glob pattern: {}", pattern))?
		.map(|entry| {
			let member_path = entry
				.with_context(|| format!("Failed to read glob entry for pattern: {}", pattern))?;
			read_workspace_member(git_workdir, &member_path)
		})
		.filter_map(Result::transpose)
		.collect()
}

impl PackageManagerAdapter for CargoAdapter {
	fn write_version(&self, project: &ProjectInfo, version: &Version) -> anyhow::Result<()> {
		let manifest_path = self.git_workdir.join(&project.path).join("Cargo.toml");
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
		std::fs::write(&manifest_path, doc.to_string())
			.with_context(|| format!("Failed to write {}", manifest_path.display()))?;
		Ok(())
	}

	fn enumerate_projects(&self) -> anyhow::Result<Vec<ProjectInfo>> {
		let pm_root = self.resolve_root();
		let Some(root_cargo) = read_cargo_toml(&pm_root)? else {
			return Ok(Vec::new());
		};

		let pm_relative_path = pm_root
			.strip_prefix(&self.git_workdir)
			.unwrap_or(Path::new(""))
			.to_path_buf();

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
			let (version, publishable, dependency_names) =
				extract_project_metadata(&root_cargo, package).with_context(|| {
					format!(
						"Failed to extract metadata from {}",
						root_manifest_path.display()
					)
				})?;
			return Ok(vec![ProjectInfo {
				name: package.name.clone(),
				path: pm_relative_path,
				version,
				publishable,
				dependency_names,
			}]);
		};

		// Workspace with members
		let mut projects: Vec<ProjectInfo> = members
			.iter()
			.map(|pattern| expand_member_pattern(&self.git_workdir, &pm_root, pattern))
			.collect::<anyhow::Result<Vec<_>>>()?
			.into_iter()
			.flatten()
			.collect();

		// Include root package if it exists (some workspaces have a root crate too)
		if let Some(ref package) = root_cargo.package {
			let (version, publishable, dependency_names) =
				extract_project_metadata(&root_cargo, package).with_context(|| {
					format!(
						"Failed to extract metadata from {}",
						root_manifest_path.display()
					)
				})?;
			projects.insert(
				0,
				ProjectInfo {
					name: package.name.clone(),
					path: pm_relative_path,
					version,
					publishable,
					dependency_names,
				},
			);
		}

		// Sort by path for consistent ordering
		projects.sort_by(|a, b| a.path.cmp(&b.path));

		Ok(projects)
	}

	fn lock_file_path(&self) -> Option<std::path::PathBuf> {
		Some(self.resolve_root().join("Cargo.lock"))
	}

	fn update_lock_file(&self) -> anyhow::Result<Option<std::path::PathBuf>> {
		// For Cargo, always regenerate the lock file at the workspace root
		let workspace_root = self.resolve_root();

		let output = self
			.runner
			.run("cargo", &["generate-lockfile"], &workspace_root)
			.with_context(|| {
				format!(
					"Failed to execute cargo generate-lockfile in {}",
					workspace_root.display()
				)
			})?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			anyhow::bail!(
				"cargo generate-lockfile failed in {}: {}",
				workspace_root.display(),
				stderr
			);
		}

		Ok(Some(workspace_root.join("Cargo.lock")))
	}

	fn publish(&self, project: &ProjectInfo) -> anyhow::Result<PublishOutcome> {
		let manifest_path = self.git_workdir.join(&project.path).join("Cargo.toml");
		let manifest_str = manifest_path.to_string_lossy();

		let output = self
			.runner
			.run(
				"cargo",
				&["publish", "--manifest-path", &manifest_str],
				&self.git_workdir,
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
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	fn write_cargo_toml(dir: &Path, content: &str) {
		std::fs::write(dir.join("Cargo.toml"), content).unwrap();
	}

	use std::sync::Arc;

	use crate::command::test_support::RecordingCommandRunner;

	/// Creates a `CargoAdapter` backed by a fresh recording runner with the given exit code.
	fn recording_adapter(config: CargoConfig, dir: &Path, exit_code: i32) -> CargoAdapter {
		CargoAdapter::new(
			config,
			dir.to_path_buf(),
			Arc::new(RecordingCommandRunner::new(exit_code)),
		)
	}

	/// Creates a `CargoAdapter` backed by a shared recording runner for inspection.
	fn recording_adapter_inspectable(
		config: CargoConfig,
		dir: &Path,
		runner: Arc<RecordingCommandRunner>,
	) -> CargoAdapter {
		CargoAdapter::new(config, dir.to_path_buf(), runner)
	}

	/// Helper to enumerate projects using the adapter with no configured path.
	fn enumerate(dir: &Path) -> anyhow::Result<Vec<ProjectInfo>> {
		recording_adapter(CargoConfig::default(), dir, 0).enumerate_projects()
	}

	/// Helper to enumerate projects using the adapter with a configured path.
	fn enumerate_with_path(dir: &Path, path: &str) -> anyhow::Result<Vec<ProjectInfo>> {
		recording_adapter(
			CargoConfig {
				enabled: true,
				path: Some(path.to_string()),
			},
			dir,
			0,
		)
		.enumerate_projects()
	}

	#[test]
	fn enumerate_returns_empty_when_no_cargo_toml() {
		let dir = temp_dir();
		let projects = enumerate(dir.path()).unwrap();
		assert!(projects.is_empty());
	}

	#[test]
	fn enumerate_single_crate() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "0.1.0"
"#,
		);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "my-crate");
		assert_eq!(projects[0].path, Path::new(""));
	}

	#[test]
	fn enumerate_virtual_manifest_no_members() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[workspace]
"#,
		);

		let projects = enumerate(dir.path()).unwrap();
		assert!(projects.is_empty());
	}

	#[test]
	fn enumerate_workspace_members() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[workspace]
members = ["crates/*"]
"#,
		);

		let crate_a = dir.path().join("crates/crate-a");
		let crate_b = dir.path().join("crates/crate-b");
		std::fs::create_dir_all(&crate_a).unwrap();
		std::fs::create_dir_all(&crate_b).unwrap();
		write_cargo_toml(
			&crate_a,
			r#"
[package]
name = "crate-a"
version = "0.1.0"
"#,
		);
		write_cargo_toml(
			&crate_b,
			r#"
[package]
name = "crate-b"
version = "0.1.0"
"#,
		);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "crate-a");
		assert_eq!(projects[0].path, Path::new("crates/crate-a"));
		assert_eq!(projects[1].name, "crate-b");
		assert_eq!(projects[1].path, Path::new("crates/crate-b"));
	}

	#[test]
	fn enumerate_workspace_with_root_package() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "root-crate"
version = "0.1.0"

[workspace]
members = ["crates/*"]
"#,
		);

		let member = dir.path().join("crates/member");
		std::fs::create_dir_all(&member).unwrap();
		write_cargo_toml(
			&member,
			r#"
[package]
name = "member-crate"
version = "0.1.0"
"#,
		);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		// Root comes first (empty path sorts first)
		assert_eq!(projects[0].name, "root-crate");
		assert_eq!(projects[0].path, Path::new(""));
		assert_eq!(projects[1].name, "member-crate");
		assert_eq!(projects[1].path, Path::new("crates/member"));
	}

	#[test]
	fn enumerate_multiple_member_patterns() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[workspace]
members = ["crates/*", "tools/*"]
"#,
		);

		let crate_dir = dir.path().join("crates/lib");
		let tool_dir = dir.path().join("tools/cli");
		std::fs::create_dir_all(&crate_dir).unwrap();
		std::fs::create_dir_all(&tool_dir).unwrap();
		write_cargo_toml(
			&crate_dir,
			r#"
[package]
name = "lib"
version = "0.1.0"
"#,
		);
		write_cargo_toml(
			&tool_dir,
			r#"
[package]
name = "cli"
version = "0.1.0"
"#,
		);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "lib");
		assert_eq!(projects[0].path, Path::new("crates/lib"));
		assert_eq!(projects[1].name, "cli");
		assert_eq!(projects[1].path, Path::new("tools/cli"));
	}

	#[test]
	fn enumerate_skips_directories_without_cargo_toml() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[workspace]
members = ["crates/*"]
"#,
		);

		let valid = dir.path().join("crates/valid");
		let no_cargo = dir.path().join("crates/no-cargo-toml");
		std::fs::create_dir_all(&valid).unwrap();
		std::fs::create_dir_all(&no_cargo).unwrap();
		write_cargo_toml(
			&valid,
			r#"
[package]
name = "valid"
version = "0.1.0"
"#,
		);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "valid");
	}

	#[test]
	fn enumerate_skips_virtual_manifest_members() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[workspace]
members = ["crates/*"]
"#,
		);

		let real_crate = dir.path().join("crates/real");
		let virtual_manifest = dir.path().join("crates/virtual");
		std::fs::create_dir_all(&real_crate).unwrap();
		std::fs::create_dir_all(&virtual_manifest).unwrap();
		write_cargo_toml(
			&real_crate,
			r#"
[package]
name = "real"
version = "0.1.0"
"#,
		);
		// Virtual manifest has [workspace] but no [package]
		write_cargo_toml(
			&virtual_manifest,
			r#"
[workspace]
members = []
"#,
		);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "real");
	}

	#[test]
	fn enumerate_fails_on_invalid_cargo_toml() {
		let dir = temp_dir();
		write_cargo_toml(dir.path(), "not valid toml [[[");

		let result = enumerate(dir.path());

		assert!(result.is_err());
	}

	#[test]
	fn enumerate_fails_on_invalid_member_cargo_toml() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[workspace]
members = ["crates/*"]
"#,
		);

		let bad = dir.path().join("crates/bad");
		std::fs::create_dir_all(&bad).unwrap();
		write_cargo_toml(&bad, "invalid toml");

		let result = enumerate(dir.path());

		assert!(result.is_err());
	}

	#[test]
	fn new_creates_adapter() {
		let dir = temp_dir();
		let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
		let _ = adapter.enumerate_projects();
	}

	#[test]
	fn enumerate_single_crate_in_subfolder() {
		let dir = temp_dir();
		let subfolder = dir.path().join("backend");
		std::fs::create_dir_all(&subfolder).unwrap();
		write_cargo_toml(
			&subfolder,
			r#"
[package]
name = "my-crate"
version = "0.1.0"
"#,
		);

		let projects = enumerate_with_path(dir.path(), "backend").unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "my-crate");
		assert_eq!(projects[0].path, Path::new("backend"));
	}

	#[test]
	fn enumerate_workspace_in_subfolder() {
		let dir = temp_dir();
		let subfolder = dir.path().join("backend");
		std::fs::create_dir_all(&subfolder).unwrap();
		write_cargo_toml(
			&subfolder,
			r#"
[workspace]
members = ["crates/*"]
"#,
		);

		let crate_a = subfolder.join("crates/crate-a");
		let crate_b = subfolder.join("crates/crate-b");
		std::fs::create_dir_all(&crate_a).unwrap();
		std::fs::create_dir_all(&crate_b).unwrap();
		write_cargo_toml(
			&crate_a,
			r#"
[package]
name = "crate-a"
version = "0.1.0"
"#,
		);
		write_cargo_toml(
			&crate_b,
			r#"
[package]
name = "crate-b"
version = "0.1.0"
"#,
		);

		let projects = enumerate_with_path(dir.path(), "backend").unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "crate-a");
		assert_eq!(projects[0].path, Path::new("backend/crates/crate-a"));
		assert_eq!(projects[1].name, "crate-b");
		assert_eq!(projects[1].path, Path::new("backend/crates/crate-b"));
	}

	#[test]
	fn enumerate_returns_empty_when_subfolder_missing() {
		let dir = temp_dir();
		let projects = enumerate_with_path(dir.path(), "nonexistent").unwrap();
		assert!(projects.is_empty());
	}

	fn project_info(name: &str, path: &str) -> ProjectInfo {
		ProjectInfo {
			name: name.to_string(),
			path: std::path::PathBuf::from(path),
			..Default::default()
		}
	}

	#[test]
	fn enumerate_includes_version() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "1.2.3"
"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].version.to_string(), "1.2.3");
	}

	#[test]
	fn enumerate_missing_version_fails() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
"#,
		);
		let result = enumerate(dir.path());
		assert!(result.is_err());
	}

	#[test]
	fn enumerate_invalid_semver_fails() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "not-a-version"
"#,
		);
		let result = enumerate(dir.path());
		assert!(result.is_err());
	}

	#[test]
	fn write_version_file_not_found() {
		let dir = temp_dir();
		let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
		let info = project_info("my-crate", "");
		let version: semver::Version = "1.0.0".parse().unwrap();
		let result = adapter.write_version(&info, &version);
		assert!(result.is_err());
	}

	#[test]
	fn write_version_invalid_toml() {
		let dir = temp_dir();
		write_cargo_toml(dir.path(), "not valid toml [[[");
		let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
		let info = project_info("my-crate", "");
		let version: semver::Version = "1.0.0".parse().unwrap();
		let result = adapter.write_version(&info, &version);
		assert!(result.is_err());
	}

	#[test]
	fn write_version_missing_package_section() {
		let dir = temp_dir();
		write_cargo_toml(dir.path(), "[dependencies]\n");
		let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
		let info = project_info("my-crate", "");
		let version: semver::Version = "1.0.0".parse().unwrap();
		let result = adapter.write_version(&info, &version);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("No [package] table")
		);
	}

	#[test]
	fn write_version_updates_cargo_toml() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "1.0.0"
edition = "2024"
"#,
		);
		let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
		let info = project_info("my-crate", "");
		let new_version: semver::Version = "2.0.0".parse().unwrap();
		adapter.write_version(&info, &new_version).unwrap();

		let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
		assert!(contents.contains("version = \"2.0.0\""));
		// Preserve other fields
		assert!(contents.contains("edition = \"2024\""));
	}

	#[test]
	fn write_version_roundtrip() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "0.1.0"
"#,
		);
		let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
		let info = project_info("my-crate", "");

		let new_v: semver::Version = "0.2.0".parse().unwrap();
		adapter.write_version(&info, &new_v).unwrap();

		// Re-enumerate to verify the write
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].version.to_string(), "0.2.0");
	}

	#[test]
	fn cargo_config_defaults_to_disabled() {
		let config = CargoConfig::default();
		assert!(!config.enabled);
		assert_eq!(config.path, None);
	}

	#[test]
	fn cargo_config_enabled_creates_enabled_config() {
		let config = CargoConfig::enabled();
		assert!(config.enabled);
		assert_eq!(config.path, None);
	}

	#[test]
	fn cargo_config_resolve_root_without_path() {
		let config = CargoConfig {
			enabled: true,
			path: None,
		};
		let git_workdir = Path::new("/repo");
		let resolved = config.resolve_root(git_workdir);
		assert_eq!(resolved, git_workdir);
	}

	#[test]
	fn cargo_config_resolve_root_with_path() {
		let config = CargoConfig {
			enabled: true,
			path: Some("rust-workspace".to_string()),
		};
		let git_workdir = Path::new("/repo");
		let resolved = config.resolve_root(git_workdir);
		assert_eq!(resolved, Path::new("/repo/rust-workspace"));
	}

	#[test]
	fn update_lock_file_command_failure_propagates_error() {
		let dir = temp_dir();
		let runner = Arc::new(
			RecordingCommandRunner::new(1).with_stderr(b"error: invalid manifest".to_vec()),
		);
		let adapter = recording_adapter_inspectable(CargoConfig::default(), dir.path(), runner);

		let result = adapter.update_lock_file();
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("cargo generate-lockfile failed"),
			"Expected 'cargo generate-lockfile failed', got: {msg}"
		);
	}

	#[test]
	fn update_lock_file_passes_correct_args() {
		let dir = temp_dir();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter =
			recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));

		let result = adapter.update_lock_file();
		assert_eq!(result.unwrap(), Some(dir.path().join("Cargo.lock")));

		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "cargo");
		assert_eq!(invocations[0].args, ["generate-lockfile"]);
		assert_eq!(invocations[0].cwd, dir.path());
	}

	#[test]
	fn enumerate_includes_publishable_status() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "1.0.0"
"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert!(
			projects[0].publishable,
			"Crate without publish field should be publishable"
		);
	}

	#[test]
	fn enumerate_publishable_false_for_publish_false() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "1.0.0"
publish = false
"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert!(
			!projects[0].publishable,
			"Crate with publish = false should not be publishable"
		);
	}

	#[test]
	fn enumerate_publishable_false_for_empty_array() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "1.0.0"
publish = []
"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert!(
			!projects[0].publishable,
			"Crate with publish = [] should not be publishable"
		);
	}

	#[test]
	fn enumerate_publishable_true_for_publish_true() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "1.0.0"
publish = true
"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert!(
			projects[0].publishable,
			"Crate with publish = true should be publishable"
		);
	}

	#[test]
	fn enumerate_publishable_true_for_registry_array() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "1.0.0"
publish = ["crates-io"]
"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert!(
			projects[0].publishable,
			"Crate with publish = [\"crates-io\"] should be publishable"
		);
	}

	#[test]
	fn enumerate_includes_dependency_names() {
		let dir = temp_dir();
		write_cargo_toml(
			dir.path(),
			r#"
[package]
name = "my-crate"
version = "1.0.0"

[dependencies]
serde = "1.0"
tokio = "1.0"

[dev-dependencies]
tempfile = "3.0"
"#,
		);
		let projects = enumerate(dir.path()).unwrap();
		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].dependency_names.len(), 3);
		assert!(projects[0].dependency_names.contains(&"serde".to_string()));
		assert!(projects[0].dependency_names.contains(&"tokio".to_string()));
		assert!(
			projects[0]
				.dependency_names
				.contains(&"tempfile".to_string())
		);
	}

	fn setup_publish_project(dir: &Path) -> ProjectInfo {
		write_cargo_toml(
			dir,
			"[package]\nname = \"my-crate\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
		);
		ProjectInfo {
			name: "my-crate".to_string(),
			path: std::path::PathBuf::new(),
			..Default::default()
		}
	}

	#[test]
	fn publish_success_returns_published() {
		let dir = temp_dir();
		let info = setup_publish_project(dir.path());
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter =
			recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
		let result = adapter.publish(&info).unwrap();
		assert_eq!(result, PublishOutcome::Published);
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "cargo");
		assert_eq!(invocations[0].args[0], "publish");
	}

	#[test]
	fn publish_already_uploaded_returns_already_published() {
		let dir = temp_dir();
		let info = setup_publish_project(dir.path());
		let runner = Arc::new(
			RecordingCommandRunner::new(1)
				.with_stderr(b"error: crate version is already uploaded".to_vec()),
		);
		let adapter =
			recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
		let result = adapter.publish(&info).unwrap();
		assert_eq!(result, PublishOutcome::AlreadyPublished);
	}

	#[test]
	fn publish_already_exists_returns_already_published() {
		let dir = temp_dir();
		let info = setup_publish_project(dir.path());
		let runner = Arc::new(
			RecordingCommandRunner::new(1)
				.with_stderr(b"error: package already exists on crates.io".to_vec()),
		);
		let adapter =
			recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
		let result = adapter.publish(&info).unwrap();
		assert_eq!(result, PublishOutcome::AlreadyPublished);
	}

	#[test]
	fn publish_other_failure_returns_error() {
		let dir = temp_dir();
		let info = setup_publish_project(dir.path());
		let runner = Arc::new(
			RecordingCommandRunner::new(1)
				.with_stderr(b"error: network error connecting to crates.io".to_vec()),
		);
		let adapter =
			recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
		let result = adapter.publish(&info);
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("cargo publish failed"),
			"Expected 'cargo publish failed', got: {msg}"
		);
	}

	#[test]
	fn publish_passes_manifest_path_arg() {
		let dir = temp_dir();
		let info = setup_publish_project(dir.path());
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let adapter =
			recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
		adapter.publish(&info).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(
			invocations[0].args.contains(&"--manifest-path".to_string()),
			"Should pass --manifest-path, got: {:?}",
			invocations[0].args
		);
	}
}
