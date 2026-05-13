use super::*;

#[test]
fn config_defaults_all_disabled() {
	let config = Config::new();
	assert!(!config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_npm_does_not_force_enabled() {
	let config = Config::new().with_npm(NpmConfig::default());
	assert!(!config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_cargo_does_not_force_enabled() {
	let config = Config::new().with_cargo(CargoConfig::default());
	assert!(!config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_npm_enabled_enables_npm() {
	let config = Config::new().with_npm(NpmConfig::enabled());
	assert!(config.npm.enabled);
	assert!(!config.cargo.enabled);
}

#[test]
fn config_with_cargo_enabled_enables_cargo() {
	let config = Config::new().with_cargo(CargoConfig::enabled());
	assert!(!config.npm.enabled);
	assert!(config.cargo.enabled);
}

#[test]
fn enabled_package_managers_returns_empty_when_none_enabled() {
	let config = Config::new();
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert!(enabled.is_empty());
}

#[test]
fn enabled_package_managers_returns_npm_when_enabled() {
	let config = Config::new().with_npm(NpmConfig::enabled());
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert_eq!(enabled, vec![PackageManager::Npm]);
}

#[test]
fn enabled_package_managers_returns_cargo_when_enabled() {
	let config = Config::new().with_cargo(CargoConfig::enabled());
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert_eq!(enabled, vec![PackageManager::Cargo]);
}

#[test]
fn enabled_package_managers_returns_both_when_both_enabled() {
	let mut config = Config::new();
	config.npm.enabled = true;
	config.cargo.enabled = true;
	let enabled: Vec<_> = config.enabled_package_managers().collect();
	assert_eq!(enabled, vec![PackageManager::Npm, PackageManager::Cargo]);
}

// ── Forge-helper accessors ────────────────────────────────────────────────────

#[test]
fn forge_enabled_false_when_no_forge_set() {
	let config = Config::new();
	assert!(!config.forge_enabled());
}

#[test]
fn forge_enabled_true_when_github_enabled() {
	let config = Config::new().with_github(GitHubConfig::enabled_config());
	assert!(config.forge_enabled());
}

#[test]
fn forge_enabled_true_when_gitlab_enabled() {
	let config = Config::new().with_gitlab(GitLabConfig::enabled_config());
	assert!(config.forge_enabled());
}

#[test]
fn release_request_title_prefers_github() {
	let github = GitHubConfig::enabled_config().with_pull_request_title("From GitHub".to_string());
	let gitlab = GitLabConfig::enabled_config().with_merge_request_title("From GitLab".to_string());
	let config = Config::new().with_github(github).with_gitlab(gitlab);
	assert_eq!(config.release_request_title(), "From GitHub");
}

#[test]
fn release_request_title_falls_back_to_gitlab() {
	let gitlab = GitLabConfig::enabled_config().with_merge_request_title("From GitLab".to_string());
	let config = Config::new().with_gitlab(gitlab);
	assert_eq!(config.release_request_title(), "From GitLab");
}

#[test]
fn release_request_title_defaults_when_no_forge_enabled() {
	let config = Config::new();
	// Falls through to the GitHub default ("Release updates") when no forge
	// is enabled — callers normally gate on `forge_enabled()` first.
	assert_eq!(config.release_request_title(), "Release updates");
}

#[test]
fn build_command_reads_from_gitlab_when_only_gitlab_enabled() {
	let mut gitlab = GitLabConfig::enabled_config();
	gitlab.build_command = "cargo make gitlab-build".to_string();
	let config = Config::new().with_gitlab(gitlab);
	assert_eq!(config.build_command(), "cargo make gitlab-build");
}

#[test]
fn build_command_prefers_github_when_both_enabled() {
	let mut github = GitHubConfig::enabled_config();
	github.build_command = "cargo make github-build".to_string();
	let mut gitlab = GitLabConfig::enabled_config();
	gitlab.build_command = "cargo make gitlab-build".to_string();
	let config = Config::new().with_github(github).with_gitlab(gitlab);
	assert_eq!(config.build_command(), "cargo make github-build");
}

#[test]
fn build_command_empty_when_no_forge_enabled() {
	let config = Config::new();
	assert_eq!(config.build_command(), "");
}

#[test]
fn forge_artifacts_reads_from_gitlab_when_only_gitlab_enabled() {
	use std::collections::BTreeMap;
	let mut pkg_artifacts = BTreeMap::new();
	pkg_artifacts.insert("linux".to_string(), "target/app".to_string());
	let mut gitlab = GitLabConfig::enabled_config();
	gitlab.artifacts.insert("my-pkg".to_string(), pkg_artifacts);
	let config = Config::new().with_gitlab(gitlab);
	assert_eq!(
		config
			.forge_artifacts()
			.get("my-pkg")
			.and_then(|m| m.get("linux"))
			.map(String::as_str),
		Some("target/app")
	);
}

#[test]
fn forge_artifacts_prefers_github_when_both_enabled() {
	use std::collections::BTreeMap;
	let mut github_pkg = BTreeMap::new();
	github_pkg.insert("linux".to_string(), "target/from-github".to_string());
	let mut github = GitHubConfig::enabled_config();
	github.artifacts.insert("my-pkg".to_string(), github_pkg);
	let mut gitlab_pkg = BTreeMap::new();
	gitlab_pkg.insert("linux".to_string(), "target/from-gitlab".to_string());
	let mut gitlab = GitLabConfig::enabled_config();
	gitlab.artifacts.insert("my-pkg".to_string(), gitlab_pkg);
	let config = Config::new().with_github(github).with_gitlab(gitlab);
	assert_eq!(
		config
			.forge_artifacts()
			.get("my-pkg")
			.and_then(|m| m.get("linux"))
			.map(String::as_str),
		Some("target/from-github")
	);
}

#[test]
fn forge_artifacts_empty_when_no_forge_enabled() {
	let config = Config::new();
	assert!(config.forge_artifacts().is_empty());
}
