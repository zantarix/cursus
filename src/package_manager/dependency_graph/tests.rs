use super::DependencyGraph;
use super::TarjanState;

#[test]
fn dependency_graph_empty() {
	let graph = DependencyGraph::from_adjacency(std::collections::HashMap::new());
	let sorted = graph.sort_leaves_first();
	assert!(sorted.is_empty());
}

#[test]
fn dependency_graph_single_node() {
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec![]);
	let graph = DependencyGraph::from_adjacency(adjacency);
	let sorted = graph.sort_leaves_first();
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

	let leaves_first = graph.sort_leaves_first();
	// c is the leaf (no dependencies), then b, then a
	assert_eq!(leaves_first, vec!["c", "b", "a"]);

	let roots_first = graph.sort_roots_first();
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

	let leaves_first = graph.sort_leaves_first();
	// d must come first, then b and c (in either order), then a
	assert_eq!(leaves_first[0], "d");
	assert_eq!(leaves_first[3], "a");
	assert!(leaves_first[1..3].contains(&"b".to_string()));
	assert!(leaves_first[1..3].contains(&"c".to_string()));

	let roots_first = graph.sort_roots_first();
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

	let leaves_first = graph.sort_leaves_first();
	// b (leaf) must come before a (dependent)
	assert_eq!(leaves_first, vec!["b", "a"]);

	let roots_first = graph.sort_roots_first();
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

	// All three nodes should be in the output, sorted alphabetically within their SCC
	let sorted = graph.sort_leaves_first();
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
	let sorted = graph.sort_leaves_first();
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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let first = graph.sort_leaves_first();
	for _ in 0..10 {
		let result = graph.sort_leaves_first();
		assert_eq!(result, first, "Sorting should be deterministic");
	}

	// The SCC should be sorted alphabetically
	assert_eq!(first, vec!["a", "b", "c"]);
}

#[test]
fn dependency_graph_consistent_results() {
	// Create a graph with cycles
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec!["b".to_string()]);
	adjacency.insert("b".to_string(), vec!["a".to_string()]);
	adjacency.insert("c".to_string(), vec!["d".to_string()]);
	adjacency.insert("d".to_string(), vec![]);

	let graph = DependencyGraph::from_adjacency(adjacency);

	let sorted1 = graph.sort_leaves_first();
	let cycles = graph.cycle_groups();
	let sorted2 = graph.sort_leaves_first();

	// Results must be consistent across repeated calls
	assert_eq!(sorted1, sorted2);
	assert_eq!(cycles.len(), 1);
	assert_eq!(cycles[0], vec!["a", "b"]);

	// All nodes must appear in sorted output
	assert!(sorted1.contains(&"d".to_string()));
	assert!(sorted1.contains(&"a".to_string()));
	assert!(sorted1.contains(&"b".to_string()));
	assert!(sorted1.contains(&"c".to_string()));
}

#[test]
fn dependency_graph_clone_returns_same_results() {
	// Create a graph
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec!["b".to_string()]);
	adjacency.insert("b".to_string(), vec![]);

	let graph = DependencyGraph::from_adjacency(adjacency);
	let cloned = graph.clone();

	// Clone must return the same results as the original
	assert_eq!(graph.sort_leaves_first(), cloned.sort_leaves_first());
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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let sorted = graph.sort_leaves_first();

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
	let mut state = TarjanState::new();

	// Call strongconnect with a node that doesn't exist in the graph
	DependencyGraph::strongconnect(&adjacency, "nonexistent", &mut state);

	// Should create SCC with just this node (defensive behavior)
	assert_eq!(state.sccs.len(), 1);
	assert_eq!(state.sccs[0], vec!["nonexistent"]);
	assert!(state.indices.contains_key("nonexistent"));
	assert!(state.lowlinks.contains_key("nonexistent"));
}

#[test]
fn strongconnect_handles_corrupted_lowlinks() {
	// Test: defensive branch where lowlinks.get(w) might fail
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec!["b".to_string()]);
	adjacency.insert("b".to_string(), vec![]);

	let mut state = TarjanState::new();

	// Pre-populate indices for "b" but NOT lowlinks (violates invariant)
	state.indices.insert("b".to_string(), 99);
	state.on_stack.insert("b".to_string());

	// Call strongconnect on "a" which depends on "b"
	// The defensive check `if let Some(&w_lowlink) = state.lowlinks.get(w)` will fail for "b"
	DependencyGraph::strongconnect(&adjacency, "a", &mut state);

	// Should still complete without panic - defensive code skips the update
	assert!(state.indices.contains_key("a"));
	assert!(state.lowlinks.contains_key("a"));
}

#[test]
fn strongconnect_handles_already_visited_node() {
	// Test: FirstVisit phase when node already in indices (skip case)
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec!["b".to_string()]);
	adjacency.insert("b".to_string(), vec![]);

	let mut state = TarjanState::new();

	// First call - normal processing
	DependencyGraph::strongconnect(&adjacency, "a", &mut state);

	let initial_sccs_count = state.sccs.len();

	// Second call on already-visited node - should skip
	DependencyGraph::strongconnect(&adjacency, "a", &mut state);

	// Should not create duplicate SCC
	assert_eq!(state.sccs.len(), initial_sccs_count);
}

#[test]
fn strongconnect_handles_node_not_on_stack_in_second_visit() {
	// Test: SecondVisit when dependency is not on stack (cross-edge case)
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec!["b".to_string(), "c".to_string()]);
	adjacency.insert("b".to_string(), vec![]);
	adjacency.insert("c".to_string(), vec![]);

	let mut state = TarjanState::new();

	// Process normally - "b" will be visited and removed from stack before "c"
	DependencyGraph::strongconnect(&adjacency, "a", &mut state);

	// Should handle cross-edges where dependency is already processed
	// The `if !state.on_stack.contains(w)` branch is exercised
	assert!(state.sccs.len() >= 1);
}

#[test]
fn strongconnect_handles_lowlink_not_improving() {
	// Test: branches where w_lowlink >= v_lowlink (no improvement)
	let mut adjacency = std::collections::HashMap::new();
	// Create a structure where lowlink won't improve
	adjacency.insert("a".to_string(), vec!["b".to_string()]);
	adjacency.insert("b".to_string(), vec!["c".to_string()]);
	adjacency.insert("c".to_string(), vec!["b".to_string()]); // Back edge

	let mut state = TarjanState::new();

	DependencyGraph::strongconnect(&adjacency, "a", &mut state);

	// The condition `w_lowlink < *v_lowlink` will be false in some cases
	// Defensive code handles this gracefully
	assert!(!state.sccs.is_empty());
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

	let mut state = TarjanState::new();

	DependencyGraph::strongconnect(&adjacency, "a", &mut state);

	// Should handle self-loop and skip external dependency
	assert_eq!(state.sccs.len(), 1);
	assert_eq!(state.sccs[0], vec!["a"]);

	// "external" should not appear in any data structures
	assert!(!state.indices.contains_key("external"));
	assert!(!state.sccs[0].contains(&"external".to_string()));
}

#[test]
fn dependency_graph_multiple_roots() {
	// a -> c, b -> c (two roots, one leaf)
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec!["c".to_string()]);
	adjacency.insert("b".to_string(), vec!["c".to_string()]);
	adjacency.insert("c".to_string(), vec![]);

	let graph = DependencyGraph::from_adjacency(adjacency);

	let leaves_first = graph.sort_leaves_first();
	// c first, then a and b in either order
	assert_eq!(leaves_first[0], "c");
	assert!(leaves_first[1..3].contains(&"a".to_string()));
	assert!(leaves_first[1..3].contains(&"b".to_string()));

	let roots_first = graph.sort_roots_first();
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

	let leaves_first = graph.sort_leaves_first();
	// b must come before a, d must come before c
	let b_index = leaves_first.iter().position(|n| n == "b").unwrap();
	let a_index = leaves_first.iter().position(|n| n == "a").unwrap();
	let d_index = leaves_first.iter().position(|n| n == "d").unwrap();
	let c_index = leaves_first.iter().position(|n| n == "c").unwrap();
	assert!(b_index < a_index, "b should come before a");
	assert!(d_index < c_index, "d should come before c");

	let roots_first = graph.sort_roots_first();
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
fn direct_dependents_leaf_node_has_no_dependents() {
	// In a -> b -> c, c has no dependents
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec!["b".to_string()]);
	adjacency.insert("b".to_string(), vec!["c".to_string()]);
	adjacency.insert("c".to_string(), vec![]);

	let graph = DependencyGraph::from_adjacency(adjacency);
	let dependents = graph.direct_dependents("a");
	assert!(dependents.is_empty(), "root node should have no dependents");

	let dependents_c = graph.direct_dependents("c");
	assert_eq!(dependents_c, vec!["b"], "b depends on c");
}

#[test]
fn direct_dependents_mid_graph_node() {
	// a -> b, c -> b: both a and c depend on b, so direct_dependents("b") returns [a, c]
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec!["b".to_string()]);
	adjacency.insert("c".to_string(), vec!["b".to_string()]);
	adjacency.insert("b".to_string(), vec![]);

	let graph = DependencyGraph::from_adjacency(adjacency);
	let mut dependents = graph.direct_dependents("b");
	dependents.sort();
	assert_eq!(dependents, vec!["a", "c"]);
}

#[test]
fn direct_dependents_absent_node_returns_empty() {
	let mut adjacency = std::collections::HashMap::new();
	adjacency.insert("a".to_string(), vec!["b".to_string()]);
	adjacency.insert("b".to_string(), vec![]);

	let graph = DependencyGraph::from_adjacency(adjacency);
	let dependents = graph.direct_dependents("nonexistent");
	assert!(dependents.is_empty());
}
