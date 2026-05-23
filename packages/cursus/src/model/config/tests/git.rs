use crate::model::config::git::*;

#[test]
fn git_config_defaults() {
	let config = GitConfig::default();
	assert!(!config.enabled());
	assert_eq!(config.strategy(), Strategy::Push);
	assert_eq!(config.tag_format, TagFormat::Auto);
}

#[test]
fn git_config_deserializes_defaults_when_empty() {
	let config: GitConfig = toml::from_str("").unwrap();
	assert!(!config.enabled());
	assert_eq!(config.strategy(), Strategy::Push);
	assert_eq!(config.tag_format, TagFormat::Auto);
}

#[test]
fn git_config_deserializes_enabled_true() {
	let config: GitConfig = toml::from_str("enabled = true").unwrap();
	assert!(config.enabled());
}

#[test]
fn git_config_deserializes_enabled_false() {
	let config: GitConfig = toml::from_str("enabled = false").unwrap();
	assert!(!config.enabled());
}

#[test]
fn git_config_deserializes_strategy_push() {
	let config: GitConfig = toml::from_str("strategy = \"push\"").unwrap();
	assert_eq!(config.strategy(), Strategy::Push);
}

#[test]
fn git_config_deserializes_strategy_branch() {
	let config: GitConfig = toml::from_str("strategy = \"branch\"").unwrap();
	assert_eq!(config.strategy(), Strategy::Branch);
}

#[test]
fn git_config_deserializes_release_branch_prefix() {
	let config: GitConfig = toml::from_str("release_branch_prefix = \"releases/\"").unwrap();
	assert_eq!(config.release_branch_prefix(), "releases/");
}

#[test]
fn git_config_deserializes_tag_format_prefixed() {
	let config: GitConfig = toml::from_str("tag_format = \"prefixed\"").unwrap();
	assert_eq!(config.tag_format, TagFormat::Prefixed);
}

#[test]
fn git_config_deserializes_tag_format_simple() {
	let config: GitConfig = toml::from_str("tag_format = \"simple\"").unwrap();
	assert_eq!(config.tag_format, TagFormat::Simple);
}

#[test]
fn git_config_deserializes_tag_format_auto() {
	let config: GitConfig = toml::from_str("tag_format = \"auto\"").unwrap();
	assert_eq!(config.tag_format, TagFormat::Auto);
}

#[test]
fn git_config_extra_files_defaults_to_empty() {
	let config = GitConfig::default();
	assert!(config.extra_files.is_empty());
}

#[test]
fn git_config_deserializes_extra_files() {
	let config: GitConfig =
		toml::from_str("extra_files = [\"custom.lock\", \"dist/manifest.json\"]").unwrap();
	assert_eq!(
		config.extra_files,
		vec!["custom.lock", "dist/manifest.json"]
	);
}

#[test]
fn signed_commits_mode_defaults_to_auto() {
	let config = GitConfig::default();
	assert_eq!(config.signed_commits, SignedCommitsMode::Auto);
}

#[test]
fn signed_commits_mode_deserializes_all_variants() {
	let auto: GitConfig = toml::from_str("signed_commits = \"auto\"").unwrap();
	assert_eq!(auto.signed_commits, SignedCommitsMode::Auto);

	let force: GitConfig = toml::from_str("signed_commits = \"force\"").unwrap();
	assert_eq!(force.signed_commits, SignedCommitsMode::Force);

	let off: GitConfig = toml::from_str("signed_commits = \"off\"").unwrap();
	assert_eq!(off.signed_commits, SignedCommitsMode::Off);
}

#[test]
fn signed_commits_mode_rejects_unknown_variant() {
	let result: Result<GitConfig, _> = toml::from_str("signed_commits = \"always\"");
	assert!(
		result.is_err(),
		"Expected error for unknown signed_commits variant"
	);
}

#[test]
fn signed_commits_mode_auto_omitted_on_serialize() {
	let config = GitConfig::default();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		!toml_str.contains("signed_commits"),
		"Auto (default) should not appear in serialized config, got: {toml_str}"
	);
}

#[test]
fn signed_commits_mode_force_serialized() {
	let config = GitConfig {
		signed_commits: SignedCommitsMode::Force,
		..Default::default()
	};
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		toml_str.contains("signed_commits = \"force\""),
		"Force mode should be serialized, got: {toml_str}"
	);
}

#[test]
fn git_config_rejects_unknown_fields() {
	let result: Result<GitConfig, _> = toml::from_str("unknown_field = true");
	assert!(result.is_err(), "Expected error for unknown field");
}

#[test]
fn git_config_rejects_old_run_until_field() {
	// run_until was removed; configs with this field must fail to parse.
	let result: Result<GitConfig, _> = toml::from_str("run_until = \"push\"");
	assert!(
		result.is_err(),
		"Expected error for removed run_until field"
	);
}

#[test]
fn git_config_roundtrip() {
	let config = GitConfig::enabled_config()
		.with_strategy(Strategy::Branch)
		.with_release_branch_prefix("release/".to_string())
		.with_tag_format(TagFormat::Prefixed)
		.with_extra_files(vec!["custom.lock".to_string()])
		.with_prepare_commit_message("chore: bump versions".to_string())
		.with_publish_private_packages(vec!["my-action".to_string()]);
	let toml_str = toml::to_string(&config).unwrap();
	let deserialized: GitConfig = toml::from_str(&toml_str).unwrap();
	assert_eq!(config, deserialized);
}

#[test]
fn git_config_publish_private_packages_defaults_to_empty() {
	let config = GitConfig::default();
	assert!(config.publish_private_packages().is_empty());
}

#[test]
fn git_config_deserializes_publish_private_packages() {
	let config: GitConfig =
		toml::from_str("publish_private_packages = [\"my-action\", \"other-action\"]").unwrap();
	assert_eq!(
		config.publish_private_packages(),
		&["my-action", "other-action"]
	);
}

#[test]
fn git_config_publish_private_packages_omitted_when_empty() {
	let config = GitConfig::default();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		!toml_str.contains("publish_private_packages"),
		"Empty list should not be serialized, got: {toml_str}"
	);
}

#[test]
fn git_config_publish_private_packages_serialized_when_set() {
	let config = GitConfig::default().with_publish_private_packages(vec!["my-action".to_string()]);
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		toml_str.contains("publish_private_packages"),
		"Non-empty list should be serialized, got: {toml_str}"
	);
}

#[test]
fn git_config_with_publish_private_packages_returns_modified_self() {
	let pkgs = vec!["my-action".to_string()];
	let config = GitConfig::default().with_publish_private_packages(pkgs.clone());
	assert_eq!(config.publish_private_packages(), pkgs.as_slice());
}

#[test]
fn git_config_serializes_enabled_true() {
	let config = GitConfig::enabled_config();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(toml_str.contains("enabled = true"));
}

#[test]
fn git_config_serializes_omits_enabled_when_none() {
	let config = GitConfig::default();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		!toml_str.contains("enabled"),
		"None enabled should be omitted"
	);
}

#[test]
fn git_config_serializes_omits_strategy_when_none() {
	let config = GitConfig::default();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		!toml_str.contains("strategy"),
		"None strategy should be omitted"
	);
}

#[test]
fn git_config_serializes_omits_release_branch_prefix_when_none() {
	let config = GitConfig::default();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		!toml_str.contains("release_branch_prefix"),
		"None release_branch_prefix should be omitted"
	);
}

#[test]
fn git_config_enabled_config_returns_enabled() {
	let config = GitConfig::enabled_config();
	assert!(config.enabled());
}

#[test]
fn git_config_resolve_defaults_sets_enabled_when_github_enabled() {
	let mut config = GitConfig::default();
	config.resolve_defaults(true);
	assert!(config.enabled());
}

#[test]
fn git_config_resolve_defaults_does_not_override_explicit_enabled_false() {
	let mut config = GitConfig {
		enabled: Some(false),
		..Default::default()
	};
	config.resolve_defaults(true);
	assert!(!config.enabled());
}

#[test]
fn git_config_resolve_defaults_sets_branch_strategy_when_github_enabled() {
	let mut config = GitConfig::default();
	config.resolve_defaults(true);
	assert_eq!(config.strategy(), Strategy::Branch);
}

#[test]
fn git_config_resolve_defaults_sets_push_strategy_when_github_disabled() {
	let mut config = GitConfig::default();
	config.resolve_defaults(false);
	assert_eq!(config.strategy(), Strategy::Push);
}

#[test]
fn git_config_resolve_defaults_does_not_override_explicit_strategy() {
	let mut config = GitConfig::default().with_strategy(Strategy::Push);
	config.resolve_defaults(true);
	assert_eq!(config.strategy(), Strategy::Push);
}

#[test]
fn git_config_release_branch_prefix_defaults_to_constant() {
	let config = GitConfig::default();
	assert_eq!(config.release_branch_prefix(), "cursus-release/");
}

#[test]
fn git_config_with_release_branch_prefix_overrides_default() {
	let config = GitConfig::default().with_release_branch_prefix("release/".to_string());
	assert_eq!(config.release_branch_prefix(), "release/");
}

#[test]
fn git_config_with_tag_format_returns_modified_self() {
	let config = GitConfig::default().with_tag_format(TagFormat::Prefixed);
	assert_eq!(
		config.tag_format,
		TagFormat::Prefixed,
		"with_tag_format should set tag_format on self"
	);
}

#[test]
fn git_config_with_extra_files_returns_modified_self() {
	let files = vec!["custom.lock".to_string(), "dist/manifest.json".to_string()];
	let config = GitConfig::default().with_extra_files(files.clone());
	assert_eq!(
		config.extra_files, files,
		"with_extra_files should set extra_files on self"
	);
}

#[test]
fn prepare_commit_message_defaults_to_constant() {
	let config = GitConfig::default();
	assert_eq!(
		config.prepare_commit_message(),
		"ci(release): version packages"
	);
}

#[test]
fn prepare_commit_message_respects_config_value() {
	let config =
		GitConfig::default().with_prepare_commit_message("chore: bump versions".to_string());
	assert_eq!(config.prepare_commit_message(), "chore: bump versions");
}

#[test]
fn prepare_commit_message_serializes_when_set() {
	let config =
		GitConfig::default().with_prepare_commit_message("chore: bump versions".to_string());
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		toml_str.contains("prepare_commit_message = \"chore: bump versions\""),
		"Custom message should be serialized, got: {toml_str}"
	);
}

#[test]
fn prepare_commit_message_omitted_when_not_set() {
	let config = GitConfig::default();
	let toml_str = toml::to_string(&config).unwrap();
	assert!(
		!toml_str.contains("prepare_commit_message"),
		"Default message should not be serialized, got: {toml_str}"
	);
}

#[test]
fn prepare_commit_message_deserializes() {
	let config: GitConfig = toml::from_str("prepare_commit_message = \"ci: release\"").unwrap();
	assert_eq!(config.prepare_commit_message(), "ci: release");
}

#[test]
fn tag_format_auto_single_package() {
	let version = semver::Version::new(1, 2, 3);
	assert_eq!(TagFormat::Auto.tag("my-pkg", &version, false), "v1.2.3");
}

#[test]
fn tag_format_auto_multi_package() {
	let version = semver::Version::new(1, 2, 3);
	assert_eq!(
		TagFormat::Auto.tag("my-pkg", &version, true),
		"my-pkg@1.2.3"
	);
}

#[test]
fn tag_format_prefixed_single_package() {
	let version = semver::Version::new(1, 2, 3);
	assert_eq!(
		TagFormat::Prefixed.tag("my-pkg", &version, false),
		"my-pkg@1.2.3"
	);
}

#[test]
fn tag_format_prefixed_multi_package() {
	let version = semver::Version::new(1, 2, 3);
	assert_eq!(
		TagFormat::Prefixed.tag("my-pkg", &version, true),
		"my-pkg@1.2.3"
	);
}

#[test]
fn tag_format_simple_single_package() {
	let version = semver::Version::new(1, 2, 3);
	assert_eq!(TagFormat::Simple.tag("my-pkg", &version, false), "v1.2.3");
}

#[test]
fn tag_format_simple_multi_package() {
	let version = semver::Version::new(1, 2, 3);
	assert_eq!(TagFormat::Simple.tag("my-pkg", &version, true), "v1.2.3");
}
