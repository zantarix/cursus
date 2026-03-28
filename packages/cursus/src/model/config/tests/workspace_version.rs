use crate::model::config::{LinkedVersionGroup, LinkedVersionsConfig};
use crate::package_manager::Project;

fn ws_project(name: &str) -> Project {
	Project::new_test(name, &format!("/tmp/{name}")).with_workspace_version(true)
}

fn regular_project(name: &str) -> Project {
	Project::new_test(name, &format!("/tmp/{name}"))
}

fn linked_global() -> LinkedVersionsConfig {
	LinkedVersionsConfig {
		enabled: Some(true),
		groups: vec![],
	}
}

fn linked_disabled() -> LinkedVersionsConfig {
	LinkedVersionsConfig::default()
}

fn linked_with_group(patterns: Vec<&str>) -> LinkedVersionsConfig {
	LinkedVersionsConfig {
		enabled: None,
		groups: vec![LinkedVersionGroup {
			packages: patterns.into_iter().map(String::from).collect(),
		}],
	}
}

fn linked_with_groups(groups: Vec<Vec<&str>>) -> LinkedVersionsConfig {
	LinkedVersionsConfig {
		enabled: None,
		groups: groups
			.into_iter()
			.map(|patterns| LinkedVersionGroup {
				packages: patterns.into_iter().map(String::from).collect(),
			})
			.collect(),
	}
}

#[test]
fn no_workspace_version_projects_always_ok() {
	let projects = vec![regular_project("a"), regular_project("b")];
	let result = super::super::validate_workspace_version_linking(&projects, &linked_disabled());
	assert!(result.is_ok());
}

#[test]
fn global_linking_covers_workspace_version_projects() {
	let projects = vec![ws_project("a"), ws_project("b")];
	let result = super::super::validate_workspace_version_linking(&projects, &linked_global());
	assert!(result.is_ok());
}

#[test]
fn all_workspace_version_projects_in_same_group_ok() {
	let projects = vec![ws_project("a"), ws_project("b"), regular_project("c")];
	let linked = linked_with_group(vec!["a", "b"]);
	let result = super::super::validate_workspace_version_linking(&projects, &linked);
	assert!(result.is_ok());
}

#[test]
fn workspace_version_project_not_in_any_group_errors() {
	let projects = vec![ws_project("a"), regular_project("b")];
	let result = super::super::validate_workspace_version_linking(&projects, &linked_disabled());
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("not in any linked-versions group"),
		"Error should mention missing linked group, got: {msg}"
	);
	assert!(
		msg.contains("a"),
		"Error should name the project, got: {msg}"
	);
}

#[test]
fn workspace_version_projects_across_multiple_groups_errors() {
	let projects = vec![ws_project("a"), ws_project("b"), regular_project("c")];
	let linked = linked_with_groups(vec![vec!["a"], vec!["b"]]);
	let result = super::super::validate_workspace_version_linking(&projects, &linked);
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("spread across"),
		"Error should mention multiple groups, got: {msg}"
	);
}

#[test]
fn single_workspace_version_project_without_linking_errors() {
	let projects = vec![ws_project("a")];
	let result = super::super::validate_workspace_version_linking(&projects, &linked_disabled());
	assert!(result.is_err());
}
