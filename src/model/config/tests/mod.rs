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

/// Creates a test `Env` with a dummy git pointed at `/tmp`.
///
/// Uses a [`RecordingCommandRunner`] — no real commands execute. Suitable for
/// tests that only need an `Env` for `Config::save()` (which uses the
/// `Config`'s own `git_workdir`, not the one on `Env`).
pub(super) fn make_env() -> crate::Env {
	let runner = Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>;
	let dummy_path = AbsolutePath::new("/tmp").unwrap();
	crate::Env::new(
		Arc::clone(&runner),
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(runner, dummy_path)),
	)
}

pub(super) mod builder;
pub(super) mod ignore;
pub(super) mod load_projects;
pub(super) mod persistence;
pub(super) mod serialization;
