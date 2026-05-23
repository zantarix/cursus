use crate::model::config::linked_versions::*;

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
	let result: Result<LinkedVersionsConfig, _> = toml::from_str("enabled = true\nunknown = 1\n");
	assert!(result.is_err());
}
