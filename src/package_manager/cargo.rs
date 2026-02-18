//! Cargo package manager adapter.

use std::path::Path;

use anyhow::Context;
use glob::glob;
use serde::Deserialize;

use super::{PackageManagerAdapter, ProjectInfo};
use crate::config::PackageManagerConfig;

/// Adapter for Cargo-based Rust projects.
///
/// Supports both single-crate repositories and workspaces.
#[derive(Debug)]
pub struct CargoAdapter {
	/// Configuration for this package manager.
	config: PackageManagerConfig,
}

impl CargoAdapter {
	/// Creates a new Cargo adapter with the given configuration.
	pub fn new(config: PackageManagerConfig) -> Self {
		Self { config }
	}
}

/// Represents the relevant fields from Cargo.toml.
#[derive(Debug, Deserialize)]
struct CargoToml {
	package: Option<Package>,
	workspace: Option<Workspace>,
}

/// The [package] section of Cargo.toml.
#[derive(Debug, Deserialize)]
struct Package {
	name: String,
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

/// Attempts to create a ProjectInfo from a workspace member directory.
///
/// Returns `Ok(None)` if the path is not a valid crate (not a directory or no Cargo.toml).
fn read_workspace_member(
	git_root: &Path,
	member_path: &Path,
) -> anyhow::Result<Option<ProjectInfo>> {
	if !member_path.is_dir() {
		return Ok(None);
	}

	let Some(cargo) = read_cargo_toml(member_path)? else {
		return Ok(None);
	};

	let Some(package) = cargo.package else {
		// Virtual manifest (workspace-only Cargo.toml without [package])
		return Ok(None);
	};

	let path = member_path
		.strip_prefix(git_root)
		.context("Member path is not under git root")?
		.to_path_buf();

	Ok(Some(ProjectInfo {
		name: package.name,
		path,
	}))
}

/// Expands a workspace member glob pattern and returns all matching projects.
///
/// Globs are resolved relative to `pm_root`, but paths in the returned
/// [`ProjectInfo`] are stripped relative to `git_root`.
fn expand_member_pattern(
	git_root: &Path,
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
			read_workspace_member(git_root, &member_path)
		})
		.filter_map(Result::transpose)
		.collect()
}

impl PackageManagerAdapter for CargoAdapter {
	fn enumerate_projects(&self, git_root: &Path) -> anyhow::Result<Vec<ProjectInfo>> {
		let pm_root = self.config.resolve_root(git_root);
		let Some(root_cargo) = read_cargo_toml(&pm_root)? else {
			return Ok(Vec::new());
		};

		let pm_relative_path = pm_root
			.strip_prefix(git_root)
			.unwrap_or(Path::new(""))
			.to_path_buf();

		// Check for workspace members
		let workspace_members = root_cargo
			.workspace
			.as_ref()
			.and_then(|ws| ws.members.as_ref())
			.filter(|members| !members.is_empty());

		let Some(members) = workspace_members else {
			// Single crate repository
			let Some(package) = root_cargo.package else {
				// Virtual manifest with no members - nothing to enumerate
				return Ok(Vec::new());
			};
			return Ok(vec![ProjectInfo {
				name: package.name,
				path: pm_relative_path,
			}]);
		};

		// Workspace with members
		let mut projects: Vec<ProjectInfo> = members
			.iter()
			.map(|pattern| expand_member_pattern(git_root, &pm_root, pattern))
			.collect::<anyhow::Result<Vec<_>>>()?
			.into_iter()
			.flatten()
			.collect();

		// Include root package if it exists (some workspaces have a root crate too)
		if let Some(package) = root_cargo.package {
			projects.insert(
				0,
				ProjectInfo {
					name: package.name,
					path: pm_relative_path,
				},
			);
		}

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

	fn write_cargo_toml(dir: &Path, content: &str) {
		std::fs::write(dir.join("Cargo.toml"), content).unwrap();
	}

	/// Helper to enumerate projects using the adapter with no configured path.
	fn enumerate(dir: &Path) -> anyhow::Result<Vec<ProjectInfo>> {
		CargoAdapter::new(PackageManagerConfig::default()).enumerate_projects(dir)
	}

	/// Helper to enumerate projects using the adapter with a configured path.
	fn enumerate_with_path(dir: &Path, path: &str) -> anyhow::Result<Vec<ProjectInfo>> {
		CargoAdapter::new(PackageManagerConfig {
			enabled: true,
			path: Some(path.to_string()),
		})
		.enumerate_projects(dir)
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
		let adapter = CargoAdapter::new(PackageManagerConfig::default());
		let dir = temp_dir();
		let _ = adapter.enumerate_projects(dir.path());
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
}
