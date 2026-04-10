//! Shared test helpers for publish submodule tests.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
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

/// Creates a [`GitWorkdir`] rooted at `dir`, driven by a clone of `runner`.
pub(super) fn git_from_dir<R: CommandRunner + 'static>(
	runner: &Arc<R>,
	dir: &std::path::Path,
) -> crate::git::GitWorkdir {
	let dir_abs = crate::path::AbsolutePath::new(dir).unwrap();
	crate::git::GitWorkdir::new(Arc::clone(runner) as Arc<dyn CommandRunner>, dir_abs)
}

/// Creates a [`RecordingCommandRunner`] wrapped in an `Arc`.
pub(super) fn recording_runner(exit_code: i32) -> Arc<RecordingCommandRunner> {
	Arc::new(RecordingCommandRunner::new(exit_code))
}

/// Builds a nested artifact map: `{ pkg => { name => path } }`.
///
/// Each element of `files` is `(artifact_name, file_path)`.
pub(super) fn artifact_map(
	pkg: &str,
	files: &[(&str, &std::path::Path)],
) -> BTreeMap<String, BTreeMap<String, String>> {
	BTreeMap::from([(
		pkg.to_string(),
		files
			.iter()
			.map(|(name, path)| (name.to_string(), path.to_string_lossy().into_owned()))
			.collect(),
	)])
}
