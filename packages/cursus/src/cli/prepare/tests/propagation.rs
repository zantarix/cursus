use std::collections::BTreeMap;

use crate::cli::prepare::propagation::*;
use crate::model::changeset::ChangeType;
use crate::model::config::DependencyBump;
use crate::package_manager::Project;

fn make_project(name: &str, version: &str) -> Project {
	crate::package_manager::Project::new_test_with_version(name, version.parse().unwrap())
}

fn make_project_with_deps(name: &str, version: &str, deps: Vec<&str>) -> Project {
	crate::package_manager::Project::new_test_with_deps(name, version, deps)
}

// ── DependencyBump::to_change_type ───────────────────────────────────────

#[test]
fn propagation_change_type_patch_mode_always_returns_patch() {
	for upstream in [ChangeType::Patch, ChangeType::Minor, ChangeType::Major] {
		assert_eq!(
			DependencyBump::Patch.to_change_type(upstream),
			ChangeType::Patch,
		);
	}
}

#[test]
fn propagation_change_type_minor_mode_always_returns_minor() {
	for upstream in [ChangeType::Patch, ChangeType::Minor, ChangeType::Major] {
		assert_eq!(
			DependencyBump::Minor.to_change_type(upstream),
			ChangeType::Minor,
		);
	}
}

#[test]
fn propagation_change_type_major_mode_always_returns_major() {
	for upstream in [ChangeType::Patch, ChangeType::Minor, ChangeType::Major] {
		assert_eq!(
			DependencyBump::Major.to_change_type(upstream),
			ChangeType::Major,
		);
	}
}

#[test]
fn propagation_change_type_match_mode_mirrors_upstream() {
	assert_eq!(
		DependencyBump::Match.to_change_type(ChangeType::Patch),
		ChangeType::Patch,
	);
	assert_eq!(
		DependencyBump::Match.to_change_type(ChangeType::Minor),
		ChangeType::Minor,
	);
	assert_eq!(
		DependencyBump::Match.to_change_type(ChangeType::Major),
		ChangeType::Major,
	);
}

#[test]
fn propagation_change_type_auto_mode_maps_minor_and_patch_to_patch() {
	assert_eq!(
		DependencyBump::Auto.to_change_type(ChangeType::Patch),
		ChangeType::Patch,
	);
	assert_eq!(
		DependencyBump::Auto.to_change_type(ChangeType::Minor),
		ChangeType::Patch,
	);
}

#[test]
fn propagation_change_type_auto_mode_maps_major_to_major() {
	assert_eq!(
		DependencyBump::Auto.to_change_type(ChangeType::Major),
		ChangeType::Major,
	);
}

// ── build_reverse_dep_graph ───────────────────────────────────────────────

#[test]
fn build_reverse_dep_graph_empty_projects_returns_empty() {
	let graph = build_reverse_dep_graph(&[]);
	assert!(graph.is_empty());
}

#[test]
fn build_reverse_dep_graph_no_deps_returns_empty() {
	let projects = vec![
		make_project("pkg-a", "1.0.0"),
		make_project("pkg-b", "1.0.0"),
	];
	let graph = build_reverse_dep_graph(&projects);
	assert!(graph.is_empty());
}

#[test]
fn build_reverse_dep_graph_filters_external_deps() {
	// pkg-a depends on serde (external) and pkg-b (intra-workspace)
	let projects = vec![
		make_project_with_deps("pkg-a", "1.0.0", vec!["serde", "pkg-b"]),
		make_project("pkg-b", "1.0.0"),
	];
	let graph = build_reverse_dep_graph(&projects);
	// Only pkg-b should appear (serde is external)
	assert_eq!(graph.len(), 1);
	assert_eq!(graph["pkg-b"], vec!["pkg-a"]);
}

#[test]
fn build_reverse_dep_graph_multiple_dependents_on_same_package() {
	let projects = vec![
		make_project_with_deps("pkg-a", "1.0.0", vec!["pkg-c"]),
		make_project_with_deps("pkg-b", "1.0.0", vec!["pkg-c"]),
		make_project("pkg-c", "1.0.0"),
	];
	let graph = build_reverse_dep_graph(&projects);
	let mut dependents = graph["pkg-c"].clone();
	dependents.sort();
	assert_eq!(dependents, vec!["pkg-a", "pkg-b"]);
}

// ── mark_propagation_bumps ────────────────────────────────────────────────

#[test]
fn mark_propagation_bumps_empty_aggregated_returns_empty() {
	let aggregated = BTreeMap::new();
	let version_overrides = BTreeMap::new();
	let reverse_deps = BTreeMap::new();
	let result = mark_propagation_bumps(
		&aggregated,
		&version_overrides,
		&reverse_deps,
		DependencyBump::Auto,
	);
	assert!(result.is_empty());
}

#[test]
fn mark_propagation_bumps_skips_linked_packages() {
	let mut aggregated = BTreeMap::new();
	aggregated.insert("pkg-a".to_string(), ChangeType::Major);
	let mut version_overrides = BTreeMap::new();
	version_overrides.insert("pkg-b".to_string(), "2.0.0".parse().unwrap());
	let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
	reverse_deps.insert("pkg-a".to_string(), vec!["pkg-b".to_string()]);
	let result = mark_propagation_bumps(
		&aggregated,
		&version_overrides,
		&reverse_deps,
		DependencyBump::Auto,
	);
	// pkg-b is linked (in version_overrides) so should not be propagated to
	assert!(!result.contains_key("pkg-b"));
}

#[test]
fn mark_propagation_bumps_equal_change_type_does_not_propagate() {
	// pkg-b already has Minor; upstream pkg-a propagates Minor (same level).
	// Guards `>=`→`>` on `current_ct.is_some_and(|c| c >= effective_ct)`:
	// with `>`, an equal ct would NOT be skipped, adding a spurious dep entry.
	let mut aggregated = BTreeMap::new();
	aggregated.insert("pkg-a".to_string(), ChangeType::Minor);
	aggregated.insert("pkg-b".to_string(), ChangeType::Minor);
	let version_overrides = BTreeMap::new();
	let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
	reverse_deps.insert("pkg-a".to_string(), vec!["pkg-b".to_string()]);
	let result = mark_propagation_bumps(
		&aggregated,
		&version_overrides,
		&reverse_deps,
		DependencyBump::Match, // Minor upstream → Minor propagation
	);
	// pkg-b already has Minor (≥ Minor propagation) → must not appear in result
	assert!(
		!result.contains_key("pkg-b"),
		"Equal ct should not create a propagation entry: {result:?}"
	);
}

#[test]
fn mark_propagation_bumps_only_upgrades_not_downgrades() {
	let mut aggregated = BTreeMap::new();
	aggregated.insert("pkg-a".to_string(), ChangeType::Patch);
	// pkg-b already has a Major changeset
	aggregated.insert("pkg-b".to_string(), ChangeType::Major);
	let version_overrides = BTreeMap::new();
	let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
	reverse_deps.insert("pkg-a".to_string(), vec!["pkg-b".to_string()]);
	let result = mark_propagation_bumps(
		&aggregated,
		&version_overrides,
		&reverse_deps,
		DependencyBump::Auto, // Patch upstream → Patch propagation
	);
	// pkg-b already has Major, propagation would be Patch → should not appear
	assert!(!result.contains_key("pkg-b"));
}

#[test]
fn mark_propagation_bumps_diamond_graph_no_duplicate_upstreams() {
	// Two aggregated packages feed into pkg-b at different bump levels, causing
	// pkg-b to be re-enqueued at the higher level. This means pkg-b is processed
	// twice from the BFS queue, and without BTreeSet it would push itself into
	// pkg-d's upstream list twice. With BTreeSet, .insert() is idempotent.
	//
	//   pkg-a (Minor) ──┐
	//                   ▼
	//   pkg-x (Major) ──► pkg-b ──► pkg-d
	//
	// BFS with DependencyBump::Match:
	//   (pkg-a, Minor) → pkg-b gets (Minor, {"pkg-a"}), enqueue (pkg-b, Minor)
	//   (pkg-x, Major) → pkg-b upgraded to (Major, {"pkg-a","pkg-x"}), enqueue (pkg-b, Major)
	//   (pkg-b, Minor) → pkg-d gets (Minor, {"pkg-b"}), enqueue (pkg-d, Minor)
	//   (pkg-b, Major) → pkg-d upgraded to (Major, insert "pkg-b") → still {"pkg-b"} ✓
	let mut aggregated = BTreeMap::new();
	aggregated.insert("pkg-a".to_string(), ChangeType::Minor);
	aggregated.insert("pkg-x".to_string(), ChangeType::Major);
	let version_overrides = BTreeMap::new();
	let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
	reverse_deps.insert("pkg-a".to_string(), vec!["pkg-b".to_string()]);
	reverse_deps.insert("pkg-x".to_string(), vec!["pkg-b".to_string()]);
	reverse_deps.insert("pkg-b".to_string(), vec!["pkg-d".to_string()]);
	let result = mark_propagation_bumps(
		&aggregated,
		&version_overrides,
		&reverse_deps,
		DependencyBump::Match,
	);
	assert!(result.contains_key("pkg-d"));
	let (_, upstreams) = &result["pkg-d"];
	// pkg-b is the sole direct upstream of pkg-d — must appear exactly once
	assert_eq!(upstreams.len(), 1);
	assert!(upstreams.contains("pkg-b"));
}

#[test]
fn mark_propagation_bumps_terminates_with_circular_deps() {
	// A depends on B, B depends on A — cycle.
	// Note: Cargo rejects circular dependencies at the workspace level, so this
	// scenario is more relevant to npm workspaces. This unit test verifies that
	// the BFS algorithm terminates regardless, via idempotent marking.
	let mut aggregated = BTreeMap::new();
	aggregated.insert("pkg-a".to_string(), ChangeType::Minor);
	let version_overrides = BTreeMap::new();
	let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
	reverse_deps.insert("pkg-a".to_string(), vec!["pkg-b".to_string()]);
	reverse_deps.insert("pkg-b".to_string(), vec!["pkg-a".to_string()]);
	// Should terminate (not loop forever) and produce a result
	let result = mark_propagation_bumps(
		&aggregated,
		&version_overrides,
		&reverse_deps,
		DependencyBump::Auto,
	);
	assert!(result.contains_key("pkg-b"));
}
