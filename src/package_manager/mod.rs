//! Package manager adapters for project enumeration and management.
//!
//! This module provides a trait-based abstraction over different package managers,
//! allowing Chronicle to work with various ecosystems (npm, Cargo, etc.) through
//! a unified interface.

mod cargo;
mod npm;

pub use cargo::{CargoAdapter, CargoConfig};
pub use npm::{NpmAdapter, NpmConfig};

use std::path::Path;
use std::sync::Arc;

use semver::Version;

/// Raw project data returned by package manager adapters.
///
/// This intermediate type contains only the project metadata without a reference
/// to the adapter. Use [`enumerate_projects`] to get full [`Project`] instances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
	/// The name of the project (e.g., package name).
	pub name: String,
	/// The path to the project root, relative to the git root.
	pub path: std::path::PathBuf,
}

/// Represents a project discovered by a package manager adapter.
///
/// Each project maintains a reference to the package manager that discovered it,
/// allowing further interaction through methods implemented on this type.
pub struct Project {
	/// The project metadata.
	info: ProjectInfo,
	/// Reference to the package manager that discovered this project.
	adapter: Arc<dyn PackageManagerAdapter>,
}

impl std::fmt::Debug for Project {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Project")
			.field("name", &self.info.name)
			.field("path", &self.info.path)
			.finish_non_exhaustive()
	}
}

impl Clone for Project {
	fn clone(&self) -> Self {
		Self {
			info: self.info.clone(),
			adapter: Arc::clone(&self.adapter),
		}
	}
}

impl PartialEq for Project {
	fn eq(&self, other: &Self) -> bool {
		self.info == other.info
	}
}

impl Eq for Project {}

impl Project {
	/// Returns the name of the project (e.g., package name).
	pub fn name(&self) -> &str {
		&self.info.name
	}

	/// Returns the path to the project root, relative to the git root.
	pub fn path(&self) -> &Path {
		&self.info.path
	}

	/// Reads the current version of this project from its manifest file.
	///
	/// Delegates to the underlying package manager adapter.
	pub fn read_version(&self, git_root: &Path) -> anyhow::Result<Version> {
		self.adapter.read_version(git_root, &self.info)
	}

	/// Writes a new version to this project's manifest file.
	///
	/// Delegates to the underlying package manager adapter.
	pub fn write_version(&self, git_root: &Path, version: &Version) -> anyhow::Result<()> {
		self.adapter.write_version(git_root, &self.info, version)
	}

	/// Updates the lock file for this project after a version change.
	///
	/// Delegates to the underlying package manager adapter.
	pub fn update_lock_file(&self, git_root: &Path) -> anyhow::Result<()> {
		self.adapter.update_lock_file(git_root, &self.info)
	}
}

/// Trait for package manager adapters.
///
/// Implementations of this trait provide package-manager-specific functionality
/// for discovering and managing projects within a repository.
pub trait PackageManagerAdapter: Send + Sync + std::fmt::Debug {
	/// Enumerates all projects managed by this package manager.
	///
	/// For single-package repositories, this returns a single project.
	/// For monorepos, this returns all workspace packages.
	///
	/// # Arguments
	///
	/// * `git_root` - The root directory of the git repository.
	///
	/// # Errors
	///
	/// Returns an error if project enumeration fails (e.g., invalid manifest files).
	fn enumerate_projects(&self, git_root: &Path) -> anyhow::Result<Vec<ProjectInfo>>;

	/// Reads the current version of a project from its manifest file.
	///
	/// # Arguments
	///
	/// * `git_root` - The root directory of the git repository.
	/// * `project` - The project to read the version for.
	///
	/// # Errors
	///
	/// Returns an error if the manifest file cannot be read or the version cannot be parsed.
	fn read_version(&self, git_root: &Path, project: &ProjectInfo) -> anyhow::Result<Version>;

	/// Writes a new version to a project's manifest file, preserving formatting.
	///
	/// # Arguments
	///
	/// * `git_root` - The root directory of the git repository.
	/// * `project` - The project to update.
	/// * `version` - The new version to write.
	///
	/// # Errors
	///
	/// Returns an error if the manifest file cannot be read or written.
	fn write_version(
		&self,
		git_root: &Path,
		project: &ProjectInfo,
		version: &Version,
	) -> anyhow::Result<()>;

	/// Updates the lock file for a project after a version change.
	///
	/// This method should regenerate or update the lock file to reflect the new
	/// version information. The implementation may use a custom command from the
	/// configuration or fall back to package-manager-specific defaults.
	///
	/// # Arguments
	///
	/// * `git_root` - The root directory of the git repository.
	/// * `project` - The project whose lock file should be updated.
	///
	/// # Errors
	///
	/// Returns an error if the lock file update command fails.
	fn update_lock_file(&self, git_root: &Path, project: &ProjectInfo) -> anyhow::Result<()>;
}

/// Enumerates projects from multiple adapters and returns a flattened list.
///
/// This is the primary way to get [`Project`] instances. The returned projects
/// maintain a reference to the adapter that discovered them for further interaction.
///
/// # Arguments
///
/// * `adapters` - The package manager adapters to use.
/// * `git_root` - The root directory of the git repository.
///
/// # Errors
///
/// Returns an error if any adapter fails to enumerate its projects.
pub fn enumerate_projects(
	adapters: impl IntoIterator<Item = Arc<dyn PackageManagerAdapter>>,
	git_root: &Path,
) -> anyhow::Result<Vec<Project>> {
	adapters
		.into_iter()
		.map(|adapter| {
			adapter.enumerate_projects(git_root).map(|infos| {
				infos
					.into_iter()
					.map(|info| Project {
						info,
						adapter: Arc::clone(&adapter),
					})
					.collect::<Vec<_>>()
			})
		})
		.collect::<anyhow::Result<Vec<_>>>()
		.map(|nested| nested.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Creates a test project with a dummy adapter.
	fn test_project(name: &str, path: &str) -> Project {
		let adapter: Arc<dyn PackageManagerAdapter> =
			Arc::new(NpmAdapter::new(NpmConfig::default()));
		Project {
			info: ProjectInfo {
				name: name.to_string(),
				path: std::path::PathBuf::from(path),
			},
			adapter,
		}
	}

	#[test]
	fn project_equality() {
		let p1 = test_project("test", "packages/test");
		let p2 = test_project("test", "packages/test");
		let p3 = test_project("other", "packages/other");

		assert_eq!(p1, p2);
		assert_ne!(p1, p3);
	}

	#[test]
	fn project_debug() {
		let project = test_project("my-package", "packages/my-package");
		let debug = format!("{:?}", project);
		assert!(debug.contains("my-package"));
	}

	#[test]
	fn project_clone() {
		let project = test_project("test", "src");
		let cloned = project.clone();
		assert_eq!(project, cloned);
	}

	#[test]
	fn project_getters() {
		let project = test_project("my-pkg", "packages/my-pkg");
		assert_eq!(project.name(), "my-pkg");
		assert_eq!(project.path(), Path::new("packages/my-pkg"));
	}

	#[test]
	fn enumerate_projects_attaches_adapter() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();

		let adapter: Arc<dyn PackageManagerAdapter> =
			Arc::new(NpmAdapter::new(NpmConfig::default()));
		let projects = enumerate_projects([adapter.clone()], dir.path()).unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name(), "test");
		// Verify the adapter is attached (Arc strong count > 1)
		assert!(Arc::strong_count(&projects[0].adapter) >= 2);
	}

	#[test]
	fn enumerate_projects_flattens_multiple_adapters() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("package.json"), r#"{"name": "npm-pkg"}"#).unwrap();

		// Two adapters pointing at the same directory (both will find the package)
		let adapter1: Arc<dyn PackageManagerAdapter> =
			Arc::new(NpmAdapter::new(NpmConfig::default()));
		let adapter2: Arc<dyn PackageManagerAdapter> =
			Arc::new(NpmAdapter::new(NpmConfig::default()));

		let projects = enumerate_projects([adapter1, adapter2], dir.path()).unwrap();

		// Both adapters find the same package, so we get 2 projects
		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name(), "npm-pkg");
		assert_eq!(projects[1].name(), "npm-pkg");
	}

	#[test]
	fn enumerate_projects_empty_adapters_returns_empty() {
		let dir = tempfile::tempdir().unwrap();
		let adapters: [Arc<dyn PackageManagerAdapter>; 0] = [];
		let projects = enumerate_projects(adapters, dir.path()).unwrap();
		assert!(projects.is_empty());
	}
}
