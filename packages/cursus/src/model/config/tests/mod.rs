use std::sync::Arc;

pub(super) use super::*;
pub(super) use crate::command::CommandRunner;
pub(super) use crate::command::test_support::RecordingCommandRunner;
use crate::filesystem::LocalFilesystem;
use crate::path::AbsolutePath;

pub(super) fn temp_dir() -> tempfile::TempDir {
	tempfile::tempdir().expect("Failed to create temp dir")
}

/// Creates a test `Env` with git set to the given directory path.
pub(super) fn make_env_with_git(dir: &std::path::Path) -> crate::Env {
	let runner = Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>;
	let path = AbsolutePath::new(dir).unwrap();
	crate::Env::new(
		Arc::clone(&runner),
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(runner, path)),
	)
}

pub(super) mod builder;
pub(super) mod cargo;
pub(super) mod git;
pub(super) mod github;
pub(super) mod gitlab;
pub(super) mod ignore;
pub(super) mod linked_versions;
pub(super) mod load_projects;
pub(super) mod npm;
pub(super) mod persistence;
pub(super) mod prepare;
pub(super) mod serialization;
pub(super) mod template;
pub(super) mod workspace_version;
