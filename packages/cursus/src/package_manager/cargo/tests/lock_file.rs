use super::*;
use std::sync::Arc;

#[tokio::test]
async fn update_lock_file_command_failure_propagates_error() {
	let dir = temp_dir();
	use crate::command::test_support::RecordingCommandRunner;
	let runner =
		Arc::new(RecordingCommandRunner::new(1).with_stderr(b"error: invalid manifest".to_vec()));
	let adapter = recording_adapter_inspectable(CargoConfig::default(), dir.path(), runner);

	let result = adapter.update_lock_file().await;
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("cargo update --workspace failed"),
		"Expected 'cargo update --workspace failed', got: {msg}"
	);
}

#[tokio::test]
async fn update_lock_file_passes_correct_args() {
	let dir = temp_dir();
	use crate::command::test_support::RecordingCommandRunner;
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter =
		recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));

	let result = adapter.update_lock_file().await;
	assert_eq!(result.unwrap(), Some(dir.path().join("Cargo.lock")));

	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "cargo");
	assert_eq!(invocations[0].args, ["update", "--workspace"]);
	assert_eq!(invocations[0].cwd, dir.path());
}

#[tokio::test]
async fn update_lock_file_dry_run_skips_command_but_returns_path() {
	use crate::command::CommandRunner;
	use crate::command::DryRunCommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	let dir = temp_dir();
	let inner: Arc<dyn CommandRunner> =
		Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>;
	let dry_runner: Arc<dyn CommandRunner> = Arc::new(DryRunCommandRunner::new(Arc::clone(&inner)));
	let env = crate::Env::new(
		Arc::clone(&dry_runner),
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			dry_runner,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	);
	let adapter = CargoAdapter::new(
		CargoConfig::default(),
		crate::path::AbsolutePath::new(dir.path()).unwrap(),
		env,
	);
	let result = adapter.update_lock_file().await.unwrap();
	assert_eq!(result, Some(dir.path().join("Cargo.lock")));
}
