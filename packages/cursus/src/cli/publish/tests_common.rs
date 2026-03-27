//! Shared test helpers for publish submodule tests.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::filesystem::LocalFilesystem;
use crate::model::config::GitHubConfig;

/// Builds a GitHub config with GitHub enabled, using known owner/repo to avoid git detection.
pub(super) fn make_github_config(
	build_command: &str,
	artifacts: BTreeMap<String, String>,
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

/// Creates a test `Env` pointing at the given directory.
pub(super) fn make_test_env(dir: &std::path::Path) -> crate::Env {
	let r = Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>;
	let path = crate::path::AbsolutePath::new(dir).unwrap();
	crate::Env::new(
		Arc::clone(&r),
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(r, path)),
	)
}

/// Creates a test `Env` pointing at `/tmp` (matching `workdir()`).
pub(super) fn workdir_env() -> crate::Env {
	make_test_env(std::path::Path::new("/tmp"))
}
