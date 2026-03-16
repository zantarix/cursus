use super::*;
use std::sync::Arc;
use tempfile::TempDir;

use crate::command::CommandRunner;
use crate::command::test_support::{DispatchingCommandRunner, RecordingCommandRunner};

pub(super) fn temp_dir() -> TempDir {
	tempfile::tempdir().expect("Failed to create temp dir")
}

pub(super) fn write_package_json(dir: &std::path::Path, content: &str) {
	std::fs::write(dir.join("package.json"), content).unwrap();
}

/// Creates a `NpmAdapter` backed by a recording runner with the given exit code.
pub(super) fn recording_adapter_default(
	config: NpmConfig,
	dir: &std::path::Path,
	exit_code: i32,
) -> NpmAdapter {
	let env =
		crate::Env::new(Arc::new(RecordingCommandRunner::new(exit_code)) as Arc<dyn CommandRunner>);
	NpmAdapter::new(config, crate::path::AbsolutePath::new(dir).unwrap(), env)
}

/// Creates a `NpmAdapter` backed by a recording runner for inspection.
pub(super) fn recording_adapter(
	config: NpmConfig,
	dir: &std::path::Path,
	runner: Arc<RecordingCommandRunner>,
) -> NpmAdapter {
	let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	NpmAdapter::new(config, crate::path::AbsolutePath::new(dir).unwrap(), env)
}

/// Creates a `NpmAdapter` backed by a dispatching runner for inspection.
pub(super) fn dispatching_adapter(
	config: NpmConfig,
	dir: &std::path::Path,
	runner: Arc<DispatchingCommandRunner>,
) -> NpmAdapter {
	let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	NpmAdapter::new(config, crate::path::AbsolutePath::new(dir).unwrap(), env)
}

/// Helper to enumerate projects using the adapter with no configured path.
pub(super) fn enumerate(dir: &std::path::Path) -> anyhow::Result<Vec<ProjectInfo>> {
	recording_adapter_default(NpmConfig::default(), dir, 0).enumerate_projects()
}

/// Helper to enumerate projects using the adapter with a configured path.
pub(super) fn enumerate_with_path(
	dir: &std::path::Path,
	path: &str,
) -> anyhow::Result<Vec<ProjectInfo>> {
	recording_adapter_default(NpmConfig::enabled().with_path(path.to_string()), dir, 0)
		.enumerate_projects()
}

pub(super) mod dependency_version;
pub(super) mod enumerate;
pub(super) mod lock_file;
pub(super) mod publish;
pub(super) mod write_version;
