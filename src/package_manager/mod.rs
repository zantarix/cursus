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
/// Stores an adjacency list where each node maps to its dependencies.
/// SCCs are computed lazily on first access and cached for reuse.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
	/// Adjacency list: dependent -> [dependencies]
	adjacency: std::collections::HashMap<String, Vec<String>>,
	/// Cached strongly connected components (computed lazily)
	sccs: std::cell::OnceCell<Vec<Vec<String>>>,
}

impl DependencyGraph {
	/// Creates a new dependency graph from an adjacency list.
	///
	/// The adjacency list maps each node to its list of dependencies.
	///
	/// Only nodes that appear as keys in the adjacency list are part of the graph.
	/// Dependencies that don't appear as keys are considered external and are ignored
	/// during topological sorting.
	///
	/// SCCs are computed lazily on first access and cached for subsequent calls.
	///
	/// # Arguments
	///
	/// * `adjacency` - Adjacency list mapping `dependent -> [dependencies]`.
	pub fn from_adjacency(adjacency: std::collections::HashMap<String, Vec<String>>) -> Self {
		Self {
			adjacency,
			sccs: std::cell::OnceCell::new(),
		}
	}

	/// Returns whether the SCC cache has been populated.
	///
	/// This is primarily useful for testing to verify caching behavior.
	#[cfg(test)]
	fn is_sccs_cached(&self) -> bool {
		self.sccs.get().is_some()
	}

	/// Finds strongly connected components using Tarjan's algorithm.
	///
	/// Returns a reference to cached SCCs in reverse topological order (leaves first).
	/// Each SCC is a vector of node names sorted alphabetically for determinism.
	///
	/// SCCs are computed on first call and cached for subsequent calls.
	fn find_sccs(&self) -> &Vec<Vec<String>> {
		self.sccs.get_or_init(|| {
			let mut index = 0usize;
			let mut indices = std::collections::HashMap::new();
			let mut lowlinks = std::collections::HashMap::new();
			let mut on_stack = std::collections::HashSet::new();
			let mut stack = Vec::new();
			let mut sccs = Vec::new();

			// Sort keys for deterministic output
			let mut nodes: Vec<_> = self.adjacency.keys().cloned().collect();
			nodes.sort();

			for node in nodes {
				if !indices.contains_key(&node) {
					self.strongconnect(
						&node,
						&mut index,
						&mut indices,
						&mut lowlinks,
						&mut on_stack,
						&mut stack,
						&mut sccs,
					);
				}
			}

			sccs
		})
	}

	/// Iterative helper for Tarjan's algorithm to avoid stack overflow.
	///
	/// This uses an explicit work stack to simulate the recursive call stack.
	#[allow(clippy::too_many_arguments)]
	fn strongconnect(
		&self,
		start: &str,
		index: &mut usize,
		indices: &mut std::collections::HashMap<String, usize>,
		lowlinks: &mut std::collections::HashMap<String, usize>,
		on_stack: &mut std::collections::HashSet<String>,
		stack: &mut Vec<String>,
		sccs: &mut Vec<Vec<String>>,
	) {
		// Work item: (node, phase)
		// Phase 0 = first visit (initialize and push children)
		// Phase 1 = second visit (update lowlinks and extract SCC)
		enum Phase {
			FirstVisit,
			SecondVisit,
		}

		let mut work_stack: Vec<(String, Phase)> = vec![(start.to_string(), Phase::FirstVisit)];

		while let Some((v, phase)) = work_stack.pop() {
			match phase {
				Phase::FirstVisit => {
					// Skip if already visited
					if indices.contains_key(&v) {
						continue;
					}

					// Initialize this node
					let current_index = *index;
					indices.insert(v.clone(), current_index);
					lowlinks.insert(v.clone(), current_index);
					*index += 1;
					stack.push(v.clone());
					on_stack.insert(v.clone());

					// Schedule second visit for after children are processed
					work_stack.push((v.clone(), Phase::SecondVisit));

					// Push children onto work stack in reverse order for deterministic processing
					if let Some(deps) = self.adjacency.get(&v) {
						let mut sorted_deps: Vec<_> = deps.iter().collect();
						sorted_deps.sort();

						// Process in reverse so first child is processed first
						for w in sorted_deps.into_iter().rev() {
							// Skip external dependencies
							if !self.adjacency.contains_key(w) {
								continue;
							}

							if !indices.contains_key(w) {
								// Not yet visited - schedule for processing
								work_stack.push((w.clone(), Phase::FirstVisit));
							} else if on_stack.contains(w) {
								// Back edge - update lowlink immediately
								if let Some(&w_index) = indices.get(w)
									&& let Some(v_lowlink) = lowlinks.get_mut(&v)
								{
									*v_lowlink = (*v_lowlink).min(w_index);
								}
							}
						}
					}
				}
				Phase::SecondVisit => {
					// Update lowlinks from children
					if let Some(deps) = self.adjacency.get(&v) {
						for w in deps {
							// Skip external dependencies
							if !self.adjacency.contains_key(w) {
								continue;
							}

							// Update from child's lowlink (not a back edge)
							if !on_stack.contains(w) {
								continue;
							}

							// Get w's lowlink and update v's if needed
							if let Some(&w_lowlink) = lowlinks.get(w)
								&& let Some(v_lowlink) = lowlinks.get_mut(&v)
								&& let Some(&v_index) = indices.get(&v)
								&& w_lowlink < *v_lowlink
								&& w_lowlink < v_index
							{
								*v_lowlink = w_lowlink;
							}
						}
					}

					// Check if v is a root of an SCC
					let is_scc_root = if let (Some(&v_index), Some(&v_lowlink)) =
						(indices.get(&v), lowlinks.get(&v))
					{
						v_index == v_lowlink
					} else {
						false
					};

					if is_scc_root {
						// Pop the SCC from the stack
						let mut scc = Vec::new();
						while let Some(w) = stack.pop() {
							on_stack.remove(&w);
							let is_root = w == v;
							scc.push(w);
							if is_root {
								break;
							}
						}
						// Sort SCC members alphabetically for determinism
						scc.sort();
						sccs.push(scc);
					}
				}
			}
		}
	}

	/// Returns groups of packages with circular dependencies.
	///
	/// Each group contains one or more package names that form a cycle.
	/// Single-node groups are only included if they have a self-loop.
	/// Groups are sorted alphabetically for deterministic output.
	///
	/// # Returns
	///
	/// A vector of cycle groups, where each group is a vector of package names.
	pub fn cycle_groups(&self) -> Vec<Vec<String>> {
		self.find_sccs()
			.iter()
			.filter(|scc| {
				if scc.len() > 1 {
					true
				} else if scc.len() == 1 {
					// Check for self-loop
					let node = &scc[0];
					if let Some(deps) = self.adjacency.get(node) {
						deps.contains(node)
					} else {
						false
					}
				} else {
					false
				}
			})
			.cloned()
			.collect()
	}

	/// Topologically sorts all nodes in the graph with leaves (dependencies) first.
	///
	/// This ordering ensures that dependencies are processed before their dependents,
	/// which is appropriate for operations like publishing packages (publish
	/// dependencies before dependents).
	///
	/// Handles circular dependencies gracefully by grouping mutually-dependent
	/// packages together in alphabetical order within their strongly connected component.
	///
	/// # Returns
	///
	/// A topologically sorted list where dependencies appear before dependents.
	pub fn sort_leaves_first(&self) -> anyhow::Result<Vec<String>> {
		// find_sccs() returns cached SCCs in reverse topological order (leaves first)
		// Each SCC is already sorted alphabetically
		let sccs = self.find_sccs();
		Ok(sccs.iter().flatten().cloned().collect())
	}

	/// Topologically sorts all nodes in the graph with roots (dependents) first.
	///
	/// This ordering ensures that dependents are processed before their dependencies,
	/// which might be useful for operations like uninstalling (remove dependents
	/// before dependencies).
	///
	/// This is the exact reverse of `sort_leaves_first`.
	///
	/// # Returns
	///
	/// A topologically sorted list where dependents appear before dependencies.
	pub fn sort_roots_first(&self) -> anyhow::Result<Vec<String>> {
		let mut result = self.sort_leaves_first()?;
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
	fn dependency_graph_cycle_succeeds_with_correct_ordering() {
		// a -> b -> c -> a (cycle)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec!["a".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let result = graph.sort_leaves_first();
		assert!(result.is_ok());

		// All three nodes should be in the output, sorted alphabetically within their SCC
		let sorted = result.unwrap();
		assert_eq!(sorted.len(), 3);
		assert!(sorted.contains(&"a".to_string()));
		assert!(sorted.contains(&"b".to_string()));
		assert!(sorted.contains(&"c".to_string()));
	}

	#[test]
	fn dependency_graph_self_loop() {
		// a -> a (self-loop)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["a".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();
		assert_eq!(sorted, vec!["a"]);

		// cycle_groups should detect the self-loop
		let cycles = graph.cycle_groups();
		assert_eq!(cycles.len(), 1);
		assert_eq!(cycles[0], vec!["a"]);
	}

	#[test]
	fn dependency_graph_two_node_cycle() {
		// a <-> b (mutual dependency)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["a".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// Both should appear, alphabetically sorted within SCC
		assert_eq!(sorted.len(), 2);
		assert_eq!(sorted, vec!["a", "b"]);

		// cycle_groups should detect the cycle
		let cycles = graph.cycle_groups();
		assert_eq!(cycles.len(), 1);
		assert_eq!(cycles[0], vec!["a", "b"]);
	}

	#[test]
	fn dependency_graph_partial_cycle_plus_dag() {
		// a -> b <-> c -> d (b and c form a cycle, a and d are DAG nodes)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec!["b".to_string(), "d".to_string()]);
		adjacency.insert("d".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// d should come first (leaf), then b and c (cycle, alphabetical), then a (root)
		assert_eq!(sorted[0], "d");
		assert_eq!(sorted[3], "a");
		// b and c should be together and sorted
		let bc_slice = &sorted[1..3];
		assert_eq!(bc_slice, &["b", "c"]);

		// cycle_groups should detect only the b-c cycle
		let cycles = graph.cycle_groups();
		assert_eq!(cycles.len(), 1);
		assert_eq!(cycles[0], vec!["b", "c"]);
	}

	#[test]
	fn dependency_graph_multiple_independent_cycles() {
		// a <-> b, c <-> d (two independent cycles)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["a".to_string()]);
		adjacency.insert("c".to_string(), vec!["d".to_string()]);
		adjacency.insert("d".to_string(), vec!["c".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// All four should appear
		assert_eq!(sorted.len(), 4);

		// Each cycle should be internally sorted
		// The order of the two cycles relative to each other is not guaranteed

		// cycle_groups should detect both cycles
		let cycles = graph.cycle_groups();
		assert_eq!(cycles.len(), 2);

		// Find which cycle is which
		let ab_cycle = cycles.iter().find(|c| c.contains(&"a".to_string()));
		let cd_cycle = cycles.iter().find(|c| c.contains(&"c".to_string()));

		assert!(ab_cycle.is_some());
		assert!(cd_cycle.is_some());
		assert_eq!(ab_cycle.unwrap(), &vec!["a", "b"]);
		assert_eq!(cd_cycle.unwrap(), &vec!["c", "d"]);
	}

	#[test]
	fn dependency_graph_diamond_with_cycle() {
		// a -> b, a -> c, b <-> c (diamond where b and c form a cycle)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string(), "c".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec!["b".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// b and c should come before a
		// b and c should be sorted alphabetically
		assert_eq!(sorted.len(), 3);
		assert_eq!(sorted[0], "b");
		assert_eq!(sorted[1], "c");
		assert_eq!(sorted[2], "a");

		// cycle_groups should detect the b-c cycle
		let cycles = graph.cycle_groups();
		assert_eq!(cycles.len(), 1);
		assert_eq!(cycles[0], vec!["b", "c"]);
	}

	#[test]
	fn cycle_groups_empty_for_dag() {
		// Simple DAG: a -> b -> c
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let cycles = graph.cycle_groups();
		assert!(cycles.is_empty());
	}

	#[test]
	fn dependency_graph_sorting_is_deterministic() {
		// Create a cycle and verify the output is always the same
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("c".to_string(), vec!["a".to_string()]);
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		// Run multiple times to verify determinism
		let first = graph.sort_leaves_first().unwrap();
		for _ in 0..10 {
			let result = graph.sort_leaves_first().unwrap();
			assert_eq!(result, first, "Sorting should be deterministic");
		}

		// The SCC should be sorted alphabetically
		assert_eq!(first, vec!["a", "b", "c"]);
	}

	#[test]
	fn dependency_graph_caches_sccs() {
		// Create a graph with cycles
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["a".to_string()]);
		adjacency.insert("c".to_string(), vec!["d".to_string()]);
		adjacency.insert("d".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		// Initially, cache should be empty
		assert!(!graph.is_sccs_cached(), "Cache should be empty on creation");

		// Call sort_leaves_first() which computes SCCs
		let sorted1 = graph.sort_leaves_first().unwrap();

		// Now cache should be populated
		assert!(
			graph.is_sccs_cached(),
			"Cache should be populated after first call"
		);

		// Call cycle_groups() which should reuse cached SCCs (not recompute)
		let cycles = graph.cycle_groups();

		// Cache should still be populated (not cleared)
		assert!(
			graph.is_sccs_cached(),
			"Cache should remain populated after cycle_groups"
		);

		// Call sort_leaves_first() again - should return same result using cache
		let sorted2 = graph.sort_leaves_first().unwrap();

		// Verify results are consistent
		assert_eq!(sorted1, sorted2);
		assert_eq!(cycles.len(), 1);
		assert_eq!(cycles[0], vec!["a", "b"]);

		// Verify both d and the cycle are in the sorted output
		assert!(sorted1.contains(&"d".to_string()));
		assert!(sorted1.contains(&"a".to_string()));
		assert!(sorted1.contains(&"b".to_string()));
		assert!(sorted1.contains(&"c".to_string()));
	}

	#[test]
	fn dependency_graph_clone_preserves_cache() {
		// Create a graph
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		// Populate the cache
		let _sorted = graph.sort_leaves_first().unwrap();
		assert!(graph.is_sccs_cached(), "Cache should be populated");

		// Clone the graph
		let cloned = graph.clone();

		// Cloned graph should also have the cache populated
		assert!(
			cloned.is_sccs_cached(),
			"Cloned graph should preserve the cache"
		);

		// Both should return the same results
		let original_sorted = graph.sort_leaves_first().unwrap();
		let cloned_sorted = cloned.sort_leaves_first().unwrap();
		assert_eq!(original_sorted, cloned_sorted);
	}

	#[test]
	fn dependency_graph_with_external_dependencies() {
		// Graph where some dependencies are external (not in the graph)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert(
			"app".to_string(),
			vec![
				"lib".to_string(),
				"external-dep".to_string(), // External - not in graph
			],
		);
		adjacency.insert("lib".to_string(), vec!["react".to_string()]); // External

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// Should only include internal nodes, external deps are ignored
		assert_eq!(sorted.len(), 2);
		assert!(sorted.contains(&"app".to_string()));
		assert!(sorted.contains(&"lib".to_string()));
		assert!(!sorted.contains(&"external-dep".to_string()));
		assert!(!sorted.contains(&"react".to_string()));

		// lib should come before app
		let lib_idx = sorted.iter().position(|n| n == "lib").unwrap();
		let app_idx = sorted.iter().position(|n| n == "app").unwrap();
		assert!(lib_idx < app_idx);
	}

	#[test]
	fn dependency_graph_cross_edges_between_sccs() {
		// Complex graph with multiple SCCs and cross edges
		// SCC1: a <-> b
		// SCC2: c <-> d
		// Cross edges: a -> c (SCC1 depends on SCC2)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string(), "c".to_string()]);
		adjacency.insert("b".to_string(), vec!["a".to_string()]);
		adjacency.insert("c".to_string(), vec!["d".to_string()]);
		adjacency.insert("d".to_string(), vec!["c".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// c,d (SCC2) should come before a,b (SCC1)
		let c_idx = sorted.iter().position(|n| n == "c").unwrap();
		let d_idx = sorted.iter().position(|n| n == "d").unwrap();
		let a_idx = sorted.iter().position(|n| n == "a").unwrap();
		let b_idx = sorted.iter().position(|n| n == "b").unwrap();

		assert!(c_idx < a_idx && c_idx < b_idx);
		assert!(d_idx < a_idx && d_idx < b_idx);

		// Both cycles should be detected
		let cycles = graph.cycle_groups();
		assert_eq!(cycles.len(), 2);
	}

	#[test]
	fn dependency_graph_node_with_no_dependencies() {
		// Node with empty dependency list
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("standalone".to_string(), vec![]);
		adjacency.insert("app".to_string(), vec!["standalone".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// standalone has no deps, should come first
		assert_eq!(sorted[0], "standalone");
		assert_eq!(sorted[1], "app");

		// No cycles
		let cycles = graph.cycle_groups();
		assert!(cycles.is_empty());
	}

	#[test]
	fn dependency_graph_complex_mixed_structure() {
		// Complex graph with:
		// - DAG part: e -> f -> g
		// - Cycle: a <-> b
		// - Cross edge from cycle to DAG: a -> f
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string(), "f".to_string()]);
		adjacency.insert("b".to_string(), vec!["a".to_string()]);
		adjacency.insert("e".to_string(), vec!["f".to_string()]);
		adjacency.insert("f".to_string(), vec!["g".to_string()]);
		adjacency.insert("g".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// g is the deepest leaf
		assert_eq!(sorted[0], "g");

		// a,b cycle should come after g,f (since a depends on f)
		let g_idx = sorted.iter().position(|n| n == "g").unwrap();
		let f_idx = sorted.iter().position(|n| n == "f").unwrap();
		let a_idx = sorted.iter().position(|n| n == "a").unwrap();
		let b_idx = sorted.iter().position(|n| n == "b").unwrap();
		let e_idx = sorted.iter().position(|n| n == "e").unwrap();

		assert!(g_idx < f_idx);
		assert!(f_idx < a_idx);
		assert!(f_idx < b_idx);
		assert!(f_idx < e_idx);

		// Only one cycle
		let cycles = graph.cycle_groups();
		assert_eq!(cycles.len(), 1);
		assert_eq!(cycles[0], vec!["a", "b"]);
	}

	#[test]
	fn dependency_graph_back_edge_to_ancestor() {
		// Graph with back edge to ancestor (not just parent)
		// a -> b -> c -> a (cycle through multiple nodes)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec!["a".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// All three in one SCC, alphabetically sorted
		assert_eq!(sorted, vec!["a", "b", "c"]);

		// One cycle containing all three
		let cycles = graph.cycle_groups();
		assert_eq!(cycles.len(), 1);
		assert_eq!(cycles[0], vec!["a", "b", "c"]);
	}

	#[test]
	fn dependency_graph_shared_dependency() {
		// Multiple nodes sharing the same dependency
		// a -> c, b -> c (both a and b depend on c)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["c".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// c must come first
		assert_eq!(sorted[0], "c");

		// a and b come after c
		assert!(sorted.contains(&"a".to_string()));
		assert!(sorted.contains(&"b".to_string()));
	}

	#[test]
	fn dependency_graph_multiple_back_edges() {
		// Node with multiple back edges in a cycle
		// a -> b, a -> c, b -> c, c -> a
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string(), "c".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec!["a".to_string()]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// All in one SCC
		assert_eq!(sorted.len(), 3);
		assert_eq!(sorted, vec!["a", "b", "c"]);

		let cycles = graph.cycle_groups();
		assert_eq!(cycles.len(), 1);
	}

	#[test]
	fn dependency_graph_deep_tree() {
		// Deep dependency tree: a -> b -> c -> d -> e -> f
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec!["d".to_string()]);
		adjacency.insert("d".to_string(), vec!["e".to_string()]);
		adjacency.insert("e".to_string(), vec!["f".to_string()]);
		adjacency.insert("f".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// Should be f, e, d, c, b, a (reverse order)
		assert_eq!(sorted, vec!["f", "e", "d", "c", "b", "a"]);

		let cycles = graph.cycle_groups();
		assert!(cycles.is_empty());
	}

	#[test]
	fn dependency_graph_wide_tree() {
		// Wide tree: a depends on many nodes
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert(
			"a".to_string(),
			vec![
				"b".to_string(),
				"c".to_string(),
				"d".to_string(),
				"e".to_string(),
			],
		);
		adjacency.insert("b".to_string(), vec![]);
		adjacency.insert("c".to_string(), vec![]);
		adjacency.insert("d".to_string(), vec![]);
		adjacency.insert("e".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// All leaves should come before a
		let a_idx = sorted.iter().position(|n| n == "a").unwrap();
		let b_idx = sorted.iter().position(|n| n == "b").unwrap();
		let c_idx = sorted.iter().position(|n| n == "c").unwrap();
		let d_idx = sorted.iter().position(|n| n == "d").unwrap();
		let e_idx = sorted.iter().position(|n| n == "e").unwrap();

		assert!(b_idx < a_idx);
		assert!(c_idx < a_idx);
		assert!(d_idx < a_idx);
		assert!(e_idx < a_idx);
	}

	#[test]
	fn dependency_graph_parallel_chains() {
		// Two parallel dependency chains with no connection
		// a -> b -> c and d -> e -> f
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec![]);
		adjacency.insert("d".to_string(), vec!["e".to_string()]);
		adjacency.insert("e".to_string(), vec!["f".to_string()]);
		adjacency.insert("f".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);
		let sorted = graph.sort_leaves_first().unwrap();

		// Verify ordering within each chain
		let a_idx = sorted.iter().position(|n| n == "a").unwrap();
		let b_idx = sorted.iter().position(|n| n == "b").unwrap();
		let c_idx = sorted.iter().position(|n| n == "c").unwrap();
		assert!(c_idx < b_idx && b_idx < a_idx);

		let d_idx = sorted.iter().position(|n| n == "d").unwrap();
		let e_idx = sorted.iter().position(|n| n == "e").unwrap();
		let f_idx = sorted.iter().position(|n| n == "f").unwrap();
		assert!(f_idx < e_idx && e_idx < d_idx);
	}

	// Tests that directly exercise strongconnect with violated invariants
	// to prove defensive branches handle corrupted state gracefully

	#[test]
	fn strongconnect_handles_missing_node_in_adjacency() {
		// Test: strongconnect called on node not in adjacency map
		let adjacency = std::collections::HashMap::new();
		let graph = DependencyGraph::from_adjacency(adjacency);

		let mut index = 0;
		let mut indices = std::collections::HashMap::new();
		let mut lowlinks = std::collections::HashMap::new();
		let mut on_stack = std::collections::HashSet::new();
		let mut stack = Vec::new();
		let mut sccs = Vec::new();

		// Call strongconnect with a node that doesn't exist in the graph
		graph.strongconnect(
			"nonexistent",
			&mut index,
			&mut indices,
			&mut lowlinks,
			&mut on_stack,
			&mut stack,
			&mut sccs,
		);

		// Should create SCC with just this node (defensive behavior)
		assert_eq!(sccs.len(), 1);
		assert_eq!(sccs[0], vec!["nonexistent"]);
		assert!(indices.contains_key("nonexistent"));
		assert!(lowlinks.contains_key("nonexistent"));
	}

	#[test]
	fn strongconnect_handles_corrupted_lowlinks() {
		// Test: defensive branch where lowlinks.get(w) might fail
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		let mut index = 0;
		let mut indices = std::collections::HashMap::new();
		let mut lowlinks = std::collections::HashMap::new();
		let mut on_stack = std::collections::HashSet::new();
		let mut stack = Vec::new();
		let mut sccs = Vec::new();

		// Pre-populate indices for "b" but NOT lowlinks (violates invariant)
		indices.insert("b".to_string(), 99);
		on_stack.insert("b".to_string());

		// Call strongconnect on "a" which depends on "b"
		// The defensive check `if let Some(&w_lowlink) = lowlinks.get(w)` will fail for "b"
		graph.strongconnect(
			"a",
			&mut index,
			&mut indices,
			&mut lowlinks,
			&mut on_stack,
			&mut stack,
			&mut sccs,
		);

		// Should still complete without panic - defensive code skips the update
		assert!(indices.contains_key("a"));
		assert!(lowlinks.contains_key("a"));
	}

	#[test]
	fn strongconnect_handles_already_visited_node() {
		// Test: FirstVisit phase when node already in indices (skip case)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		let mut index = 0;
		let mut indices = std::collections::HashMap::new();
		let mut lowlinks = std::collections::HashMap::new();
		let mut on_stack = std::collections::HashSet::new();
		let mut stack = Vec::new();
		let mut sccs = Vec::new();

		// First call - normal processing
		graph.strongconnect(
			"a",
			&mut index,
			&mut indices,
			&mut lowlinks,
			&mut on_stack,
			&mut stack,
			&mut sccs,
		);

		let initial_sccs_count = sccs.len();

		// Second call on already-visited node - should skip
		graph.strongconnect(
			"a",
			&mut index,
			&mut indices,
			&mut lowlinks,
			&mut on_stack,
			&mut stack,
			&mut sccs,
		);

		// Should not create duplicate SCC
		assert_eq!(sccs.len(), initial_sccs_count);
	}

	#[test]
	fn strongconnect_handles_node_not_on_stack_in_second_visit() {
		// Test: SecondVisit when dependency is not on stack (cross-edge case)
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert("a".to_string(), vec!["b".to_string(), "c".to_string()]);
		adjacency.insert("b".to_string(), vec![]);
		adjacency.insert("c".to_string(), vec![]);

		let graph = DependencyGraph::from_adjacency(adjacency);

		let mut index = 0;
		let mut indices = std::collections::HashMap::new();
		let mut lowlinks = std::collections::HashMap::new();
		let mut on_stack = std::collections::HashSet::new();
		let mut stack = Vec::new();
		let mut sccs = Vec::new();

		// Process normally - "b" will be visited and removed from stack before "c"
		graph.strongconnect(
			"a",
			&mut index,
			&mut indices,
			&mut lowlinks,
			&mut on_stack,
			&mut stack,
			&mut sccs,
		);

		// Should handle cross-edges where dependency is already processed
		// The `if !on_stack.contains(w)` branch is exercised
		assert!(sccs.len() >= 1);
	}

	#[test]
	fn strongconnect_handles_lowlink_not_improving() {
		// Test: branches where w_lowlink >= v_lowlink (no improvement)
		let mut adjacency = std::collections::HashMap::new();
		// Create a structure where lowlink won't improve
		adjacency.insert("a".to_string(), vec!["b".to_string()]);
		adjacency.insert("b".to_string(), vec!["c".to_string()]);
		adjacency.insert("c".to_string(), vec!["b".to_string()]); // Back edge

		let graph = DependencyGraph::from_adjacency(adjacency);

		let mut index = 0;
		let mut indices = std::collections::HashMap::new();
		let mut lowlinks = std::collections::HashMap::new();
		let mut on_stack = std::collections::HashSet::new();
		let mut stack = Vec::new();
		let mut sccs = Vec::new();

		graph.strongconnect(
			"a",
			&mut index,
			&mut indices,
			&mut lowlinks,
			&mut on_stack,
			&mut stack,
			&mut sccs,
		);

		// The condition `w_lowlink < *v_lowlink` will be false in some cases
		// Defensive code handles this gracefully
		assert!(sccs.len() > 0);
	}

	#[test]
	fn strongconnect_with_self_referencing_external_dep() {
		// Test: node depends on itself AND on external dependency
		let mut adjacency = std::collections::HashMap::new();
		adjacency.insert(
			"a".to_string(),
			vec![
				"a".to_string(),        // Self-loop
				"external".to_string(), // External (not in adjacency)
			],
		);

		let graph = DependencyGraph::from_adjacency(adjacency);

		let mut index = 0;
		let mut indices = std::collections::HashMap::new();
		let mut lowlinks = std::collections::HashMap::new();
		let mut on_stack = std::collections::HashSet::new();
		let mut stack = Vec::new();
		let mut sccs = Vec::new();

		graph.strongconnect(
			"a",
			&mut index,
			&mut indices,
			&mut lowlinks,
			&mut on_stack,
			&mut stack,
			&mut sccs,
		);

		// Should handle self-loop and skip external dependency
		assert_eq!(sccs.len(), 1);
		assert_eq!(sccs[0], vec!["a"]);

		// "external" should not appear in any data structures
		assert!(!indices.contains_key("external"));
		assert!(!sccs[0].contains(&"external".to_string()));
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
