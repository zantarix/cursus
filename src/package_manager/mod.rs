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

use semver::{BuildMetadata, Prerelease, Version};

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
	/// The current version of the project.
	pub version: Version,
	/// Whether the project is publishable (not marked as private).
	pub publishable: bool,
	/// Names of intra-workspace dependencies.
	pub dependency_names: Vec<String>,
}

impl Default for ProjectInfo {
	fn default() -> Self {
		Self {
			name: String::new(),
			path: std::path::PathBuf::new(),
			version: Version {
				major: 0,
				minor: 0,
				patch: 0,
				pre: Prerelease::new("development").unwrap(),
				build: BuildMetadata::EMPTY,
			},
			publishable: true,
			dependency_names: Vec::new(),
		}
	}
}

/// Outcome of a package publish operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
	/// The package was successfully published.
	Published,
	/// The package version already exists in the registry (not an error).
	AlreadyPublished,
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

	/// Returns a reference to the project metadata.
	pub fn info(&self) -> &ProjectInfo {
		&self.info
	}

	/// Returns the current version of this project.
	///
	/// The version is cached from when the project was enumerated.
	pub fn version(&self) -> &Version {
		&self.info.version
	}

	/// Writes a new version to this project's manifest file.
	///
	/// Delegates to the underlying package manager adapter.
	pub fn write_version(&self, version: &Version) -> anyhow::Result<()> {
		self.adapter.write_version(&self.info, version)
	}

	/// Publishes this project to its package registry.
	///
	/// Delegates to the underlying package manager adapter.
	pub fn publish(&self) -> anyhow::Result<PublishOutcome> {
		self.adapter.publish(&self.info)
	}

	/// Returns the name of the registry this project would be published to.
	///
	/// Delegates to the underlying package manager adapter.
	pub fn registry_name(&self) -> &str {
		self.adapter.registry_name()
	}

	/// Returns whether this project is publishable (not marked as private).
	///
	/// The publishable status is cached from when the project was enumerated.
	pub fn is_publishable(&self) -> anyhow::Result<bool> {
		Ok(self.info.publishable)
	}

	/// Returns the names of intra-workspace dependencies for this project.
	///
	/// The dependency names are cached from when the project was enumerated.
	pub fn dependency_names(&self) -> &[String] {
		&self.info.dependency_names
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
	/// The returned `ProjectInfo` instances include:
	/// - `version`: The current version from the manifest
	/// - `publishable`: Whether the project is publishable (not private)
	/// - `dependency_names`: Names of intra-workspace dependencies
	///
	/// # Errors
	///
	/// Returns an error if project enumeration fails (e.g., invalid manifest files).
	fn enumerate_projects(&self) -> anyhow::Result<Vec<ProjectInfo>>;

	/// Writes a new version to a project's manifest file, preserving formatting.
	///
	/// # Arguments
	///
	/// * `project` - The project to update.
	/// * `version` - The new version to write.
	///
	/// # Errors
	///
	/// Returns an error if the manifest file cannot be read or written.
	fn write_version(&self, project: &ProjectInfo, version: &Version) -> anyhow::Result<()>;

	/// Updates the lock file after version changes.
	///
	/// This method should regenerate or update the lock file to reflect the new
	/// version information. The implementation may use a custom command from the
	/// configuration or fall back to package-manager-specific defaults.
	///
	/// This is a workspace-level operation and should be called once per adapter
	/// after all version writes are complete.
	///
	/// # Errors
	///
	/// Returns an error if the lock file update command fails.
	fn update_lock_file(&self) -> anyhow::Result<()>;

	/// Publishes a project to its package registry.
	///
	/// If the package version already exists in the registry, this should return
	/// `Ok(PublishOutcome::AlreadyPublished)` rather than an error.
	///
	/// # Arguments
	///
	/// * `project` - The project to publish.
	///
	/// # Errors
	///
	/// Returns an error if the publish operation fails for reasons other than
	/// the package already existing.
	fn publish(&self, project: &ProjectInfo) -> anyhow::Result<PublishOutcome>;

	/// Returns the name of the registry this adapter publishes to.
	///
	/// Used for display purposes in CLI output (e.g., "crates.io", "npm").
	fn registry_name(&self) -> &str;
}

/// A directed graph for dependency ordering.
///
/// Stores an adjacency list where each node maps to its dependencies,
/// along with in-degree counts for efficient topological sorting.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
	/// Adjacency list: dependent -> [dependencies]
	adjacency: std::collections::HashMap<String, Vec<String>>,
	/// How many dependents point to each node
	in_degree: std::collections::HashMap<String, usize>,
}

impl DependencyGraph {
	/// Creates a new dependency graph from an adjacency list.
	///
	/// The adjacency list maps each node to its list of dependencies.
	/// In-degree counts are computed automatically from the adjacency list.
	///
	/// Only nodes that appear as keys in the adjacency list are part of the graph.
	/// Dependencies that don't appear as keys are considered external and are tracked
	/// in the in-degree map but not added to the adjacency list.
	///
	/// # Arguments
	///
	/// * `adjacency` - Adjacency list mapping `dependent -> [dependencies]`.
	pub fn from_adjacency(adjacency: std::collections::HashMap<String, Vec<String>>) -> Self {
		// Compute in-degree for each node
		let mut in_degree: std::collections::HashMap<String, usize> =
			adjacency.keys().map(|k| (k.clone(), 0)).collect();

		// Count in-degrees for all dependencies
		for dependencies in adjacency.values() {
			for dep in dependencies {
				*in_degree.entry(dep.clone()).or_insert(0) += 1;
			}
		}

		Self {
			adjacency,
			in_degree,
		}
	}

	/// Topologically sorts all nodes in the graph with roots (dependents) first.
	///
	/// This ordering ensures that dependents are processed before their dependencies,
	/// which might be useful for operations like uninstalling (remove dependents
	/// before dependencies).
	///
	/// # Returns
	///
	/// A topologically sorted list where dependents appear before dependencies.
	///
	/// # Errors
	///
	/// Returns an error if the graph contains cycles.
	pub fn sort_roots_first(&self) -> anyhow::Result<Vec<String>> {
		// Kahn's algorithm for topological sort
		// Clone in_degree since Kahn's algorithm is destructive
		let mut in_degree = self.in_degree.clone();

		// Start with nodes that have no incoming edges (roots)
		let mut queue: Vec<String> = in_degree
			.iter()
			.filter(|(_, deg)| **deg == 0)
			.map(|(n, _)| n.clone())
			.collect();

		let mut result = Vec::new();

		while let Some(node) = queue.pop() {
			result.push(node.clone());

			// For each dependency of this node, reduce its in-degree
			if let Some(dependencies) = self.adjacency.get(&node) {
				for dep in dependencies {
					if let Some(degree) = in_degree.get_mut(dep) {
						*degree -= 1;
						if *degree == 0 {
							queue.push(dep.clone());
						}
					}
				}
			}
		}

		// Check for cycles
		if result.len() != self.adjacency.len() {
			anyhow::bail!("Dependency graph contains a cycle");
		}

		Ok(result)
	}

	/// Topologically sorts all nodes in the graph with leaves (dependencies) first.
	///
	/// This ordering ensures that dependencies are processed before their dependents,
	/// which is appropriate for operations like publishing packages (publish
	/// dependencies before dependents).
	///
	/// This is the exact reverse of `sort_roots_first`.
	///
	/// # Returns
	///
	/// A topologically sorted list where dependencies appear before dependents.
	///
	/// # Errors
	///
	/// Returns an error if the graph contains cycles.
	pub fn sort_leaves_first(&self) -> anyhow::Result<Vec<String>> {
		let mut result = self.sort_roots_first()?;
		result.reverse();
		Ok(result)
	}
}

/// Filters a project list by package names, validating that all names exist.
///
/// If `package_names` is empty, returns all projects unchanged.
/// Otherwise, returns only projects whose names match the given list.
///
/// # Errors
///
/// Returns an error if any name in `package_names` does not match a known project.
pub fn filter_projects_by_name(
	projects: &[Project],
	package_names: &[String],
) -> anyhow::Result<Vec<Project>> {
	if package_names.is_empty() {
		return Ok(projects.to_vec());
	}

	package_names
		.iter()
		.map(|name| {
			projects
				.iter()
				.find(|p| p.name() == name)
				.cloned()
				.ok_or_else(|| anyhow::anyhow!("Unknown package: {name}"))
		})
		.collect()
}

/// Enumerates projects from multiple adapters and returns a flattened list.
///
/// This is the primary way to get [`Project`] instances. The returned projects
/// maintain a reference to the adapter that discovered them for further interaction.
///
/// # Arguments
///
/// * `adapters` - The package manager adapters to use.
/// * `git_workdir` - The root directory of the git repository.
///
/// # Errors
///
/// Returns an error if any adapter fails to enumerate its projects.
pub fn enumerate_projects(
	adapters: impl IntoIterator<Item = Arc<dyn PackageManagerAdapter>>,
) -> anyhow::Result<Vec<Project>> {
	adapters
		.into_iter()
		.map(|adapter| {
			adapter.enumerate_projects().map(|infos| {
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

/// Builds a dependency graph for the given projects.
///
/// This function uses cached dependency names from each project to construct
/// the dependency graph. The adjacency list maps each project to its dependencies.
///
/// # Arguments
///
/// * `projects` - All projects in the workspace to analyze.
///
/// # Errors
///
/// Returns an error if dependency analysis fails (e.g., manifest files cannot be read).
pub fn build_dependency_graph(projects: &[Project]) -> anyhow::Result<DependencyGraph> {
	let project_names: std::collections::HashSet<_> = projects.iter().map(|p| p.name()).collect();

	let mut adjacency = std::collections::HashMap::new();
	for project in projects {
		let mut dependencies = Vec::new();
		for dep_name in project.dependency_names() {
			// Only include intra-workspace dependencies
			if project_names.contains(dep_name.as_str()) {
				dependencies.push(dep_name.clone());
			}
		}
		adjacency.insert(project.name().to_string(), dependencies);
	}

	Ok(DependencyGraph::from_adjacency(adjacency))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Creates a test project with a dummy adapter.
	fn test_project(name: &str, path: &str) -> Project {
		let adapter: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			std::path::PathBuf::from("."),
		));
		Project {
			info: ProjectInfo {
				name: name.to_string(),
				path: std::path::PathBuf::from(path),
				..Default::default()
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
		std::fs::write(
			dir.path().join("package.json"),
			r#"{"name": "test", "version": "0.1.0"}"#,
		)
		.unwrap();

		let adapter: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			dir.path().to_path_buf(),
		));
		let projects = enumerate_projects([adapter.clone()]).unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name(), "test");
		// Verify the adapter is attached (Arc strong count > 1)
		assert!(Arc::strong_count(&projects[0].adapter) >= 2);
	}

	#[test]
	fn enumerate_projects_flattens_multiple_adapters() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("package.json"),
			r#"{"name": "npm-pkg", "version": "0.1.0"}"#,
		)
		.unwrap();

		// Two adapters pointing at the same directory (both will find the package)
		let adapter1: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			dir.path().to_path_buf(),
		));
		let adapter2: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			dir.path().to_path_buf(),
		));

		let projects = enumerate_projects([adapter1, adapter2]).unwrap();

		// Both adapters find the same package, so we get 2 projects
		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name(), "npm-pkg");
		assert_eq!(projects[1].name(), "npm-pkg");
	}

	#[test]
	fn enumerate_projects_empty_adapters_returns_empty() {
		let _dir = tempfile::tempdir().unwrap();
		let adapters: [Arc<dyn PackageManagerAdapter>; 0] = [];
		let projects = enumerate_projects(adapters).unwrap();
		assert!(projects.is_empty());
	}

	#[test]
	fn filter_projects_empty_names_returns_all() {
		let projects = vec![
			test_project("a", "packages/a"),
			test_project("b", "packages/b"),
		];
		let result = filter_projects_by_name(&projects, &[]).unwrap();
		assert_eq!(result.len(), 2);
	}

	#[test]
	fn filter_projects_selects_matching() {
		let projects = vec![
			test_project("a", "packages/a"),
			test_project("b", "packages/b"),
			test_project("c", "packages/c"),
		];
		let names = vec!["b".to_string(), "c".to_string()];
		let result = filter_projects_by_name(&projects, &names).unwrap();
		assert_eq!(result.len(), 2);
		assert_eq!(result[0].name(), "b");
		assert_eq!(result[1].name(), "c");
	}

	#[test]
	fn filter_projects_unknown_name_returns_error() {
		let projects = vec![test_project("a", "packages/a")];
		let names = vec!["nonexistent".to_string()];
		let result = filter_projects_by_name(&projects, &names);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("Unknown package: nonexistent")
		);
	}

	#[test]
	fn dependency_graph_empty() {
		let graph = DependencyGraph::from_adjacency(std::collections::HashMap::new());
		let sorted = graph.sort_leaves_first().unwrap();
		assert!(sorted.is_empty());
	}

	#[test]
	fn dependency_graph_single_node() {
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec![]);
		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();
		assert_eq!(sorted, vec!["a"]);
	}

	#[test]
	fn dependency_graph_linear_chain() {
		// a -> b -> c (a depends on b, b depends on c)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		let leaves_first = graph.sort_leaves_first().unwrap();
		// c is the leaf (no dependencies), then b, then a
		assert_eq!(leaves_first, vec!["c", "b", "a"]);

		let roots_first = graph.sort_roots_first().unwrap();
		// a is the root (no dependents), then b, then c
		assert_eq!(roots_first, vec!["a", "b", "c"]);
	}

	#[test]
	fn dependency_graph_diamond() {
		// a -> b, a -> c, b -> d, c -> d (diamond: a depends on b and c, both depend on d)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string(), "c".to_string()]);
		adjacency.insert("b".to_string(), vec!["d".to_string()]);
		adjacency.insert("c".to_string(), vec!["d".to_string()]);
		adjacency.insert("d".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		let leaves_first = graph.sort_leaves_first().unwrap();
		// d must come first, then b and c (in either order), then a
		assert_eq!(leaves_first[0], "d");
		assert_eq!(leaves_first[3], "a");
		assert!(leaves_first[1..3].contains(&"b".to_string()));
		assert!(leaves_first[1..3].contains(&"c".to_string()));

		let roots_first = graph.sort_roots_first().unwrap();
		// a must come first, then b and c (in either order), then d
		assert_eq!(roots_first[0], "a");
		assert_eq!(roots_first[3], "d");
		assert!(roots_first[1..3].contains(&"b".to_string()));
		assert!(roots_first[1..3].contains(&"c".to_string()));
	}

	#[test]
	fn dependency_graph_simple_dependency() {
		// a -> b (a depends on b)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		let leaves_first = graph.sort_leaves_first().unwrap();
		// b (leaf) must come before a (dependent)
		assert_eq!(leaves_first, vec!["b", "a"]);

		let roots_first = graph.sort_roots_first().unwrap();
		// Exact reverse of leaves_first
		assert_eq!(roots_first, vec!["a", "b"]);
	}

	#[test]
	fn dependency_graph_cycle_returns_error() {
		// a -> b -> c -> a (cycle)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec!["a".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let result = graph.sort_leaves_first();
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("Dependency graph contains a cycle")
		);
	}

	#[test]
	fn dependency_graph_multiple_roots() {
		// a -> c, b -> c (two roots, one leaf)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["c".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		let leaves_first = graph.sort_leaves_first().unwrap();
		// c first, then a and b in either order
		assert_eq!(leaves_first[0], "c");
		assert!(leaves_first[1..3].contains(&"a".to_string()));
		assert!(leaves_first[1..3].contains(&"b".to_string()));

		let roots_first = graph.sort_roots_first().unwrap();
		// a and b first (in either order), then c
		assert_eq!(roots_first[2], "c");
		assert!(roots_first[0..2].contains(&"a".to_string()));
		assert!(roots_first[0..2].contains(&"b".to_string()));
	}

	#[test]
	fn dependency_graph_disconnected_subgraphs() {
		// Two independent chains: a -> b, c -> d
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec![]);
		adjacency.insert("c".to_string(), vec!["d".to_string()]);
		adjacency.insert("d".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		let leaves_first = graph.sort_leaves_first().unwrap();
		// b must come before a, d must come before c
		let b_index = leaves_first.iter().position(|n| n == "b").unwrap();
		let a_index = leaves_first.iter().position(|n| n == "a").unwrap();
		let d_index = leaves_first.iter().position(|n| n == "d").unwrap();
		let c_index = leaves_first.iter().position(|n| n == "c").unwrap();
		assert!(b_index < a_index, "b should come before a");
		assert!(d_index < c_index, "d should come before c");

		let roots_first = graph.sort_roots_first().unwrap();
		// Exact reverse: a before b, c before d
		let a_index = roots_first.iter().position(|n| n == "a").unwrap();
		let b_index = roots_first.iter().position(|n| n == "b").unwrap();
		let c_index = roots_first.iter().position(|n| n == "c").unwrap();
		let d_index = roots_first.iter().position(|n| n == "d").unwrap();
		assert!(a_index < b_index, "a should come before b");
		assert!(c_index < d_index, "c should come before d");

		// Verify it's truly the exact reverse
		let mut reversed_leaves = leaves_first.clone();
		reversed_leaves.reverse();
		assert_eq!(roots_first, reversed_leaves);
	}

	#[test]
	fn build_dependency_graph_empty_for_single_package() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("package.json"),
			r#"{"name": "test-package", "version": "0.1.0"}"#,
		)
		.unwrap();

		let adapter: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			dir.path().to_path_buf(),
		));
		let projects = enumerate_projects([adapter]).unwrap();
		let graph = build_dependency_graph(&projects).unwrap();

		// Single package with no dependencies should result in trivial sorting
		let sorted = graph.sort_leaves_first().unwrap();
		assert_eq!(sorted, vec!["test-package"]);
	}

	#[test]
	fn build_dependency_graph_with_workspace_dependencies() {
		let dir = tempfile::tempdir().unwrap();

		// Create workspace with dependencies
		std::fs::write(
			dir.path().join("package.json"),
			r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
		)
		.unwrap();

		std::fs::create_dir_all(dir.path().join("packages/lib")).unwrap();
		std::fs::write(
			dir.path().join("packages/lib/package.json"),
			r#"{"name": "lib", "version": "0.1.0"}"#,
		)
		.unwrap();

		std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
		std::fs::write(
			dir.path().join("packages/app/package.json"),
			r#"{"name": "app", "version": "0.1.0", "dependencies": {"lib": "0.1.0"}}"#,
		)
		.unwrap();

		let adapter: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			dir.path().to_path_buf(),
		));
		let projects = enumerate_projects([adapter]).unwrap();
		let graph = build_dependency_graph(&projects).unwrap();

		// app depends on lib, so lib should come before app
		let sorted = graph.sort_leaves_first().unwrap();

		// lib should be published before app (lib is a dependency of app)
		let lib_index = sorted.iter().position(|n| n == "lib").unwrap();
		let app_index = sorted.iter().position(|n| n == "app").unwrap();
		assert!(lib_index < app_index, "lib should come before app");
	}

	#[test]
	fn build_dependency_graph_excludes_external_dependencies() {
		let dir = tempfile::tempdir().unwrap();

		// Create workspace where app depends on both workspace lib and external react
		std::fs::write(
			dir.path().join("package.json"),
			r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
		)
		.unwrap();

		std::fs::create_dir_all(dir.path().join("packages/lib")).unwrap();
		std::fs::write(
			dir.path().join("packages/lib/package.json"),
			r#"{"name": "lib", "version": "0.1.0"}"#,
		)
		.unwrap();

		std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
		std::fs::write(
			dir.path().join("packages/app/package.json"),
			r#"{"name": "app", "version": "0.1.0", "dependencies": {"lib": "0.1.0", "react": "^18.0.0"}}"#,
		)
		.unwrap();

		let adapter: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			dir.path().to_path_buf(),
		));
		let projects = enumerate_projects([adapter]).unwrap();
		let graph = build_dependency_graph(&projects).unwrap();

		// Verify that app's adjacency list only includes lib, not react
		assert_eq!(
			graph.adjacency.get("app").unwrap(),
			&vec!["lib".to_string()]
		);

		// Verify react is not in the graph at all
		assert!(!graph.adjacency.contains_key("react"));
		assert!(!graph.in_degree.contains_key("react"));

		// Verify topological sort still works correctly
		let sorted = graph.sort_leaves_first().unwrap();

		// react should not appear in the sorted output
		assert!(!sorted.contains(&"react".to_string()));

		// lib should come before app
		let lib_index = sorted.iter().position(|n| n == "lib").unwrap();
		let app_index = sorted.iter().position(|n| n == "app").unwrap();
		assert!(lib_index < app_index, "lib should come before app");
	}
}
