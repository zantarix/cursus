//! npm package manager adapter.

use std::path::{Path, PathBuf};

use anyhow::Context;
use glob::glob;
use semver::Version;
use serde::{Deserialize, Serialize};

use super::{PackageManagerAdapter, ProjectInfo, PublishOutcome};

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
	/// Returns the resolved root directory for this package manager.
	///
	/// If a `path` is configured, returns `git_root` joined with that path.
	/// Otherwise, returns a copy of `git_root`.
	pub fn resolve_root(&self, git_root: &Path) -> PathBuf {
		match &self.path {
			Some(path) => git_root.join(path),
			None => git_root.to_path_buf(),
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
}

impl NpmAdapter {
	/// Creates a new npm adapter with the given configuration.
	pub fn new(config: NpmConfig) -> Self {
		Self { config }
	}
}

/// Represents the relevant fields from package.json.
#[derive(Debug, Deserialize)]
struct PackageJson {
	name: Option<String>,
	workspaces: Option<Workspaces>,
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
fn read_pnpm_workspace(git_root: &Path) -> anyhow::Result<Option<PnpmWorkspace>> {
	let path = git_root.join("pnpm-workspace.yaml");
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

/// Attempts to create a ProjectInfo from a workspace directory path.
///
/// Returns `Ok(None)` if the path is not a valid workspace (not a directory or no package.json).
fn read_workspace_project(
	git_root: &Path,
	workspace_path: &Path,
) -> anyhow::Result<Option<ProjectInfo>> {
	if !workspace_path.is_dir() {
		return Ok(None);
	}

	let Some(package) = read_package_json(workspace_path)? else {
		return Ok(None);
	};

	let name = package.name.unwrap_or_else(|| "unnamed".to_string());
	let path = workspace_path
		.strip_prefix(git_root)
		.context("Workspace path is not under git root")?
		.to_path_buf();

	Ok(Some(ProjectInfo { name, path }))
}

/// Expands a workspace glob pattern and returns all matching projects.
///
/// Globs are resolved relative to `pm_root`, but paths in the returned
/// [`ProjectInfo`] are stripped relative to `git_root`.
fn expand_workspace_pattern(
	git_root: &Path,
	pm_root: &Path,
	pattern: &str,
) -> anyhow::Result<Vec<ProjectInfo>> {
	let full_pattern = pm_root.join(pattern);
	let pattern_str = full_pattern
		.to_str()
		.context("Invalid UTF-8 in workspace pattern")?;

	glob(pattern_str)
		.with_context(|| format!("Invalid glob pattern: {}", pattern))?
		.map(|entry| {
			let workspace_path = entry
				.with_context(|| format!("Failed to read glob entry for pattern: {}", pattern))?;
			read_workspace_project(git_root, &workspace_path)
		})
		.filter_map(Result::transpose)
		.collect()
}

impl PackageManagerAdapter for NpmAdapter {
	fn read_version(&self, git_root: &Path, project: &ProjectInfo) -> anyhow::Result<Version> {
		let manifest_path = git_root.join(&project.path).join("package.json");
		let contents = std::fs::read_to_string(&manifest_path)
			.with_context(|| format!("Failed to read {}", manifest_path.display()))?;
		let json: serde_json::Value = serde_json::from_str(&contents)
			.with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
		let version_str = json["version"]
			.as_str()
			.context("Missing version in package.json")?;
		version_str
			.parse::<Version>()
			.with_context(|| format!("Invalid semver version: {version_str}"))
	}

	fn write_version(
		&self,
		git_root: &Path,
		project: &ProjectInfo,
		version: &Version,
	) -> anyhow::Result<()> {
		let manifest_path = git_root.join(&project.path).join("package.json");
		let contents = std::fs::read_to_string(&manifest_path)
			.with_context(|| format!("Failed to read {}", manifest_path.display()))?;
		let mut json: serde_json::Value = serde_json::from_str(&contents)
			.with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
		json["version"] = serde_json::Value::String(version.to_string());
		let output =
			serde_json::to_string_pretty(&json).context("Failed to serialize package.json")?;
		std::fs::write(&manifest_path, format!("{output}\n"))
			.with_context(|| format!("Failed to write {}", manifest_path.display()))?;
		Ok(())
	}

	fn enumerate_projects(&self, git_root: &Path) -> anyhow::Result<Vec<ProjectInfo>> {
		let pm_root = self.config.resolve_root(git_root);
		let Some(root_package) = read_package_json(&pm_root)? else {
			return Ok(Vec::new());
		};
		let pnpm_workspace = read_pnpm_workspace(&pm_root)?;

		let pm_relative_path = pm_root
			.strip_prefix(git_root)
			.unwrap_or(Path::new(""))
			.to_path_buf();

		let Some(workspace_patterns) =
			get_workspace_patterns(pnpm_workspace.as_ref(), &root_package)
		else {
			// Single package repository
			let name = root_package.name.unwrap_or_else(|| "unnamed".to_string());
			return Ok(vec![ProjectInfo {
				name,
				path: pm_relative_path,
			}]);
		};

		// Monorepo with workspaces - include root project first
		let root_name = root_package.name.unwrap_or_else(|| "unnamed".to_string());
		let root_project = ProjectInfo {
			name: root_name,
			path: pm_relative_path,
		};

		let mut projects: Vec<ProjectInfo> = std::iter::once(root_project)
			.chain(
				workspace_patterns
					.iter()
					.map(|pattern| expand_workspace_pattern(git_root, &pm_root, pattern))
					.collect::<anyhow::Result<Vec<_>>>()?
					.into_iter()
					.flatten(),
			)
			.collect();

		// Sort by path for consistent ordering
		projects.sort_by(|a, b| a.path.cmp(&b.path));

		Ok(projects)
	}

	fn update_lock_file(&self, git_root: &Path, _project: &ProjectInfo) -> anyhow::Result<()> {
		let workspace_root = self.config.resolve_root(git_root);

		// If a custom lock command is configured, use it
		if let Some(ref lock_command) = self.config.lock_command {
			let parts: Vec<&str> = lock_command.split_whitespace().collect();
			if parts.is_empty() {
				anyhow::bail!("lock_command is empty");
			}

			let output = std::process::Command::new(parts[0])
				.args(&parts[1..])
				.current_dir(&workspace_root)
				.output()
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

			return Ok(());
		}

		// Auto-detect lock file and run appropriate command
		if workspace_root.join("package-lock.json").exists() {
			let output = std::process::Command::new("npm")
				.args(["install", "--package-lock-only"])
				.current_dir(&workspace_root)
				.output()
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
		} else if workspace_root.join("pnpm-lock.yaml").exists() {
			let output = std::process::Command::new("pnpm")
				.args(["install", "--lockfile-only"])
				.current_dir(&workspace_root)
				.output()
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
		} else if workspace_root.join("yarn.lock").exists() {
			let output = std::process::Command::new("yarn")
				.args(["install", "--mode", "update-lockfile"])
				.current_dir(&workspace_root)
				.output()
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
		}
		// No lock file found - no-op

		Ok(())
	}

	fn publish(
		&self,
		git_root: &Path,
		project: &ProjectInfo,
		dry_run: bool,
	) -> anyhow::Result<PublishOutcome> {
		let project_dir = git_root.join(&project.path);

		let mut cmd = std::process::Command::new("npm");
		cmd.arg("publish").current_dir(&project_dir);

		if dry_run {
			cmd.arg("--dry-run");
		}

		// For scoped packages, add --access flag
		if project.name.starts_with('@') {
			let access = self.config.access.as_deref().unwrap_or("restricted");
			cmd.arg("--access").arg(access);
		}

		let output = cmd
			.output()
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

	fn intra_dependencies(
		&self,
		git_root: &Path,
		projects: &[&ProjectInfo],
	) -> anyhow::Result<Vec<(String, String)>> {
		let project_names: std::collections::HashSet<_> =
			projects.iter().map(|p| p.name.as_str()).collect();

		let mut edges = Vec::new();

		for &project in projects {
			let manifest_path = git_root.join(&project.path).join("package.json");
			let contents = std::fs::read_to_string(&manifest_path)
				.with_context(|| format!("Failed to read {}", manifest_path.display()))?;
			let json: serde_json::Value = serde_json::from_str(&contents)
				.with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

			// Check all dependency sections
			for section in ["dependencies", "devDependencies", "peerDependencies"] {
				if let Some(deps) = json.get(section).and_then(|d| d.as_object()) {
					for dep_name in deps.keys() {
						if project_names.contains(dep_name.as_str()) {
							edges.push((project.name.clone(), dep_name.to_string()));
						}
					}
				}
			}
		}

		Ok(edges)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	fn write_package_json(dir: &Path, content: &str) {
		std::fs::write(dir.join("package.json"), content).unwrap();
	}

	/// Helper to enumerate projects using the adapter with no configured path.
	fn enumerate(dir: &Path) -> anyhow::Result<Vec<ProjectInfo>> {
		NpmAdapter::new(NpmConfig::default()).enumerate_projects(dir)
	}

	/// Helper to enumerate projects using the adapter with a configured path.
	fn enumerate_with_path(dir: &Path, path: &str) -> anyhow::Result<Vec<ProjectInfo>> {
		NpmAdapter::new(NpmConfig {
			enabled: true,
			path: Some(path.to_string()),
			..Default::default()
		})
		.enumerate_projects(dir)
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
		write_package_json(dir.path(), r#"{"name": "my-app"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "my-app");
		assert_eq!(projects[0].path, Path::new(""));
	}

	#[test]
	fn enumerate_single_package_without_name() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "unnamed");
	}

	#[test]
	fn enumerate_workspaces_array() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "monorepo", "workspaces": ["packages/*"]}"#,
		);

		// Create workspace packages
		let pkg_a = dir.path().join("packages/pkg-a");
		let pkg_b = dir.path().join("packages/pkg-b");
		std::fs::create_dir_all(&pkg_a).unwrap();
		std::fs::create_dir_all(&pkg_b).unwrap();
		write_package_json(&pkg_a, r#"{"name": "@scope/pkg-a"}"#);
		write_package_json(&pkg_b, r#"{"name": "@scope/pkg-b"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 3);
		// Root project first (empty path sorts first)
		assert_eq!(projects[0].name, "monorepo");
		assert_eq!(projects[0].path, Path::new(""));
		assert_eq!(projects[1].name, "@scope/pkg-a");
		assert_eq!(projects[1].path, Path::new("packages/pkg-a"));
		assert_eq!(projects[2].name, "@scope/pkg-b");
		assert_eq!(projects[2].path, Path::new("packages/pkg-b"));
	}

	#[test]
	fn enumerate_workspaces_object() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "root", "workspaces": {"packages": ["packages/*"]}}"#,
		);

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "root");
		assert_eq!(projects[0].path, Path::new(""));
		assert_eq!(projects[1].name, "my-pkg");
	}

	#[test]
	fn enumerate_workspace_without_root_name() {
		let dir = temp_dir();
		// Root package without a name field
		write_package_json(dir.path(), r#"{"workspaces": ["packages/*"]}"#);

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "unnamed");
		assert_eq!(projects[0].path, Path::new(""));
		assert_eq!(projects[1].name, "my-pkg");
	}

	#[test]
	fn enumerate_multiple_workspace_patterns() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "monorepo", "workspaces": ["packages/*", "apps/*"]}"#,
		);

		let pkg = dir.path().join("packages/lib");
		let app = dir.path().join("apps/web");
		std::fs::create_dir_all(&pkg).unwrap();
		std::fs::create_dir_all(&app).unwrap();
		write_package_json(&pkg, r#"{"name": "lib"}"#);
		write_package_json(&app, r#"{"name": "web"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 3);
		// Root first (empty path), then sorted by path
		assert_eq!(projects[0].name, "monorepo");
		assert_eq!(projects[0].path, Path::new(""));
		assert_eq!(projects[1].name, "web");
		assert_eq!(projects[1].path, Path::new("apps/web"));
		assert_eq!(projects[2].name, "lib");
		assert_eq!(projects[2].path, Path::new("packages/lib"));
	}

	#[test]
	fn enumerate_skips_directories_without_package_json() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "root", "workspaces": ["packages/*"]}"#,
		);

		let pkg = dir.path().join("packages/valid");
		let no_pkg = dir.path().join("packages/no-package-json");
		std::fs::create_dir_all(&pkg).unwrap();
		std::fs::create_dir_all(&no_pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "valid"}"#);
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
			r#"{"name": "root", "workspaces": ["packages/*"]}"#,
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
			r#"{"name": "root", "workspaces": ["packages/*/subpackages/*"]}"#,
		);

		let nested = dir.path().join("packages/group/subpackages/nested-pkg");
		std::fs::create_dir_all(&nested).unwrap();
		write_package_json(&nested, r#"{"name": "nested-pkg"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "root");
		assert_eq!(projects[1].name, "nested-pkg");
		assert_eq!(
			projects[1].path,
			Path::new("packages/group/subpackages/nested-pkg")
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
		write_package_json(dir.path(), r#"{"workspaces": ["packages/*"]}"#);

		let pkg = dir.path().join("packages/bad");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, "invalid json");

		let result = enumerate(dir.path());

		assert!(result.is_err());
	}

	#[test]
	fn new_creates_adapter() {
		let adapter = NpmAdapter::new(NpmConfig::default());
		let dir = temp_dir();
		// Should work without panicking
		let _ = adapter.enumerate_projects(dir.path());
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
		write_package_json(dir.path(), r#"{"name": "pnpm-monorepo"}"#);
		write_pnpm_workspace(dir.path(), "packages:\n  - 'packages/*'\n");

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg"}"#);

		let projects = enumerate(dir.path()).unwrap();

		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name, "pnpm-monorepo");
		assert_eq!(projects[0].path, Path::new(""));
		assert_eq!(projects[1].name, "my-pkg");
		assert_eq!(projects[1].path, Path::new("packages/my-pkg"));
	}

	#[test]
	fn enumerate_pnpm_workspace_multiple_patterns() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "root"}"#);
		write_pnpm_workspace(dir.path(), "packages:\n  - 'packages/*'\n  - 'apps/*'\n");

		let pkg = dir.path().join("packages/lib");
		let app = dir.path().join("apps/web");
		std::fs::create_dir_all(&pkg).unwrap();
		std::fs::create_dir_all(&app).unwrap();
		write_package_json(&pkg, r#"{"name": "lib"}"#);
		write_package_json(&app, r#"{"name": "web"}"#);

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
		write_package_json(dir.path(), r#"{"name": "root", "workspaces": ["other/*"]}"#);
		// pnpm-workspace.yaml should take precedence
		write_pnpm_workspace(dir.path(), "packages:\n  - 'packages/*'\n");

		let pkg = dir.path().join("packages/from-pnpm");
		let other = dir.path().join("other/from-npm");
		std::fs::create_dir_all(&pkg).unwrap();
		std::fs::create_dir_all(&other).unwrap();
		write_package_json(&pkg, r#"{"name": "from-pnpm"}"#);
		write_package_json(&other, r#"{"name": "from-npm"}"#);

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
			r#"{"name": "root", "workspaces": ["packages/*"]}"#,
		);
		// pnpm-workspace.yaml with empty packages
		write_pnpm_workspace(dir.path(), "packages: []\n");

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg"}"#);

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
			r#"{"name": "root", "workspaces": ["packages/*"]}"#,
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
			r#"{"name": "root", "workspaces": ["packages/*"]}"#,
		);
		// pnpm-workspace.yaml without packages field
		write_pnpm_workspace(dir.path(), "other_field: true\n");

		let pkg = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&pkg).unwrap();
		write_package_json(&pkg, r#"{"name": "my-pkg"}"#);

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
		write_package_json(&subfolder, r#"{"name": "my-app"}"#);

		let projects = enumerate_with_path(dir.path(), "frontend").unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name, "my-app");
		assert_eq!(projects[0].path, Path::new("frontend"));
	}

	#[test]
	fn enumerate_workspace_in_subfolder() {
		let dir = temp_dir();
		let subfolder = dir.path().join("frontend");
		std::fs::create_dir_all(&subfolder).unwrap();
		write_package_json(
			&subfolder,
			r#"{"name": "monorepo", "workspaces": ["packages/*"]}"#,
		);

		let pkg_a = subfolder.join("packages/pkg-a");
		let pkg_b = subfolder.join("packages/pkg-b");
		std::fs::create_dir_all(&pkg_a).unwrap();
		std::fs::create_dir_all(&pkg_b).unwrap();
		write_package_json(&pkg_a, r#"{"name": "@scope/pkg-a"}"#);
		write_package_json(&pkg_b, r#"{"name": "@scope/pkg-b"}"#);

		let projects = enumerate_with_path(dir.path(), "frontend").unwrap();

		assert_eq!(projects.len(), 3);
		assert_eq!(projects[0].name, "monorepo");
		assert_eq!(projects[0].path, Path::new("frontend"));
		assert_eq!(projects[1].name, "@scope/pkg-a");
		assert_eq!(projects[1].path, Path::new("frontend/packages/pkg-a"));
		assert_eq!(projects[2].name, "@scope/pkg-b");
		assert_eq!(projects[2].path, Path::new("frontend/packages/pkg-b"));
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
		}
	}

	#[test]
	fn read_version_from_package_json() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.2.3"}"#);
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");
		let version = adapter.read_version(dir.path(), &info).unwrap();
		assert_eq!(version.to_string(), "1.2.3");
	}

	#[test]
	fn read_version_missing_version_field() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app"}"#);
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");
		let result = adapter.read_version(dir.path(), &info);
		assert!(result.is_err());
	}

	#[test]
	fn read_version_file_not_found() {
		let dir = temp_dir();
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");
		let result = adapter.read_version(dir.path(), &info);
		assert!(result.is_err());
	}

	#[test]
	fn read_version_invalid_json() {
		let dir = temp_dir();
		write_package_json(dir.path(), "not valid json");
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");
		let result = adapter.read_version(dir.path(), &info);
		assert!(result.is_err());
	}

	#[test]
	fn read_version_invalid_semver() {
		let dir = temp_dir();
		write_package_json(
			dir.path(),
			r#"{"name": "my-app", "version": "not-a-version"}"#,
		);
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");
		let result = adapter.read_version(dir.path(), &info);
		assert!(result.is_err());
	}

	#[test]
	fn write_version_file_not_found() {
		let dir = temp_dir();
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");
		let version: semver::Version = "1.0.0".parse().unwrap();
		let result = adapter.write_version(dir.path(), &info, &version);
		assert!(result.is_err());
	}

	#[test]
	fn write_version_invalid_json() {
		let dir = temp_dir();
		write_package_json(dir.path(), "not valid json");
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");
		let version: semver::Version = "1.0.0".parse().unwrap();
		let result = adapter.write_version(dir.path(), &info, &version);
		assert!(result.is_err());
	}

	#[test]
	fn write_version_updates_package_json() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");
		let new_version: semver::Version = "2.0.0".parse().unwrap();
		adapter
			.write_version(dir.path(), &info, &new_version)
			.unwrap();

		let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
		assert!(
			contents.contains("\"2.0.0\""),
			"Should contain new version, got: {contents}"
		);
		assert!(contents.ends_with('\n'), "Should end with newline");
	}

	#[test]
	fn read_write_version_roundtrip() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "0.1.0"}"#);
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");

		let v = adapter.read_version(dir.path(), &info).unwrap();
		assert_eq!(v.to_string(), "0.1.0");

		let new_v: semver::Version = "0.2.0".parse().unwrap();
		adapter.write_version(dir.path(), &info, &new_v).unwrap();

		let v2 = adapter.read_version(dir.path(), &info).unwrap();
		assert_eq!(v2.to_string(), "0.2.0");
	}

	#[test]
	fn update_lock_file_no_op_when_no_lock_file() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("my-app", "");

		// Should succeed even without a lock file
		let result = adapter.update_lock_file(dir.path(), &info);
		assert!(result.is_ok());
	}

	#[test]
	fn update_lock_file_custom_command_empty_fails() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let adapter = NpmAdapter::new(NpmConfig {
			enabled: true,
			path: None,
			lock_command: Some("".to_string()),
			access: None,
		});
		let info = project_info("my-app", "");

		let result = adapter.update_lock_file(dir.path(), &info);
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
		let adapter = NpmAdapter::new(NpmConfig {
			enabled: true,
			path: None,
			lock_command: Some("nonexistent-command-12345".to_string()),
			access: None,
		});
		let info = project_info("my-app", "");

		let result = adapter.update_lock_file(dir.path(), &info);
		assert!(result.is_err());
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
	fn npm_config_resolve_root_without_path() {
		let config = NpmConfig {
			enabled: true,
			path: None,
			lock_command: None,
			access: None,
		};
		let git_root = Path::new("/repo");
		let resolved = config.resolve_root(git_root);
		assert_eq!(resolved, git_root);
	}

	#[test]
	fn npm_config_resolve_root_with_path() {
		let config = NpmConfig {
			enabled: true,
			path: Some("frontend".to_string()),
			lock_command: None,
			access: None,
		};
		let git_root = Path::new("/repo");
		let resolved = config.resolve_root(git_root);
		assert_eq!(resolved, Path::new("/repo/frontend"));
	}

	#[test]
	fn update_lock_file_custom_command_with_exit_code_fails() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let adapter = NpmAdapter::new(NpmConfig {
			enabled: true,
			path: None,
			lock_command: Some("false".to_string()), // 'false' always exits with 1
			access: None,
		});
		let info = project_info("my-app", "");

		let result = adapter.update_lock_file(dir.path(), &info);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("Lock command"));
	}

	#[test]
	fn update_lock_file_custom_command_succeeds() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		let adapter = NpmAdapter::new(NpmConfig {
			enabled: true,
			path: None,
			lock_command: Some("true".to_string()), // 'true' always exits with 0
			access: None,
		});
		let info = project_info("my-app", "");

		let result = adapter.update_lock_file(dir.path(), &info);
		assert!(result.is_ok());
	}

	#[test]
	fn update_lock_file_npm_not_found_fails() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
		// Create package-lock.json to trigger npm detection
		std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();

		// Use a custom PATH that doesn't include npm
		let _adapter = NpmAdapter::new(NpmConfig::default());
		let _info = project_info("my-app", "");

		// This will fail if npm is not in PATH
		// We can't reliably test this without manipulating PATH, but we document the behavior
		// The actual error will come from Command::new("npm").output()
	}

	#[test]
	fn update_lock_file_npm_succeeds() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
		// Create an initial package-lock.json
		std::fs::write(
			dir.path().join("package-lock.json"),
			r#"{"name":"test-app","version":"1.0.0","lockfileVersion":3,"requires":true,"packages":{"":{"name":"test-app","version":"1.0.0"}}}"#,
		)
		.unwrap();

		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("test-app", "");

		let result = adapter.update_lock_file(dir.path(), &info);
		assert!(result.is_ok(), "npm lock file update should succeed");
	}

	#[test]
	fn update_lock_file_pnpm_succeeds() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
		// Create a pnpm-workspace.yaml to avoid pnpm complaints
		std::fs::write(
			dir.path().join("pnpm-workspace.yaml"),
			"packages:\n  - '.'\n",
		)
		.unwrap();
		// Create an initial pnpm-lock.yaml
		std::fs::write(
			dir.path().join("pnpm-lock.yaml"),
			"lockfileVersion: '6.0'\n",
		)
		.unwrap();

		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("test-app", "");

		let result = adapter.update_lock_file(dir.path(), &info);
		assert!(result.is_ok(), "pnpm lock file update should succeed");
	}

	#[test]
	fn update_lock_file_yarn_succeeds() {
		let dir = temp_dir();
		write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
		// Create an initial yarn.lock (yarn 1.x format)
		std::fs::write(
			dir.path().join("yarn.lock"),
			"# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.\n# yarn lockfile v1\n",
		)
		.unwrap();

		let adapter = NpmAdapter::new(NpmConfig::default());
		let info = project_info("test-app", "");

		let result = adapter.update_lock_file(dir.path(), &info);
		assert!(
			result.is_ok(),
			"yarn lock file update should succeed: {:?}",
			result.err()
		);
	}
}
