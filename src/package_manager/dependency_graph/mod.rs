//! Dependency graph construction and topological sorting for workspace packages.

/// Mutable state threaded through Tarjan's iterative SCC algorithm.
struct TarjanState {
	index: usize,
	indices: std::collections::HashMap<String, usize>,
	lowlinks: std::collections::HashMap<String, usize>,
	on_stack: std::collections::HashSet<String>,
	stack: Vec<String>,
	sccs: Vec<Vec<String>>,
}

impl TarjanState {
	fn new() -> Self {
		Self {
			index: 0,
			indices: std::collections::HashMap::new(),
			lowlinks: std::collections::HashMap::new(),
			on_stack: std::collections::HashSet::new(),
			stack: Vec::new(),
			sccs: Vec::new(),
		}
	}

	/// Returns true if `v` is the root of a strongly connected component.
	fn is_scc_root(&self, v: &str) -> bool {
		match (self.indices.get(v), self.lowlinks.get(v)) {
			(Some(&v_index), Some(&v_lowlink)) => v_index == v_lowlink,
			_ => false,
		}
	}

	/// Pops the SCC rooted at `v` from the stack and returns it sorted.
	fn extract_scc(&mut self, v: &str) -> Vec<String> {
		let mut scc = Vec::new();
		while let Some(w) = self.stack.pop() {
			self.on_stack.remove(&w);
			let is_root = w == v;
			scc.push(w);
			if is_root {
				break;
			}
		}
		scc.sort();
		scc
	}
}

/// A directed graph for dependency ordering.
///
/// Stores an adjacency list where each node maps to its dependencies.
/// SCCs are computed eagerly during construction.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
	/// Adjacency list: dependent -> [dependencies]
	pub(crate) adjacency: std::collections::HashMap<String, Vec<String>>,
	/// Strongly connected components in reverse topological order (leaves first).
	sccs: Vec<Vec<String>>,
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
	/// SCCs are computed eagerly during construction.
	///
	/// # Arguments
	///
	/// * `adjacency` - Adjacency list mapping `dependent -> [dependencies]`.
	pub fn from_adjacency(adjacency: std::collections::HashMap<String, Vec<String>>) -> Self {
		let sccs = Self::compute_sccs(&adjacency);
		Self { adjacency, sccs }
	}

	/// Computes strongly connected components from an adjacency list.
	///
	/// Returns SCCs in reverse topological order (leaves first).
	/// Each SCC is sorted alphabetically for determinism.
	fn compute_sccs(
		adjacency: &std::collections::HashMap<String, Vec<String>>,
	) -> Vec<Vec<String>> {
		let mut state = TarjanState::new();
		let mut nodes: Vec<_> = adjacency.keys().cloned().collect();
		nodes.sort();
		for node in nodes {
			if !state.indices.contains_key(&node) {
				Self::strongconnect(adjacency, &node, &mut state);
			}
		}
		state.sccs
	}

	/// Iterative helper for Tarjan's algorithm to avoid stack overflow.
	///
	/// This uses an explicit work stack to simulate the recursive call stack.
	/// The iterative simulation requires tracking two phases per node (first-visit
	/// and post-children), which creates inherent nesting that cannot be meaningfully
	/// reduced without fragmenting the algorithm's logic.
	#[allow(clippy::excessive_nesting)]
	#[allow(clippy::too_many_lines)]
	fn strongconnect(
		adjacency: &std::collections::HashMap<String, Vec<String>>,
		start: &str,
		state: &mut TarjanState,
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
					if state.indices.contains_key(&v) {
						continue;
					}

					// Initialize this node
					let current_index = state.index;
					state.indices.insert(v.clone(), current_index);
					state.lowlinks.insert(v.clone(), current_index);
					state.index += 1;
					state.stack.push(v.clone());
					state.on_stack.insert(v.clone());

					// Schedule second visit for after children are processed
					work_stack.push((v.clone(), Phase::SecondVisit));

					// Push children onto work stack in reverse order for deterministic processing
					if let Some(deps) = adjacency.get(&v) {
						let mut sorted_deps: Vec<_> = deps.iter().collect();
						sorted_deps.sort();

						// Process in reverse so first child is processed first
						for w in sorted_deps.into_iter().rev() {
							// Skip external dependencies
							if !adjacency.contains_key(w) {
								continue;
							}

							if !state.indices.contains_key(w) {
								// Not yet visited - schedule for processing
								work_stack.push((w.clone(), Phase::FirstVisit));
							} else if state.on_stack.contains(w) {
								// Back edge - update lowlink immediately
								if let Some(&w_index) = state.indices.get(w)
									&& let Some(v_lowlink) = state.lowlinks.get_mut(&v)
								{
									*v_lowlink = (*v_lowlink).min(w_index);
								}
							}
						}
					}
				}
				Phase::SecondVisit => {
					// Update lowlinks from children
					if let Some(deps) = adjacency.get(&v) {
						for w in deps {
							// Skip external dependencies
							if !adjacency.contains_key(w) {
								continue;
							}

							// Update from child's lowlink (not a back edge)
							if !state.on_stack.contains(w) {
								continue;
							}

							// Propagate w's lowlink up to v
							if let Some(&w_lowlink) = state.lowlinks.get(w)
								&& let Some(v_lowlink) = state.lowlinks.get_mut(&v)
							{
								*v_lowlink = (*v_lowlink).min(w_lowlink);
							}
						}
					}

					// Check if v is a root of an SCC and extract if so
					if state.is_scc_root(&v) {
						let scc = state.extract_scc(&v);
						state.sccs.push(scc);
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
		self.sccs
			.iter()
			.filter(|scc| {
				if scc.len() > 1 {
					true
				} else {
					// scc.len() == 1 guaranteed by Tarjan's algorithm; check for self-loop
					let node = &scc[0];
					self.adjacency
						.get(node)
						.is_some_and(|deps| deps.contains(node))
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
	pub fn sort_leaves_first(&self) -> Vec<String> {
		// SCCs are in reverse topological order (leaves first), each sorted alphabetically
		self.sccs.iter().flatten().cloned().collect()
	}

	/// Returns the direct dependents of a package (packages that depend on it).
	///
	/// Scans the adjacency list for entries whose dependency lists contain the given
	/// package name. Only considers internal packages (those present in the graph).
	///
	/// # Arguments
	///
	/// * `package` - The package name to find dependents for.
	///
	/// # Returns
	///
	/// A vector of package names that directly depend on the given package.
	/// The order is not guaranteed to be deterministic.
	pub fn direct_dependents(&self, package: &str) -> Vec<String> {
		self.adjacency
			.iter()
			.filter(|(_, deps)| deps.iter().any(|dep| dep == package))
			.map(|(name, _)| name.clone())
			.collect()
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
	pub fn sort_roots_first(&self) -> Vec<String> {
		let mut result = self.sort_leaves_first();
		result.reverse();
		result
	}
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;
