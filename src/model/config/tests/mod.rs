use std::sync::Arc;

pub(super) use super::*;
pub(super) use crate::command::CommandRunner;
pub(super) use crate::command::test_support::RecordingCommandRunner;
use crate::filesystem::LocalFilesystem;

pub(super) fn temp_dir() -> tempfile::TempDir {
	tempfile::tempdir().expect("Failed to create temp dir")
}

pub(super) fn make_env() -> crate::Env {
	crate::Env::new(
		Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
	)
}

pub(super) mod builder;
pub(super) mod ignore;
pub(super) mod load_projects;
pub(super) mod persistence;
pub(super) mod serialization;
