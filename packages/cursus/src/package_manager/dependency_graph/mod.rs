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

	/// Iterative Tarjan's SCC using a resume-index work stack.
	///
	/// Each frame is `(node, sorted_children, resume_index)`. A resume index of 0
	/// means first visit; an index `i > 0` means we are returning from
	/// `children[i-1]` and must propagate its lowlink before continuing.
	/// Between those two cases we scan forward through the remaining children:
	/// unvisited ones are pushed as new frames (tree edges), already-visited
	/// on-stack ones update the lowlink immediately (back edges), and
	/// already-completed ones are skipped (cross edges). This mirrors the
	/// recursive formulation exactly, processing every edge exactly once.
	#[allow(clippy::excessive_nesting)]
	#[allow(clippy::too_many_lines)]
	fn strongconnect(
		adjacency: &std::collections::HashMap<String, Vec<String>>,
		start: &str,
		state: &mut TarjanState,
	) {
		// Returns the sorted, internal-only children of a node.
		let children_of = |node: &str| -> Vec<String> {
			let Some(deps) = adjacency.get(node) else {
				return Vec::new();
			};
			let mut kids: Vec<_> = deps
				.iter()
				.filter(|w| adjacency.contains_key(*w))
				.cloned()
				.collect();
			kids.sort();
			kids
		};

		let mut work_stack = vec![(start.to_string(), children_of(start), 0usize)];

		while let Some((v, children, i)) = work_stack.pop() {
			if i == 0 {
				// First visit: skip if already visited (e.g. start was pre-visited).
				if state.indices.contains_key(&v) {
					continue;
				}
				let idx = state.index;
				state.indices.insert(v.clone(), idx);
				state.lowlinks.insert(v.clone(), idx);
				state.index += 1;
				state.stack.push(v.clone());
				state.on_stack.insert(v.clone());
			} else {
				// Returning from children[i-1]: propagate tree-edge lowlink.
				let w = &children[i - 1];
				let w_lowlink = state.lowlinks.get(w).copied();
				if let (Some(w_lowlink), Some(v_lowlink)) = (w_lowlink, state.lowlinks.get_mut(&v))
				{
					*v_lowlink = (*v_lowlink).min(w_lowlink);
				}
			}

			// Advance through remaining children to find the next one to recurse into.
			let mut next_i = i;
			let mut recurse: Option<String> = None;
			while next_i < children.len() {
				let w = &children[next_i];
				next_i += 1;
				if !state.indices.contains_key(w) {
					// Tree edge: recurse into w.
					recurse = Some(w.clone());
					break;
				} else if state.on_stack.contains(w) {
					// Back edge: tighten v's lowlink using w's discovery index.
					if let (Some(&w_index), Some(v_lowlink)) =
						(state.indices.get(w), state.lowlinks.get_mut(&v))
					{
						*v_lowlink = (*v_lowlink).min(w_index);
					}
				}
				// Cross edge (w already in a completed SCC): skip.
			}

			if let Some(w) = recurse {
				// Suspend v at next_i; process w first.
				work_stack.push((v, children, next_i));
				work_stack.push((w.clone(), children_of(&w), 0));
			} else {
				// All children processed: check if v is an SCC root.
				if state.is_scc_root(&v) {
					let scc = state.extract_scc(&v);
					state.sccs.push(scc);
				}
			}
		}
	}

	/// Returns groups of packages with circular dependencies.
	///
	/// Each group contains two or more package names that form a cycle.
	/// Single-node self-loops are not included: a package that lists itself as a
	/// dependency (e.g. via `dev-dependencies`) does not affect release ordering
	/// because only one package is involved.
	/// Groups are sorted alphabetically for deterministic output.
	///
	/// # Returns
	///
	/// A vector of cycle groups, where each group is a vector of package names.
	pub fn cycle_groups(&self) -> Vec<Vec<String>> {
		self.sccs
			.iter()
			.filter(|scc| scc.len() > 1)
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
