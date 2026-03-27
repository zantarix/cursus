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

#[cfg(test)]
mod tests {
	use super::*;

	fn make_config(enabled: Option<bool>, groups: Vec<Vec<&str>>) -> LinkedVersionsConfig {
		LinkedVersionsConfig {
			enabled,
			groups: groups
				.into_iter()
				.map(|pkgs| LinkedVersionGroup {
					packages: pkgs.into_iter().map(str::to_string).collect(),
				})
				.collect(),
		}
	}

	// ── is_enabled ────────────────────────────────────────────────────────────

	#[test]
	fn disabled_when_explicit_false_no_groups() {
		let c = make_config(Some(false), vec![]);
		assert!(!c.is_enabled());
	}

	#[test]
	fn disabled_when_explicit_false_with_groups() {
		let c = make_config(Some(false), vec![vec!["pkg-a"]]);
		assert!(!c.is_enabled());
	}

	#[test]
	fn disabled_when_none_no_groups() {
		let c = make_config(None, vec![]);
		assert!(!c.is_enabled());
	}

	#[test]
	fn enabled_when_explicit_true_no_groups() {
		let c = make_config(Some(true), vec![]);
		assert!(c.is_enabled());
	}

	#[test]
	fn enabled_when_none_with_groups() {
		let c = make_config(None, vec![vec!["pkg-a"]]);
		assert!(c.is_enabled());
	}

	#[test]
	fn default_is_disabled() {
		let c = LinkedVersionsConfig::default();
		assert!(!c.is_enabled());
	}

	// ── is_global ─────────────────────────────────────────────────────────────

	#[test]
	fn global_when_enabled_true_no_groups() {
		let c = make_config(Some(true), vec![]);
		assert!(c.is_global());
	}

	#[test]
	fn not_global_when_groups_present() {
		let c = make_config(Some(true), vec![vec!["pkg-a"]]);
		assert!(!c.is_global());
	}

	#[test]
	fn not_global_when_disabled() {
		let c = make_config(Some(false), vec![]);
		assert!(!c.is_global());
	}

	// ── resolve_groups ────────────────────────────────────────────────────────

	#[test]
	fn resolve_groups_returns_empty_when_disabled() {
		let c = make_config(Some(false), vec![vec!["pkg-*"]]);
		let groups = c.resolve_groups(&["pkg-a", "pkg-b"]).unwrap();
		assert!(groups.is_empty());
	}

	#[test]
	fn resolve_groups_global_returns_single_group_of_all() {
		let c = make_config(Some(true), vec![]);
		let groups = c.resolve_groups(&["pkg-a", "pkg-b", "pkg-c"]).unwrap();
		assert_eq!(groups.len(), 1);
		assert_eq!(groups[0], vec!["pkg-a", "pkg-b", "pkg-c"]);
	}

	#[test]
	fn resolve_groups_glob_matches_prefix() {
		let c = make_config(None, vec![vec!["pkg-*"]]);
		let groups = c.resolve_groups(&["pkg-a", "pkg-b", "other"]).unwrap();
		assert_eq!(groups.len(), 1);
		assert_eq!(groups[0], vec!["pkg-a", "pkg-b"]);
	}

	#[test]
	fn resolve_groups_multiple_groups() {
		let c = make_config(None, vec![vec!["a-*"], vec!["b-*"]]);
		let groups = c.resolve_groups(&["a-1", "a-2", "b-1"]).unwrap();
		assert_eq!(groups.len(), 2);
		assert_eq!(groups[0], vec!["a-1", "a-2"]);
		assert_eq!(groups[1], vec!["b-1"]);
	}

	#[test]
	fn resolve_groups_empty_packages_is_error() {
		let c = make_config(None, vec![vec![]]);
		let result = c.resolve_groups(&["pkg-a"]);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("empty 'packages' array")
		);
	}

	#[test]
	fn resolve_groups_overlap_is_error() {
		let c = make_config(None, vec![vec!["pkg-*"], vec!["*-a"]]);
		let result = c.resolve_groups(&["pkg-a", "pkg-b"]);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("multiple linked-versions groups")
		);
	}

	#[test]
	fn resolve_groups_no_match_warns_but_succeeds() {
		let c = make_config(None, vec![vec!["nonexistent-*"]]);
		let groups = c.resolve_groups(&["pkg-a"]).unwrap();
		// Group with no matches is dropped from results
		assert!(groups.is_empty());
	}

	#[test]
	fn resolve_groups_duplicate_project_names_deduplicated() {
		// project_names with duplicates should not produce duplicate entries
		let c = make_config(None, vec![vec!["pkg-*"]]);
		let groups = c.resolve_groups(&["pkg-a", "pkg-b", "pkg-a"]).unwrap();
		assert_eq!(groups.len(), 1);
		assert_eq!(groups[0], vec!["pkg-a", "pkg-b"]);
	}

	#[test]
	fn resolve_groups_exact_name_matches() {
		let c = make_config(None, vec![vec!["pkg-a", "pkg-b"]]);
		let groups = c.resolve_groups(&["pkg-a", "pkg-b", "pkg-c"]).unwrap();
		assert_eq!(groups.len(), 1);
		assert_eq!(groups[0], vec!["pkg-a", "pkg-b"]);
	}

	// ── serde ─────────────────────────────────────────────────────────────────

	#[test]
	fn deserializes_global_mode() {
		let toml_str = "[linked-versions]\nenabled = true\n";
		let config: toml::Value = toml::from_str(toml_str).unwrap();
		let lv: LinkedVersionsConfig = config["linked-versions"].clone().try_into().unwrap();
		assert_eq!(lv.enabled, Some(true));
		assert!(lv.groups.is_empty());
	}

	#[test]
	fn deserializes_group_mode() {
		let toml_str = "[[linked-versions.groups]]\npackages = [\"pkg-a\", \"pkg-b\"]\n";
		let config: toml::Value = toml::from_str(toml_str).unwrap();
		let lv: LinkedVersionsConfig = config["linked-versions"].clone().try_into().unwrap();
		assert_eq!(lv.enabled, None);
		assert_eq!(lv.groups.len(), 1);
		assert_eq!(lv.groups[0].packages, vec!["pkg-a", "pkg-b"]);
	}

	#[test]
	fn unknown_field_is_rejected() {
		let result: Result<LinkedVersionsConfig, _> =
			toml::from_str("enabled = true\nunknown = 1\n");
		assert!(result.is_err());
	}
}
