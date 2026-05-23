use std::collections::BTreeMap;

use crate::model::config::github::*;

#[test]
fn github_config_defaults() {
	let config = GitHubConfig::default();
	assert!(!config.enabled);
	assert_eq!(config.owner(), None);
	assert_eq!(config.repo(), None);
	assert!(config.build_command.is_empty());
	assert!(config.artifacts.is_empty());
}

#[test]
fn github_config_deserializes_empty() {
	let config: GitHubConfig = toml::from_str("").unwrap();
	assert!(!config.enabled);
	assert_eq!(config.owner(), None);
	assert_eq!(config.repo(), None);
}

#[test]
fn github_config_deserializes_enabled_only() {
	let config: GitHubConfig = toml::from_str("enabled = true").unwrap();
	assert!(config.enabled);
	assert_eq!(config.owner(), None);
	assert_eq!(config.repo(), None);
}

#[test]
fn github_config_deserializes_all_fields() {
	let toml_str = r#"
enabled = true
owner = "acme"
repo = "my-app"
build_command = "cargo build --release"
[artifacts.my-app]
"linux-amd64" = "target/release/app"
"#;
	let config: GitHubConfig = toml::from_str(toml_str).unwrap();
	assert!(config.enabled);
	assert_eq!(config.owner(), Some("acme"));
	assert_eq!(config.repo(), Some("my-app"));
	assert_eq!(config.build_command, "cargo build --release");
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
fn github_config_rejects_unknown_fields() {
	let result: Result<GitHubConfig, _> = toml::from_str("unknown_field = true");
	assert!(result.is_err(), "Expected error for unknown field");
}

#[test]
fn github_config_rejects_old_flat_artifacts_format() {
	// Pre-ADR-044 configs had `[artifacts]` with string values; verify they produce a clear error.
	let toml_str = r#"
[artifacts]
"linux-amd64" = "target/release/app"
"#;
	let result: Result<GitHubConfig, _> = toml::from_str(toml_str);
	assert!(
		result.is_err(),
		"Old flat artifact format should fail to parse"
	);
	let err_msg = result.unwrap_err().to_string();
	assert!(
		err_msg.contains("invalid type"),
		"Expected a type error for old format, got: {err_msg}"
	);
}

#[test]
fn github_config_roundtrip() {
	let mut pkg_artifacts = BTreeMap::new();
	pkg_artifacts.insert("linux".to_string(), "target/app".to_string());
	let mut artifacts = BTreeMap::new();
	artifacts.insert("my-pkg".to_string(), pkg_artifacts);
	let config = GitHubConfig {
		enabled: true,
		build_command: "make release".to_string(),
		artifacts,
		..Default::default()
	}
	.with_owner("owner".to_string())
	.with_repo("repo".to_string());
	let toml_str = toml::to_string(&config).unwrap();
	let deserialized: GitHubConfig = toml::from_str(&toml_str).unwrap();
	assert_eq!(config, deserialized);
}

#[test]
fn github_config_serializes_skips_none_and_empty() {
	let config = GitHubConfig::default();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(!toml_str.contains("owner"), "None owner should be omitted");
	assert!(!toml_str.contains("repo"), "None repo should be omitted");
	assert!(
		!toml_str.contains("build_command"),
		"Empty build_command should be omitted"
	);
	assert!(
		!toml_str.contains("artifacts"),
		"Empty artifacts should be omitted"
	);
}

#[test]
fn github_config_serializes_some_owner() {
	let config = GitHubConfig::default().with_owner("myorg".to_string());
	let toml_str = toml::to_string(&config).unwrap();
	assert!(toml_str.contains("owner = \"myorg\""));
}

#[test]
fn github_config_serializes_build_command() {
	let config = GitHubConfig {
		build_command: "make all".to_string(),
		..Default::default()
	};
	let toml_str = toml::to_string(&config).unwrap();
	assert!(toml_str.contains("build_command = \"make all\""));
}

#[test]
fn github_config_pull_request_title_defaults_to_constant() {
	let config = GitHubConfig::default();
	assert_eq!(config.pull_request_title(), "Release updates");
}

#[test]
fn github_config_deserializes_pull_request_title() {
	let config: GitHubConfig = toml::from_str("pull_request_title = \"Release PR\"").unwrap();
	assert_eq!(config.pull_request_title(), "Release PR");
}

#[test]
fn github_config_serializes_omits_pull_request_title_when_none() {
	let config = GitHubConfig::default();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		!toml_str.contains("pull_request_title"),
		"None pull_request_title should be omitted, got: {toml_str}"
	);
}

#[test]
fn github_config_serializes_pull_request_title_when_set() {
	let config = GitHubConfig::default().with_pull_request_title("My Release".to_string());
	let toml_str = toml::to_string(&config).unwrap();
	assert!(toml_str.contains("pull_request_title = \"My Release\""));
}
