//! Linked package versions configuration.

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

/// Configuration for keeping groups of packages at the same version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct LinkedVersionsConfig {
	/// Explicitly enable or disable linked versioning.
	///
	/// - `Some(false)` — always disabled, regardless of groups
	/// - `Some(true)` with no groups — global linking (all packages share one version)
	/// - `None` with non-empty groups — enabled (groups take effect)
	/// - `None` with empty groups — disabled
	pub enabled: Option<bool>,
	/// Groups of packages whose versions should be kept in sync.
	pub groups: Vec<LinkedVersionGroup>,
}

/// A group of packages whose versions must stay in sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedVersionGroup {
	/// Glob patterns matching package names in this group.
	pub packages: Vec<String>,
}

/// Resolves a single linked-versions group to the list of matching package names.
///
/// Updates `assigned` to track which group each package was assigned to, so that
/// overlap can be detected across groups.
fn resolve_one_group(
	group_idx: usize,
	group: &LinkedVersionGroup,
	project_names: &[&str],
	assigned: &mut std::collections::HashMap<String, usize>,
) -> anyhow::Result<Vec<String>> {
	if group.packages.is_empty() {
		bail!(
			"linked-versions group {} has an empty 'packages' array",
			group_idx + 1
		);
	}
	let mut matched: Vec<String> = Vec::new();
	let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
	for pattern_str in &group.packages {
		let pattern = glob::Pattern::new(pattern_str)
			.with_context(|| format!("Invalid glob pattern: {pattern_str}"))?;
		let mut any_match = false;
		for name in project_names {
			if !pattern.matches(name) {
				continue;
			}
			any_match = true;
			if let Some(prev) = assigned.get(*name) {
				bail!(
					"Package '{name}' matches patterns in multiple linked-versions \
					 groups (groups {} and {})",
					prev + 1,
					group_idx + 1
				);
			}
			if seen.insert(*name) {
				matched.push(name.to_string());
			}
		}
		if !any_match {
			log::warn!(
				"linked-versions pattern '{pattern_str}' in group {} matches no packages",
				group_idx + 1
			);
		}
	}
	for name in &matched {
		assigned.insert(name.clone(), group_idx);
	}
	Ok(matched)
}

impl LinkedVersionsConfig {
	/// Returns `true` if linked versioning is active.
	pub fn is_enabled(&self) -> bool {
		match self.enabled {
			Some(false) => false,
			Some(true) => true,
			None => !self.groups.is_empty(),
		}
	}

	/// Returns `true` if all packages should be linked globally (no groups defined).
	pub fn is_global(&self) -> bool {
		self.is_enabled() && self.groups.is_empty()
	}

	/// Resolves glob patterns in each group to actual package names.
	///
	/// Returns a list of groups, each group being a list of matched package names.
	/// Returns an empty list when linking is disabled.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - A group has an empty `packages` array
	/// - A package name matches patterns in more than one group
	pub fn resolve_groups(&self, project_names: &[&str]) -> anyhow::Result<Vec<Vec<String>>> {
		if !self.is_enabled() {
			return Ok(Vec::new());
		}
		if self.is_global() {
			return Ok(vec![project_names.iter().map(|s| s.to_string()).collect()]);
		}
		let mut resolved: Vec<Vec<String>> = Vec::new();
		let mut assigned: std::collections::HashMap<String, usize> =
			std::collections::HashMap::new();
		for (group_idx, group) in self.groups.iter().enumerate() {
			let matched = resolve_one_group(group_idx, group, project_names, &mut assigned)?;
			if !matched.is_empty() {
				resolved.push(matched);
			}
		}
		Ok(resolved)
	}
}
