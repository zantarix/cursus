//! npm package manager adapter.

use std::path::Path;

use anyhow::Context;
use glob::glob;
use serde::Deserialize;

use super::{PackageManagerAdapter, ProjectInfo};
use crate::config::PackageManagerConfig;

/// Adapter for npm-based projects.
///
/// Supports both single-package repositories and monorepos using npm/yarn/pnpm workspaces.
#[derive(Debug)]
pub struct NpmAdapter {
	/// Configuration for this package manager.
	#[allow(dead_code)]
	config: PackageManagerConfig,
}

impl NpmAdapter {
	/// Creates a new npm adapter with the given configuration.
	pub fn new(config: PackageManagerConfig) -> Self {
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
	let workspace: PnpmWorkspace = serde_yaml_ng::from_str(&contents)
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
fn expand_workspace_pattern(git_root: &Path, pattern: &str) -> anyhow::Result<Vec<ProjectInfo>> {
	let full_pattern = git_root.join(pattern);
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
	fn enumerate_projects(&self, git_root: &Path) -> anyhow::Result<Vec<ProjectInfo>> {
		let Some(root_package) = read_package_json(git_root)? else {
			return Ok(Vec::new());
		};
		let pnpm_workspace = read_pnpm_workspace(git_root)?;

		let Some(workspace_patterns) =
			get_workspace_patterns(pnpm_workspace.as_ref(), &root_package)
		else {
			// Single package repository
			let name = root_package.name.unwrap_or_else(|| "unnamed".to_string());
			return Ok(vec![ProjectInfo {
				name,
				path: std::path::PathBuf::new(),
			}]);
		};

		// Monorepo with workspaces - include root project first
		let root_name = root_package.name.unwrap_or_else(|| "unnamed".to_string());
		let root_project = ProjectInfo {
			name: root_name,
			path: std::path::PathBuf::new(),
		};

		let mut projects: Vec<ProjectInfo> = std::iter::once(root_project)
			.chain(
				workspace_patterns
					.iter()
					.map(|pattern| expand_workspace_pattern(git_root, pattern))
					.collect::<anyhow::Result<Vec<_>>>()?
					.into_iter()
					.flatten(),
			)
			.collect();

		// Sort by path for consistent ordering
		projects.sort_by(|a, b| a.path.cmp(&b.path));

		Ok(projects)
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

	/// Helper to enumerate projects using the adapter.
	fn enumerate(dir: &Path) -> anyhow::Result<Vec<ProjectInfo>> {
		NpmAdapter::new(PackageManagerConfig::default()).enumerate_projects(dir)
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
		let adapter = NpmAdapter::new(PackageManagerConfig::default());
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
}
