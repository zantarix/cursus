use super::*;
use std::sync::Arc;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::filesystem::LocalFilesystem;

fn project_info(dir: &std::path::Path, name: &str, path: &str) -> ProjectInfo {
	ProjectInfo::for_test(name, AbsolutePath::new(dir.join(path)).unwrap())
}

/// Creates a `NpmAdapter` with a fully-configured `Env` for warning tests.
fn recording_adapter_with_env(
	config: NpmConfig,
	dir: &std::path::Path,
	env: crate::Env,
) -> NpmAdapter {
	NpmAdapter::new(config, crate::path::AbsolutePath::new(dir).unwrap(), env)
}

fn project_info_with_provenance(
	dir: &std::path::Path,
	name: &str,
	provenance: Option<bool>,
) -> ProjectInfo {
	let mut info = project_info(dir, name, "");
	info.publishconfig_provenance = provenance;
	info
}

#[tokio::test]
async fn publish_success_returns_published() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
	let info = project_info(dir.path(), "my-app", "");
	assert_eq!(
		adapter.publish(&info).await.unwrap(),
		PublishOutcome::Published
	);
}

#[tokio::test]
async fn publish_epublishconflict_returns_already_published() {
	let dir = temp_dir();
	let runner = Arc::new(
		RecordingCommandRunner::new(1).with_stderr(b"npm error code EPUBLISHCONFLICT".to_vec()),
	);
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
	let info = project_info(dir.path(), "my-app", "");
	assert_eq!(
		adapter.publish(&info).await.unwrap(),
		PublishOutcome::AlreadyPublished
	);
}

#[tokio::test]
async fn publish_cannot_publish_over_returns_already_published() {
	let dir = temp_dir();
	let runner =
		Arc::new(RecordingCommandRunner::new(1).with_stderr(
			b"npm error cannot publish over the previously published version".to_vec(),
		));
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
	let info = project_info(dir.path(), "my-app", "");
	assert_eq!(
		adapter.publish(&info).await.unwrap(),
		PublishOutcome::AlreadyPublished
	);
}

#[tokio::test]
async fn publish_other_failure_returns_error() {
	let dir = temp_dir();
	let runner =
		Arc::new(RecordingCommandRunner::new(1).with_stderr(b"npm error 403 Forbidden".to_vec()));
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), runner);
	let info = project_info(dir.path(), "my-app", "");
	assert!(adapter.publish(&info).await.is_err());
}

#[tokio::test]
async fn publish_scoped_package_uses_restricted_access_by_default() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));
	let info = project_info(dir.path(), "@scope/my-pkg", "");
	adapter.publish(&info).await.unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	let args = &invocations[0].args;
	assert!(
		args.contains(&"--access".to_string()),
		"Expected --access flag"
	);
	assert!(
		args.contains(&"restricted".to_string()),
		"Expected restricted access"
	);
}

#[tokio::test]
async fn publish_scoped_package_respects_custom_access() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter = recording_adapter(
		NpmConfig::enabled().with_access(NpmAccess::Public),
		dir.path(),
		Arc::clone(&runner),
	);
	let info = project_info(dir.path(), "@scope/my-pkg", "");
	adapter.publish(&info).await.unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	let args = &invocations[0].args;
	assert!(args.contains(&"--access".to_string()));
	assert!(args.contains(&"public".to_string()));
}

#[tokio::test]
async fn publish_non_scoped_package_omits_access_flag() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let adapter = recording_adapter(NpmConfig::default(), dir.path(), Arc::clone(&runner));
	let info = project_info(dir.path(), "my-app", "");
	adapter.publish(&info).await.unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	let args = &invocations[0].args;
	assert!(
		!args.contains(&"--access".to_string()),
		"Non-scoped package should not have --access flag"
	);
}

#[tokio::test]
async fn publish_no_auth_still_executes_command() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(false)
	.with_node_auth_token_present(false);
	let adapter = recording_adapter_with_env(NpmConfig::default(), dir.path(), env);
	let info = project_info(dir.path(), "my-app", "");
	let result = adapter.publish(&info).await.unwrap();
	assert_eq!(result, PublishOutcome::Published);
	assert_eq!(runner.invocations()[0].program, "npm");
}

#[tokio::test]
async fn publish_no_auth_emits_no_auth_warning() {
	crate::test_logging::init_test_logger();
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(false)
	.with_node_auth_token_present(false);
	let adapter = recording_adapter_with_env(NpmConfig::default(), dir.path(), env);
	adapter
		.publish(&project_info(dir.path(), "my-app", ""))
		.await
		.unwrap();
	let logs = crate::test_logging::take_logs();
	let warn_msgs: Vec<_> = logs
		.iter()
		.filter(|(lvl, _)| *lvl == log::Level::Warn)
		.collect();
	assert!(
		warn_msgs
			.iter()
			.any(|(_, msg)| msg.contains("no npm authentication detected")),
		"Expected no-auth warning, got: {warn_msgs:?}"
	);
}

#[tokio::test]
async fn publish_oidc_with_node_auth_token_still_executes_command() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(true)
	.with_node_auth_token_present(true);
	let adapter = recording_adapter_with_env(NpmConfig::default(), dir.path(), env);
	let info = project_info(dir.path(), "my-app", "");
	let result = adapter.publish(&info).await.unwrap();
	assert_eq!(result, PublishOutcome::Published);
	assert_eq!(runner.invocations()[0].program, "npm");
}

#[tokio::test]
async fn publish_oidc_with_node_auth_token_emits_token_override_warning() {
	crate::test_logging::init_test_logger();
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(true)
	.with_node_auth_token_present(true);
	let adapter = recording_adapter_with_env(NpmConfig::default(), dir.path(), env);
	adapter
		.publish(&project_info(dir.path(), "my-app", ""))
		.await
		.unwrap();
	let logs = crate::test_logging::take_logs();
	let warn_msgs: Vec<_> = logs
		.iter()
		.filter(|(lvl, _)| *lvl == log::Level::Warn)
		.collect();
	assert!(
		warn_msgs
			.iter()
			.any(|(_, msg)| msg.contains("NODE_AUTH_TOKEN is set in an OIDC-capable CI")),
		"Expected token-override warning, got: {warn_msgs:?}"
	);
}

#[tokio::test]
async fn publish_oidc_without_provenance_still_executes_command() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(true)
	.with_node_auth_token_present(false);
	let adapter = recording_adapter_with_env(
		NpmConfig::enabled().with_access(NpmAccess::Public),
		dir.path(),
		env,
	);
	// publishconfig_provenance is None — provenance warning should fire but publish proceeds
	let info = project_info_with_provenance(dir.path(), "my-app", None);
	let result = adapter.publish(&info).await.unwrap();
	assert_eq!(result, PublishOutcome::Published);
	assert_eq!(runner.invocations()[0].program, "npm");
}

#[tokio::test]
async fn publish_oidc_public_without_provenance_emits_provenance_warning() {
	crate::test_logging::init_test_logger();
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(true)
	.with_node_auth_token_present(false);
	let adapter = recording_adapter_with_env(
		NpmConfig::enabled().with_access(NpmAccess::Public),
		dir.path(),
		env,
	);
	let info = project_info_with_provenance(dir.path(), "my-app", None);
	adapter.publish(&info).await.unwrap();
	let logs = crate::test_logging::take_logs();
	let warn_msgs: Vec<_> = logs
		.iter()
		.filter(|(lvl, _)| *lvl == log::Level::Warn)
		.collect();
	assert!(
		warn_msgs
			.iter()
			.any(|(_, msg)| msg.contains("publishConfig.provenance is not set to true")),
		"Expected provenance warning, got: {warn_msgs:?}"
	);
}

#[tokio::test]
async fn publish_oidc_with_provenance_true_still_executes_command() {
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(true)
	.with_node_auth_token_present(false);
	let adapter = recording_adapter_with_env(
		NpmConfig::enabled().with_access(NpmAccess::Public),
		dir.path(),
		env,
	);
	let info = project_info_with_provenance(dir.path(), "my-app", Some(true));
	let result = adapter.publish(&info).await.unwrap();
	assert_eq!(result, PublishOutcome::Published);
	assert_eq!(runner.invocations()[0].program, "npm");
}

#[tokio::test]
async fn publish_oidc_public_with_provenance_true_no_provenance_warning() {
	crate::test_logging::init_test_logger();
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(true)
	.with_node_auth_token_present(false);
	let adapter = recording_adapter_with_env(
		NpmConfig::enabled().with_access(NpmAccess::Public),
		dir.path(),
		env,
	);
	let info = project_info_with_provenance(dir.path(), "my-app", Some(true));
	adapter.publish(&info).await.unwrap();
	let logs = crate::test_logging::take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, msg)| msg.contains("publishConfig.provenance is not set to true")),
		"Should NOT emit provenance warning when provenance is true"
	);
}

/// Catches the `&&`→`||` mutation at the override-warning guard: when only OIDC
/// is active (no NODE_AUTH_TOKEN), no override warning should fire.
#[tokio::test]
async fn publish_oidc_only_does_not_emit_override_warning() {
	crate::test_logging::init_test_logger();
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(true)
	.with_node_auth_token_present(false);
	let adapter = recording_adapter_with_env(NpmConfig::default(), dir.path(), env);
	adapter
		.publish(&project_info(dir.path(), "my-app", ""))
		.await
		.unwrap();
	let logs = crate::test_logging::take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, msg)| msg.contains("NODE_AUTH_TOKEN is set in an OIDC-capable CI")),
		"Should NOT emit override warning when only OIDC is active: {logs:?}"
	);
}

/// Catches the `&&`→`||` mutation at the no-auth-warning guard: when NODE_AUTH_TOKEN
/// is present (no OIDC), no "no auth" warning should fire.
#[tokio::test]
async fn publish_node_auth_only_does_not_emit_no_auth_warning() {
	crate::test_logging::init_test_logger();
	let dir = temp_dir();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new("/tmp").unwrap(),
		)),
	)
	.with_oidc_environment(false)
	.with_node_auth_token_present(true);
	let adapter = recording_adapter_with_env(NpmConfig::default(), dir.path(), env);
	adapter
		.publish(&project_info(dir.path(), "my-app", ""))
		.await
		.unwrap();
	let logs = crate::test_logging::take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, msg)| msg.contains("no npm authentication detected")),
		"Should NOT emit no-auth warning when NODE_AUTH_TOKEN is present: {logs:?}"
	);
}

#[tokio::test]
async fn registry_name_is_npm() {
	let dir = temp_dir();
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	assert_eq!(adapter.registry_name().await, "npm");
}

#[tokio::test]
async fn manifest_filename_is_package_json() {
	let dir = temp_dir();
	let adapter = recording_adapter_default(NpmConfig::default(), dir.path(), 0);
	assert_eq!(adapter.manifest_filename().await, "package.json");
}
