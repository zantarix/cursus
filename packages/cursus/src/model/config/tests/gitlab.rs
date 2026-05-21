use std::collections::BTreeMap;

use crate::model::config::gitlab::*;

#[test]
fn gitlab_config_defaults() {
	let config = GitLabConfig::default();
	assert!(!config.enabled);
	assert_eq!(config.group(), None);
	assert_eq!(config.project(), None);
	assert!(config.host.is_empty());
	assert!(config.build_command.is_empty());
	assert!(config.artifacts.is_empty());
}

#[test]
fn gitlab_config_deserializes_empty() {
	let config: GitLabConfig = toml::from_str("").unwrap();
	assert!(!config.enabled);
	assert_eq!(config.group(), None);
	assert_eq!(config.project(), None);
}

#[test]
fn gitlab_config_deserializes_enabled_only() {
	let config: GitLabConfig = toml::from_str("enabled = true").unwrap();
	assert!(config.enabled);
}

#[test]
fn gitlab_config_deserializes_all_fields() {
	let toml_str = r#"
enabled = true
group = "acme"
project = "my-app"
host = "https://gitlab.example.com"
build_command = "cargo build --release"
merge_request_title = "Release MR"
[artifacts.my-app]
"linux-amd64" = "target/release/app"
"#;
	let config: GitLabConfig = toml::from_str(toml_str).unwrap();
	assert!(config.enabled);
	assert_eq!(config.group(), Some("acme"));
	assert_eq!(config.project(), Some("my-app"));
	assert_eq!(config.host, "https://gitlab.example.com");
	assert_eq!(config.build_command, "cargo build --release");
	assert_eq!(config.merge_request_title(), "Release MR");
	assert_eq!(
		config
			.artifacts
			.get("my-app")
			.and_then(|m| m.get("linux-amd64"))
			.map(|s| s.as_str()),
		Some("target/release/app")
	);
}

#[test]
fn gitlab_config_subgroup_paths_are_accepted() {
	let config: GitLabConfig =
		toml::from_str("group = \"acme/subgroup\"\nproject = \"my-app\"").unwrap();
	assert_eq!(config.group(), Some("acme/subgroup"));
	assert_eq!(config.project(), Some("my-app"));
}

#[test]
fn gitlab_config_rejects_unknown_fields() {
	let result: Result<GitLabConfig, _> = toml::from_str("unknown_field = true");
	assert!(result.is_err(), "Expected error for unknown field");
}

#[test]
fn gitlab_config_rejects_github_keys() {
	// `owner` and `repo` are GitHub vocabulary; the GitLab schema must not silently accept them.
	let result_owner: Result<GitLabConfig, _> = toml::from_str("owner = \"acme\"");
	assert!(
		result_owner.is_err(),
		"`owner` is GitHub vocabulary and should be rejected"
	);
	let result_repo: Result<GitLabConfig, _> = toml::from_str("repo = \"my-app\"");
	assert!(
		result_repo.is_err(),
		"`repo` is GitHub vocabulary and should be rejected"
	);
	let result_pr: Result<GitLabConfig, _> = toml::from_str("pull_request_title = \"Release\"");
	assert!(
		result_pr.is_err(),
		"`pull_request_title` is GitHub vocabulary and should be rejected"
	);
}

#[test]
fn gitlab_config_roundtrip() {
	let mut pkg_artifacts = BTreeMap::new();
	pkg_artifacts.insert("linux".to_string(), "target/app".to_string());
	let mut artifacts = BTreeMap::new();
	artifacts.insert("my-pkg".to_string(), pkg_artifacts);
	let config = GitLabConfig {
		enabled: true,
		host: "https://gitlab.example.com".to_string(),
		build_command: "make release".to_string(),
		artifacts,
		..Default::default()
	}
	.with_group("acme".to_string())
	.with_project("my-app".to_string())
	.with_merge_request_title("Release MR".to_string());
	let toml_str = toml::to_string(&config).unwrap();
	let deserialized: GitLabConfig = toml::from_str(&toml_str).unwrap();
	assert_eq!(config, deserialized);
}

#[test]
fn gitlab_config_serializes_skips_none_and_empty() {
	let config = GitLabConfig::default();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(!toml_str.contains("group"), "None group should be omitted");
	assert!(
		!toml_str.contains("project"),
		"None project should be omitted"
	);
	assert!(!toml_str.contains("host"), "Empty host should be omitted");
	assert!(
		!toml_str.contains("build_command"),
		"Empty build_command should be omitted"
	);
	assert!(
		!toml_str.contains("artifacts"),
		"Empty artifacts should be omitted"
	);
	assert!(
		!toml_str.contains("merge_request_title"),
		"None merge_request_title should be omitted"
	);
}

#[test]
fn gitlab_config_merge_request_title_defaults_to_constant() {
	let config = GitLabConfig::default();
	assert_eq!(config.merge_request_title(), "Release updates");
}
