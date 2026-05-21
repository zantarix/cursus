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
pub(crate) fn build_reverse_dep_graph(projects: &[Project]) -> BTreeMap<String, Vec<String>> {
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
pub(crate) fn mark_propagation_bumps(
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
