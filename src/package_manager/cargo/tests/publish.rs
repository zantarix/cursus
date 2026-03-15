use super::*;
use std::sync::Arc;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;

fn setup_publish_project(dir: &std::path::Path) -> ProjectInfo {
	write_cargo_toml(
		dir,
		"[package]\nname = \"my-crate\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
	);
	ProjectInfo::for_test("my-crate", AbsolutePath::new(dir.to_path_buf()).unwrap())
}

fn recording_adapter_with_env(dir: &std::path::Path, env: crate::Env) -> CargoAdapter {
	CargoAdapter::new(
		CargoConfig::default(),
		crate::path::AbsolutePath::new(dir).unwrap(),
		env,
	)
}

#[test]
fn publish_success_returns_published() {
	let dir = temp_dir();
	let info = setup_publish_project(dir.path());
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter =
		recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
	let result = adapter.publish(&info).unwrap();
	assert_eq!(result, PublishOutcome::Published);
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "cargo");
	assert_eq!(invocations[0].args[0], "publish");
}

#[test]
fn publish_already_uploaded_returns_already_published() {
	let dir = temp_dir();
	let info = setup_publish_project(dir.path());
	let runner = Arc::new(
		RecordingCommandRunner::new(1)
			.with_stderr(b"error: crate version is already uploaded".to_vec()),
	);
	let adapter =
		recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
	let result = adapter.publish(&info).unwrap();
	assert_eq!(result, PublishOutcome::AlreadyPublished);
}

#[test]
fn publish_already_exists_returns_already_published() {
	let dir = temp_dir();
	let info = setup_publish_project(dir.path());
	let runner = Arc::new(
		RecordingCommandRunner::new(1)
			.with_stderr(b"error: package already exists on crates.io".to_vec()),
	);
	let adapter =
		recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
	let result = adapter.publish(&info).unwrap();
	assert_eq!(result, PublishOutcome::AlreadyPublished);
}

#[test]
fn publish_other_failure_returns_error() {
	let dir = temp_dir();
	let info = setup_publish_project(dir.path());
	let runner = Arc::new(
		RecordingCommandRunner::new(1)
			.with_stderr(b"error: network error connecting to crates.io".to_vec()),
	);
	let adapter =
		recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
	let result = adapter.publish(&info);
	assert!(result.is_err());
	let msg = result.unwrap_err().to_string();
	assert!(
		msg.contains("cargo publish failed"),
		"Expected 'cargo publish failed', got: {msg}"
	);
}

#[test]
fn publish_passes_manifest_path_arg() {
	let dir = temp_dir();
	let info = setup_publish_project(dir.path());
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter =
		recording_adapter_inspectable(CargoConfig::default(), dir.path(), Arc::clone(&runner));
	adapter.publish(&info).unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert!(
		invocations[0].args.contains(&"--manifest-path".to_string()),
		"Should pass --manifest-path, got: {:?}",
		invocations[0].args
	);
}

#[test]
fn publish_without_cargo_token_still_executes_publish() {
	let dir = temp_dir();
	let info = setup_publish_project(dir.path());
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>)
		.with_cargo_registry_token_present(false);
	let adapter = recording_adapter_with_env(dir.path(), env);
	let result = adapter.publish(&info).unwrap();
	assert_eq!(result, PublishOutcome::Published);
	// Command must still be dispatched despite missing token
	assert_eq!(runner.invocations()[0].program, "cargo");
	assert_eq!(runner.invocations()[0].args[0], "publish");
}

#[test]
fn publish_without_cargo_token_emits_warning() {
	crate::test_logging::init_test_logger();
	let dir = temp_dir();
	let info = setup_publish_project(dir.path());
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>)
		.with_cargo_registry_token_present(false);
	let adapter = recording_adapter_with_env(dir.path(), env);
	adapter.publish(&info).unwrap();
	let logs = crate::test_logging::take_logs();
	let warn_msgs: Vec<_> = logs
		.iter()
		.filter(|(lvl, _)| *lvl == log::Level::Warn)
		.collect();
	assert!(
		warn_msgs
			.iter()
			.any(|(_, msg)| msg.contains("CARGO_REGISTRY_TOKEN is not set")),
		"Expected token-absent warning, got: {warn_msgs:?}"
	);
}

#[test]
fn publish_with_cargo_token_no_warning() {
	crate::test_logging::init_test_logger();
	let dir = temp_dir();
	let info = setup_publish_project(dir.path());
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>)
		.with_cargo_registry_token_present(true);
	let adapter = recording_adapter_with_env(dir.path(), env);
	adapter.publish(&info).unwrap();
	let logs = crate::test_logging::take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, msg)| msg.contains("CARGO_REGISTRY_TOKEN")),
		"Should NOT emit token warning when token is present"
	);
}

#[test]
fn publish_with_cargo_token_executes_publish() {
	let dir = temp_dir();
	let info = setup_publish_project(dir.path());
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>)
		.with_cargo_registry_token_present(true);
	let adapter = recording_adapter_with_env(dir.path(), env);
	let result = adapter.publish(&info).unwrap();
	assert_eq!(result, PublishOutcome::Published);
	assert_eq!(runner.invocations()[0].program, "cargo");
}

#[test]
fn registry_name_is_crates_io() {
	let dir = temp_dir();
	let adapter = recording_adapter(CargoConfig::default(), dir.path(), 0);
	assert_eq!(adapter.registry_name(), "crates.io");
}
