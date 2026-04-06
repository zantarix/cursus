//! Shared test helpers for publish submodule tests.

use std::collections::BTreeMap;

use crate::model::config::GitHubConfig;

/// Builds a GitHub config with GitHub enabled, using known owner/repo to avoid git detection.
pub(super) fn make_github_config(
	build_command: &str,
	artifacts: BTreeMap<String, BTreeMap<String, String>>,
) -> GitHubConfig {
	let mut config = GitHubConfig::enabled_config();
	config.build_command = build_command.to_string();
	config.artifacts = artifacts;
	config
		.with_owner("acme".to_string())
		.with_repo("app".to_string())
}

pub(super) fn workdir() -> crate::path::AbsolutePath {
	crate::path::AbsolutePath::new("/tmp").unwrap()
}
