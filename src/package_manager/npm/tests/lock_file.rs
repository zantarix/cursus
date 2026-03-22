use super::*;
use std::sync::Arc;

use crate::command::test_support::{DispatchingCommandRunner, RecordingCommandRunner};
use crate::filesystem::LocalFilesystem;

#[test]
fn update_lock_file_no_op_when_no_lock_file() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);

	// Should succeed and return None when there is no lock file
	assert_eq!(adapter.update_lock_file().unwrap(), None);
}

#[test]
fn update_lock_file_custom_command_empty_fails() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let adapter = recording_adapter_default(
		NpmConfig::enabled().with_lock_command("".to_string()),
		dir.path(),
		0,
	);

	let result = adapter.update_lock_file();
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("lock_command is empty")
	);
}

#[test]
fn update_lock_file_custom_command_nonexistent_fails() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let runner =
		Arc::new(RecordingCommandRunner::new(1).with_stderr(b"command not found".to_vec()));
	let adapter = recording_adapter(
		NpmConfig::enabled().with_lock_command("nonexistent-command-12345".to_string()),
		dir.path(),
		runner,
	);

	let result = adapter.update_lock_file();
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("Lock command"));
}

#[test]
fn update_lock_file_custom_command_with_exit_code_fails() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let runner = Arc::new(RecordingCommandRunner::new(1).with_stderr(b"exit status 1".to_vec()));
	let adapter = recording_adapter(
		NpmConfig::enabled().with_lock_command("false".to_string()),
		dir.path(),
		runner,
	);

	let result = adapter.update_lock_file();
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("Lock command"));
}

#[test]
fn update_lock_file_custom_command_succeeds() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let adapter = recording_adapter_default(
		NpmConfig::enabled().with_lock_command("true".to_string()),
		dir.path(),
		0,
	);

	// Custom command succeeds but returns None (we don't know which file it wrote)
	assert_eq!(adapter.update_lock_file().unwrap(), None);
}

#[test]
fn update_lock_file_no_lock_file_returns_none() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	// No lock file present — update_lock_file should return Ok(None)
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	assert_eq!(adapter.update_lock_file().unwrap(), None);
}

#[test]
fn update_lock_file_npm_passes_correct_args() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
	std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));

	let result = adapter.update_lock_file();
	assert_eq!(result.unwrap(), Some(dir.path().join("package-lock.json")));

	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "npm");
	assert_eq!(
		invocations[0].args,
		["install", "--package-lock-only", "--ignore-scripts"]
	);
}

#[test]
fn update_lock_file_npm_failure_propagates() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
	std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(1).with_stderr(b"npm error".to_vec()));
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
	assert!(adapter.update_lock_file().is_err());
}

#[test]
fn update_lock_file_pnpm_passes_correct_args() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
	std::fs::write(
		dir.path().join("pnpm-lock.yaml"),
		"lockfileVersion: '6.0'\n",
	)
	.unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));

	let result = adapter.update_lock_file();
	assert_eq!(result.unwrap(), Some(dir.path().join("pnpm-lock.yaml")));

	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "pnpm");
	assert_eq!(
		invocations[0].args,
		["install", "--lockfile-only", "--ignore-scripts"]
	);
}

#[test]
fn update_lock_file_pnpm_failure_propagates() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
	std::fs::write(
		dir.path().join("pnpm-lock.yaml"),
		"lockfileVersion: '6.0'\n",
	)
	.unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(1).with_stderr(b"pnpm error".to_vec()));
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
	assert!(adapter.update_lock_file().is_err());
}

#[test]
fn update_lock_file_yarn_classic_passes_correct_args() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
	std::fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();

	// Yarn Classic (1.x) silently ignores --mode, so we detect the version and
	// use --ignore-scripts instead to suppress lifecycle scripts.
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"yarn",
		vec!["--version".into()],
		0,
		b"1.22.22\n".to_vec(),
	));
	let adapter = dispatching_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));

	let result = adapter.update_lock_file();
	assert_eq!(result.unwrap(), Some(dir.path().join("yarn.lock")));

	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 2);
	assert_eq!(invocations[0].args, ["--version"]);
	assert_eq!(invocations[1].args, ["install", "--ignore-scripts"]);
}

#[test]
fn update_lock_file_yarn_berry_passes_correct_args() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
	std::fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();

	// Yarn Berry (v2+) supports --mode update-lockfile which skips scripts automatically.
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"yarn",
		vec!["--version".into()],
		0,
		b"4.13.0\n".to_vec(),
	));
	let adapter = dispatching_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));

	let result = adapter.update_lock_file();
	assert_eq!(result.unwrap(), Some(dir.path().join("yarn.lock")));

	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 2);
	assert_eq!(invocations[0].args, ["--version"]);
	assert_eq!(
		invocations[1].args,
		["install", "--mode", "update-lockfile"]
	);
}

#[test]
fn update_lock_file_yarn_version_detection_failure_propagates() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
	std::fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();

	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stderr(
		"yarn",
		vec!["--version".into()],
		1,
		b"command not found".to_vec(),
	));
	let adapter = dispatching_adapter(NpmConfig::default(), dir.path(), runner);
	assert!(adapter.update_lock_file().is_err());
}

#[test]
fn update_lock_file_yarn_failure_propagates() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
	std::fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();

	let runner = Arc::new(
		DispatchingCommandRunner::new(0)
			.on_with_args_stdout("yarn", vec!["--version".into()], 0, b"1.22.22\n".to_vec())
			.on_with_args_stderr("yarn", vec!["install".into()], 1, b"yarn error".to_vec()),
	);
	let adapter = dispatching_adapter(NpmConfig::default(), dir.path(), runner);
	assert!(adapter.update_lock_file().is_err());
}

#[test]
fn update_lock_file_custom_command_uses_shell_execution() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter = recording_adapter(
		NpmConfig::enabled().with_lock_command("custom-lock-cmd --flag".to_string()),
		dir.path(),
		Arc::clone(&runner),
	);
	adapter.update_lock_file().unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert!(
		invocations[0].is_shell,
		"Custom lock_command should use shell execution"
	);
	assert_eq!(invocations[0].args[1], "custom-lock-cmd --flag");
}

#[test]
fn update_lock_file_custom_command_failure_propagates() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "my-app", "version": "1.0.0"}"#);
	let runner =
		Arc::new(RecordingCommandRunner::new(1).with_stderr(b"command not found".to_vec()));
	let adapter = recording_adapter(
		NpmConfig::enabled().with_lock_command("bad-cmd".to_string()),
		dir.path(),
		runner,
	);
	let result = adapter.update_lock_file();
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("Lock command"));
}

fn dry_run_adapter(config: NpmConfig, dir: &std::path::Path) -> NpmAdapter {
	use crate::command::CommandRunner;
	use crate::command::DryRunCommandRunner;
	let inner: Arc<dyn CommandRunner> =
		Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>;
	let dry_runner: Arc<dyn CommandRunner> = Arc::new(DryRunCommandRunner::new(Arc::clone(&inner)));
	let env = crate::Env::new(dry_runner, Arc::new(LocalFilesystem));
	NpmAdapter::new(config, crate::path::AbsolutePath::new(dir).unwrap(), env)
}

#[test]
fn update_lock_file_dry_run_custom_command_returns_none() {
	let dir = temp_dir();
	let adapter = dry_run_adapter(
		NpmConfig::enabled().with_lock_command("my-lock-cmd".to_string()),
		dir.path(),
	);
	assert_eq!(adapter.update_lock_file().unwrap(), None);
}

#[test]
fn update_lock_file_dry_run_no_lock_file_returns_none() {
	let dir = temp_dir();
	let adapter = dry_run_adapter(NpmConfig::default(), dir.path());
	assert_eq!(adapter.update_lock_file().unwrap(), None);
}

#[test]
fn update_lock_file_dry_run_package_lock_json_returns_path() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();
	let adapter = dry_run_adapter(NpmConfig::default(), dir.path());
	assert_eq!(
		adapter.update_lock_file().unwrap(),
		Some(dir.path().join("package-lock.json"))
	);
}

#[test]
fn update_lock_file_dry_run_pnpm_lock_yaml_returns_path() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
	let adapter = dry_run_adapter(NpmConfig::default(), dir.path());
	assert_eq!(
		adapter.update_lock_file().unwrap(),
		Some(dir.path().join("pnpm-lock.yaml"))
	);
}

#[test]
fn update_lock_file_dry_run_yarn_lock_returns_path() {
	use crate::command::DryRunCommandRunner;
	let dir = temp_dir();
	std::fs::write(dir.path().join("yarn.lock"), "").unwrap();
	// yarn --version is a read-only call that passes through DryRunCommandRunner,
	// so the inner runner must return a parseable version string.
	let inner: Arc<dyn crate::command::CommandRunner> =
		Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
			"yarn",
			vec!["--version".into()],
			0,
			b"1.22.22\n".to_vec(),
		));
	let dry_runner: Arc<dyn crate::command::CommandRunner> =
		Arc::new(DryRunCommandRunner::new(Arc::clone(&inner)));
	let env = crate::Env::new(dry_runner, Arc::new(LocalFilesystem));
	let adapter = NpmAdapter::new(
		NpmConfig::default(),
		crate::path::AbsolutePath::new(dir.path()).unwrap(),
		env,
	);
	assert_eq!(
		adapter.update_lock_file().unwrap(),
		Some(dir.path().join("yarn.lock"))
	);
}

#[test]
fn update_lock_file_yarn_version_failure_includes_stderr_in_error() {
	let dir = temp_dir();
	write_package_json(dir.path(), r#"{"name": "test-app", "version": "1.0.0"}"#);
	std::fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();

	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stderr(
		"yarn",
		vec!["--version".into()],
		1,
		b"yarn: command not found".to_vec(),
	));
	let adapter = dispatching_adapter(NpmConfig::default(), dir.path(), runner);

	let err = adapter.update_lock_file().unwrap_err();
	assert!(
		err.to_string().contains("yarn: command not found"),
		"error should include stderr from failed yarn --version: {err}"
	);
}
