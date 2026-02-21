//! Package manager adapters for project enumeration and management.
//!
//! This module provides a trait-based abstraction over different package managers,
//! allowing Chronicle to work with various ecosystems (npm, Cargo, etc.) through
//! a unified interface.

use anyhow::Context;

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

	/// Reads the current version of this project from its manifest file.
	///
	/// Delegates to the underlying package manager adapter.
	pub fn read_version(&self) -> anyhow::Result<Version> {
		self.adapter.read_version(&self.info)
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
	/// Delegates to the underlying package manager adapter.
	pub fn is_publishable(&self) -> anyhow::Result<bool> {
		self.adapter.is_publishable(&self.info)
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
	/// # Errors
	///
	/// Returns an error if project enumeration fails (e.g., invalid manifest files).
	fn enumerate_projects(&self) -> anyhow::Result<Vec<ProjectInfo>>;

	/// Reads the current version of a project from its manifest file.
	///
	/// # Arguments
	///
	/// * `project` - The project to read the version for.
	///
	/// # Errors
	///
	/// Returns an error if the manifest file cannot be read or the version cannot be parsed.
	fn read_version(&self, project: &ProjectInfo) -> anyhow::Result<Version>;

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

	/// Returns whether this project is publishable (not marked as private).
	///
	/// Checks package-manager-specific markers:
	/// - npm: `"private": true` in package.json
	/// - Cargo: `publish = false` or `publish = []` in Cargo.toml
	///
	/// # Arguments
	///
	/// * `_project` - The project to check.
	///
	/// # Errors
	///
	/// Returns an error if the manifest file cannot be read or parsed.
	fn is_publishable(&self, _project: &ProjectInfo) -> anyhow::Result<bool> {
		Ok(true)
	}

	/// Returns intra-workspace dependency edges between projects.
	///
	/// This method analyzes the dependency declarations in all projects to find
	/// which workspace projects depend on other workspace projects. The returned
	/// edges are in the form `(dependent, dependency)`, meaning the first project
	/// depends on the second.
	///
	/// # Arguments
	///
	/// * `projects` - All projects in the workspace to analyze.
	///
	/// # Errors
	///
	/// Returns an error if manifest files cannot be read or parsed.
	fn intra_dependencies(
		&self,
		projects: &[&ProjectInfo],
	) -> anyhow::Result<Vec<(String, String)>>;
}

/// A directed graph for dependency ordering.
///
/// Stores edges in the form `(dependent, dependency)` meaning the first node
/// depends on the second.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
	edges: Vec<(String, String)>,
}

impl DependencyGraph {
	/// Creates a new dependency graph from a list of edges.
	///
	/// # Arguments
	///
	/// * `edges` - Dependency edges in the form `(dependent, dependency)`.
	pub fn from_edges(edges: Vec<(String, String)>) -> Self {
		Self { edges }
	}

	/// Returns all unique nodes in the graph.
	fn all_nodes(&self) -> std::collections::HashSet<String> {
		self.edges
			.iter()
			.flat_map(|(a, b)| [a.clone(), b.clone()])
			.collect()
	}

	/// Topologically sorts nodes with leaves (dependencies) first.
	///
	/// This ordering ensures that dependencies are processed before their dependents,
	/// which is appropriate for operations like publishing packages (publish
	/// dependencies before dependents).
	///
	/// Nodes not in the graph are included at the beginning of the result.
	///
	/// # Arguments
	///
	/// * `names` - The set of names to sort. Names not in the graph edges are
	///   included at the start of the result.
	///
	/// # Returns
	///
	/// A topologically sorted list where dependencies appear before dependents.
	///
	/// # Errors
	///
	/// Returns an error if the graph contains cycles or has inconsistent edges.
	pub fn sort_leaves_first(&self, names: &[String]) -> anyhow::Result<Vec<String>> {
		// Kahn's algorithm for topological sort
		let nodes_in_graph = self.all_nodes();
		let mut in_degree: std::collections::HashMap<String, usize> =
			nodes_in_graph.iter().map(|n| (n.clone(), 0)).collect();

		// Calculate in-degrees (how many things depend on each node)
		for (_, dependency) in &self.edges {
			*in_degree
				.get_mut(dependency)
				.with_context(|| format!("Edge references unknown node: {dependency}"))? += 1;
		}

		// Build adjacency list (dependent -> [dependencies])
		let mut adj: std::collections::HashMap<String, Vec<String>> =
			nodes_in_graph.iter().map(|n| (n.clone(), vec![])).collect();
		for (dependent, dependency) in &self.edges {
			adj.get_mut(dependent)
				.with_context(|| format!("Edge references unknown node: {dependent}"))?
				.push(dependency.clone());
		}

		// Start with nodes that have no incoming edges (dependents with no dependencies in this graph)
		let mut queue: Vec<String> = in_degree
			.iter()
			.filter(|(_, deg)| **deg == 0)
			.map(|(n, _)| n.clone())
			.collect();

		let mut result = Vec::new();

		while let Some(node) = queue.pop() {
			result.push(node.clone());

			// For each dependency of this node, reduce its in-degree
			if let Some(dependencies) = adj.get(&node) {
				for dep in dependencies {
					let degree = in_degree
						.get_mut(dep)
						.with_context(|| format!("Edge references unknown node: {dep}"))?;
					*degree -= 1;
					if *degree == 0 {
						queue.push(dep.clone());
					}
				}
			}
		}

		// Check for cycles
		if result.len() != nodes_in_graph.len() {
			anyhow::bail!("Dependency graph contains a cycle");
		}

		// Reverse to get leaves (dependencies) first
		result.reverse();

		// Prepend nodes that aren't in the graph at all
		let result_set: std::collections::HashSet<_> = result.iter().cloned().collect();
		let mut final_result: Vec<String> = names
			.iter()
			.filter(|n| !result_set.contains(*n))
			.cloned()
			.collect();
		final_result.extend(result);

		Ok(final_result)
	}

	/// Topologically sorts nodes with roots (dependents) first.
	///
	/// This ordering ensures that dependents are processed before their dependencies,
	/// which might be useful for operations like uninstalling (remove dependents
	/// before dependencies).
	///
	/// Nodes not in the graph are included at the beginning of the result.
	///
	/// # Arguments
	///
	/// * `names` - The set of names to sort. Names not in the graph edges are
	///   included at the start of the result.
	///
	/// # Returns
	///
	/// A topologically sorted list where dependents appear before dependencies.
	///
	/// # Errors
	///
	/// Returns an error if the graph contains cycles.
	pub fn sort_roots_first(&self, names: &[String]) -> anyhow::Result<Vec<String>> {
		// Simply reverse the edges and call sort_leaves_first
		let reversed_edges: Vec<_> = self
			.edges
			.iter()
			.map(|(a, b)| (b.clone(), a.clone()))
			.collect();
		let reversed_graph = DependencyGraph::from_edges(reversed_edges);
		reversed_graph.sort_leaves_first(names)
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
/// This function analyzes all intra-workspace dependencies by grouping projects
/// by their adapter and calling `intra_dependencies()` on each unique adapter
/// with only its own projects. This ensures adapters only analyze manifests
/// they understand.
///
/// # Arguments
///
/// * `projects` - All projects in the workspace to analyze.
///
/// # Errors
///
/// Returns an error if dependency analysis fails (e.g., manifest files cannot be read).
pub fn build_dependency_graph(projects: &[Project]) -> anyhow::Result<DependencyGraph> {
	// Group projects by adapter using Arc pointer equality
	let mut adapter_groups: Vec<(Arc<dyn PackageManagerAdapter>, Vec<&ProjectInfo>)> = Vec::new();
	for project in projects {
		if let Some(group) = adapter_groups
			.iter_mut()
			.find(|(a, _)| Arc::ptr_eq(a, &project.adapter))
		{
			group.1.push(&project.info);
		} else {
			adapter_groups.push((Arc::clone(&project.adapter), vec![&project.info]));
		}
	}

	// Collect edges from each adapter group
	let mut all_edges = Vec::new();
	for (adapter, project_infos) in &adapter_groups {
		let edges = adapter.intra_dependencies(project_infos)?;
		all_edges.extend(edges);
	}

	Ok(DependencyGraph::from_edges(all_edges))
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
		std::fs::write(dir.path().join("package.json"), r#"{"name": "npm-pkg"}"#).unwrap();

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
		let graph = DependencyGraph::from_edges(vec![]);
		let sorted = graph.sort_leaves_first(&[]).unwrap();
		assert!(sorted.is_empty());
	}

	#[test]
	fn dependency_graph_single_node_no_edges() {
		let graph = DependencyGraph::from_edges(vec![]);
		let names = vec!["a".to_string()];
		let sorted = graph.sort_leaves_first(&names).unwrap();
		assert_eq!(sorted, vec!["a"]);
	}

	#[test]
	fn dependency_graph_linear_chain() {
		// a -> b -> c (a depends on b, b depends on c)
		let edges = vec![
			("a".to_string(), "b".to_string()),
			("b".to_string(), "c".to_string()),
		];
		let graph = DependencyGraph::from_edges(edges);
		let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];

		let leaves_first = graph.sort_leaves_first(&names).unwrap();
		// c is the leaf (no dependencies), then b, then a
		assert_eq!(leaves_first, vec!["c", "b", "a"]);

		let roots_first = graph.sort_roots_first(&names).unwrap();
		// a is the root (no dependents), then b, then c
		assert_eq!(roots_first, vec!["a", "b", "c"]);
	}

	#[test]
	fn dependency_graph_diamond() {
		// a -> b, a -> c, b -> d, c -> d (diamond: a depends on b and c, both depend on d)
		let edges = vec![
			("a".to_string(), "b".to_string()),
			("a".to_string(), "c".to_string()),
			("b".to_string(), "d".to_string()),
			("c".to_string(), "d".to_string()),
		];
		let graph = DependencyGraph::from_edges(edges);
		let names = vec![
			"a".to_string(),
			"b".to_string(),
			"c".to_string(),
			"d".to_string(),
		];

		let leaves_first = graph.sort_leaves_first(&names).unwrap();
		// d must come first, then b and c (in either order), then a
		assert_eq!(leaves_first[0], "d");
		assert_eq!(leaves_first[3], "a");
		assert!(leaves_first[1..3].contains(&"b".to_string()));
		assert!(leaves_first[1..3].contains(&"c".to_string()));

		let roots_first = graph.sort_roots_first(&names).unwrap();
		// a must come first, then b and c (in either order), then d
		assert_eq!(roots_first[0], "a");
		assert_eq!(roots_first[3], "d");
		assert!(roots_first[1..3].contains(&"b".to_string()));
		assert!(roots_first[1..3].contains(&"c".to_string()));
	}

	#[test]
	fn dependency_graph_disconnected_nodes() {
		// a -> b, c is disconnected
		let edges = vec![("a".to_string(), "b".to_string())];
		let graph = DependencyGraph::from_edges(edges);
		let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];

		let leaves_first = graph.sort_leaves_first(&names).unwrap();
		// c (not in graph) comes first, then b (leaf), then a
		assert_eq!(leaves_first, vec!["c", "b", "a"]);

		let roots_first = graph.sort_roots_first(&names).unwrap();
		// c (not in graph) comes first, then a (root), then b
		assert_eq!(roots_first, vec!["c", "a", "b"]);
	}

	#[test]
	fn dependency_graph_cycle_returns_error() {
		// a -> b -> c -> a (cycle)
		let edges = vec![
			("a".to_string(), "b".to_string()),
			("b".to_string(), "c".to_string()),
			("c".to_string(), "a".to_string()),
		];
		let graph = DependencyGraph::from_edges(edges);
		let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];
		let result = graph.sort_leaves_first(&names);
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
		let edges = vec![
			("a".to_string(), "c".to_string()),
			("b".to_string(), "c".to_string()),
		];
		let graph = DependencyGraph::from_edges(edges);
		let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];

		let leaves_first = graph.sort_leaves_first(&names).unwrap();
		// c first, then a and b in either order
		assert_eq!(leaves_first[0], "c");
		assert!(leaves_first[1..3].contains(&"a".to_string()));
		assert!(leaves_first[1..3].contains(&"b".to_string()));

		let roots_first = graph.sort_roots_first(&names).unwrap();
		// a and b first (in either order), then c
		assert_eq!(roots_first[2], "c");
		assert!(roots_first[0..2].contains(&"a".to_string()));
		assert!(roots_first[0..2].contains(&"b".to_string()));
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
		let names: Vec<String> = projects.iter().map(|p| p.name().to_string()).collect();
		let sorted = graph.sort_leaves_first(&names).unwrap();
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
		let names: Vec<String> = projects.iter().map(|p| p.name().to_string()).collect();
		let sorted = graph.sort_leaves_first(&names).unwrap();

		// lib should be published before app (lib is a dependency of app)
		let lib_index = sorted.iter().position(|n| n == "lib").unwrap();
		let app_index = sorted.iter().position(|n| n == "app").unwrap();
		assert!(lib_index < app_index, "lib should come before app");
	}
}
