use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

use anyhow::Context;
use log::info;

use crate::model::changeset::{ChangeType, Changeset};
use crate::model::config::DependencyBump;
use crate::package_manager::Project;

use super::version::effective_new_version;
use super::{PropagationMap, PropagationResult};

/// Builds a reverse dependency graph for intra-workspace dependencies.
///
/// Returns a map from each package name to the list of packages that depend on it.
pub(super) fn build_reverse_dep_graph(projects: &[Project]) -> BTreeMap<String, Vec<String>> {
	let project_names: BTreeSet<String> = projects.iter().map(|p| p.name().to_string()).collect();
	let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
	for project in projects {
		for dep_name in project.dependency_names() {
			if project_names.contains(dep_name.as_str()) {
				reverse_deps
					.entry(dep_name.clone())
					.or_default()
					.push(project.name().to_string());
			}
		}
	}
	reverse_deps
}

/// Phase 1 of dependency propagation: marks all transitively dependent packages.
///
/// Starting from the initially-bumped set (`aggregated`), traverses the reverse
/// dependency graph and returns a map of `pkg_name → (effective_ct, [upstream_names])`.
/// Packages in `version_overrides` (linked-version bumps) are exempt.
pub(super) fn mark_propagation_bumps(
	aggregated: &BTreeMap<String, ChangeType>,
	version_overrides: &BTreeMap<String, semver::Version>,
	reverse_deps: &BTreeMap<String, Vec<String>>,
	dep_bump: DependencyBump,
) -> PropagationMap {
	let mut queue: VecDeque<(String, ChangeType)> = aggregated
		.iter()
		.map(|(name, &ct)| (name.clone(), ct))
		.collect();
	let mut propagation_map: PropagationMap = BTreeMap::new();

	while let Some((bumped_name, upstream_ct)) = queue.pop_front() {
		let effective_ct = dep_bump.to_change_type(upstream_ct);
		let Some(dependents) = reverse_deps.get(&bumped_name) else {
			continue;
		};
		for dependent_name in dependents {
			if version_overrides.contains_key(dependent_name.as_str()) {
				continue; // Linked packages are exempt from propagation.
			}
			let current_ct = aggregated
				.get(dependent_name.as_str())
				.copied()
				.or_else(|| {
					propagation_map
						.get(dependent_name.as_str())
						.map(|(ct, _)| *ct)
				});
			if current_ct.is_some_and(|c| c >= effective_ct) {
				continue; // Already at a sufficient bump level.
			}
			let entry = propagation_map
				.entry(dependent_name.clone())
				.or_insert_with(|| (effective_ct, BTreeSet::new()));
			entry.0 = effective_ct;
			entry.1.insert(bumped_name.clone());
			queue.push_back((dependent_name.clone(), effective_ct));
		}
	}
	propagation_map
}

/// Writes or logs a changeset for an out-of-scope dependent package.
pub(super) async fn write_out_of_scope_changeset(
	pkg_name: &str,
	effective_ct: ChangeType,
	dep_msgs: &[String],
	env: &crate::Env,
	dry_run: bool,
) -> anyhow::Result<Option<PathBuf>> {
	let message = format!("Dependency updates: {}", dep_msgs.join(", "));
	let mut packages = BTreeMap::new();
	packages.insert(pkg_name.to_string(), effective_ct);
	let changeset = Changeset::new(packages, Some(message));
	if dry_run {
		info!(
			"Would write dependency propagation changeset for \
			 out-of-scope package '{pkg_name}' ({effective_ct})"
		);
		return Ok(None);
	}
	let path = changeset
		.write(env.git(), env.fs())
		.await
		.with_context(|| format!("Failed to write propagation changeset for '{pkg_name}'"))?;
	info!(
		"Wrote dependency propagation changeset for '{pkg_name}': {}",
		path.display()
	);
	Ok(Some(path))
}

/// Applies dependency propagation bumps (ADR-023).
///
/// Walks the intra-workspace dependency graph using a two-phase mark-then-sweep
/// algorithm. In-scope packages have their entry in `aggregated` updated; out-of-scope
/// dependents receive a newly written changeset file in `.cursus/`.
///
/// Returns `(dep_entries_per_package, new_changeset_paths)` where:
/// - `dep_entries_per_package`: human-readable dependency update messages per in-scope
///   package, for rendering in the `### Dependencies` changelog section.
/// - `new_changeset_paths`: paths of changeset files written for out-of-scope packages.
///
/// # Errors
///
/// Returns an error if writing a changeset file for an out-of-scope dependent fails.
pub(super) async fn apply_dependency_propagation(
	projects: &[Project],
	aggregated: &mut BTreeMap<String, ChangeType>,
	version_overrides: &BTreeMap<String, semver::Version>,
	package_filter: &[String],
	dep_bump: DependencyBump,
	env: &crate::Env,
	dry_run: bool,
) -> anyhow::Result<PropagationResult> {
	let reverse_deps = build_reverse_dep_graph(projects);
	let propagation_map =
		mark_propagation_bumps(aggregated, version_overrides, &reverse_deps, dep_bump);
	if propagation_map.is_empty() {
		return Ok((BTreeMap::new(), Vec::new()));
	}

	let mut dep_entries: BTreeMap<String, Vec<String>> = BTreeMap::new();
	let mut new_changeset_paths: Vec<PathBuf> = Vec::new();

	for (pkg_name, (effective_ct, upstream_names)) in &propagation_map {
		let dep_msgs: Vec<String> = upstream_names
			.iter()
			.map(|up| {
				match effective_new_version(
					up,
					projects,
					aggregated,
					version_overrides,
					&propagation_map,
				) {
					Some(v) => format!("`{up}` bumped to {v}"),
					None => format!("`{up}` bumped"),
				}
			})
			.collect();

		if package_filter.is_empty() || package_filter.contains(pkg_name) {
			let existing_ct = aggregated.get(pkg_name.as_str()).copied();
			if existing_ct.is_none_or(|c| c < *effective_ct) {
				aggregated.insert(pkg_name.clone(), *effective_ct);
				dep_entries.insert(pkg_name.clone(), dep_msgs);
				info!(
					"{pkg_name}: dependency propagation bump ({effective_ct}) from {}",
					upstream_names
						.iter()
						.cloned()
						.collect::<Vec<_>>()
						.join(", ")
				);
			}
		} else if let Some(path) =
			write_out_of_scope_changeset(pkg_name, *effective_ct, &dep_msgs, env, dry_run).await?
		{
			new_changeset_paths.push(path);
		}
	}

	Ok((dep_entries, new_changeset_paths))
}

#[cfg(test)]
mod tests {
	use super::*;

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
}
