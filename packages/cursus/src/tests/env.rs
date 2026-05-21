use std::path::Path;
use std::sync::Arc;

use crate::command::test_support::RecordingCommandRunner;
use crate::command::{CommandRunner, shell_program};
use crate::env::Env;
use crate::filesystem::LocalFilesystem;
use crate::forge::CodeForgeClient;
use crate::forge::test_support::RecordingCodeForgeClient;

fn recording_env(exit_code: i32) -> (Arc<RecordingCommandRunner>, Env, tempfile::TempDir) {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let runner = Arc::new(RecordingCommandRunner::new(exit_code));
	let git = Arc::new(crate::git::GitWorkdir::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		crate::path::AbsolutePath::new(dir.path()).unwrap(),
	));
	let env = Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		git,
	);
	(runner, env, dir)
}

#[test]
fn new_has_no_editor_or_code_forge_client() {
	let (_, env, _dir) = recording_env(0);
	assert!(env.editor().is_none());
	assert!(env.code_forge_client().is_err());
}

#[test]
fn new_has_false_auth_flags() {
	let (_, env, _dir) = recording_env(0);
	assert!(!env.oidc_environment());
	assert!(!env.node_auth_token_present());
	assert!(!env.cargo_registry_token_present());
}

#[test]
fn with_oidc_environment_sets_flag() {
	let (_, env, _dir) = recording_env(0);
	let env = env.with_oidc_environment(true);
	assert!(env.oidc_environment());
}

#[test]
fn with_node_auth_token_present_sets_flag() {
	let (_, env, _dir) = recording_env(0);
	let env = env.with_node_auth_token_present(true);
	assert!(env.node_auth_token_present());
}

#[test]
fn with_cargo_registry_token_present_sets_flag() {
	let (_, env, _dir) = recording_env(0);
	let env = env.with_cargo_registry_token_present(true);
	assert!(env.cargo_registry_token_present());
}

#[test]
fn new_has_default_locale() {
	let (_, env, _dir) = recording_env(0);
	assert_eq!(env.locale(), crate::locale::DEFAULT_LOCALE);
}

#[test]
fn with_locale_sets_locale() {
	let (_, env, _dir) = recording_env(0);
	let env = env.with_locale("pt-BR".to_string());
	assert_eq!(env.locale(), "pt-BR");
}

#[test]
fn with_dry_run_runner_preserves_auth_flags() {
	let (_, env, _dir) = recording_env(0);
	let env = env
		.with_oidc_environment(true)
		.with_node_auth_token_present(true)
		.with_cargo_registry_token_present(true);
	let dry_env = env.with_dry_run_runner();
	assert!(dry_env.oidc_environment());
	assert!(dry_env.node_auth_token_present());
	assert!(dry_env.cargo_registry_token_present());
}

#[test]
fn with_dry_run_runner_preserves_locale() {
	let (_, env, _dir) = recording_env(0);
	let env = env.with_locale("fr".to_string());
	let dry_env = env.with_dry_run_runner();
	assert_eq!(dry_env.locale(), "fr");
}

#[test]
fn with_dry_run_runner_preserves_git() {
	let (_, env, _dir) = recording_env(0);
	let path = env.git().path().clone();
	let dry_env = env.with_dry_run_runner();
	assert_eq!(dry_env.git().path(), &path);
}

#[test]
fn with_editor_sets_editor() {
	let (_, env, _dir) = recording_env(0);
	let env = env.with_editor("vim".to_string());
	assert_eq!(env.editor(), Some("vim"));
}

#[test]
fn with_code_forge_client_sets_client() {
	let (_, env, _dir) = recording_env(0);
	let client = Arc::new(RecordingCodeForgeClient::new()) as Arc<dyn CodeForgeClient>;
	let env = env.with_code_forge_client(Arc::clone(&client));
	assert!(env.code_forge_client().is_ok());
}

#[test]
fn with_editor_opt_some_sets_editor() {
	let (_, env, _dir) = recording_env(0);
	let env = env.with_editor_opt(Some("nano".to_string()));
	assert_eq!(env.editor(), Some("nano"));
}

#[test]
fn with_editor_opt_none_clears_editor() {
	let (_, env, _dir) = recording_env(0);
	let env = env.with_editor("vim".to_string()).with_editor_opt(None);
	assert!(env.editor().is_none());
}

#[test]
fn with_code_forge_client_result_ok_sets_client() {
	let (_, env, _dir) = recording_env(0);
	let client = Arc::new(RecordingCodeForgeClient::new()) as Arc<dyn CodeForgeClient>;
	let env = env.with_code_forge_client_result(Ok(client));
	assert!(env.code_forge_client().is_ok());
}

#[test]
fn with_code_forge_client_result_err_clears_client() {
	let (_, env, _dir) = recording_env(0);
	let client = Arc::new(RecordingCodeForgeClient::new()) as Arc<dyn CodeForgeClient>;
	let env = env
		.with_code_forge_client(client)
		.with_code_forge_client_result(Err("no token".into()));
	assert!(env.code_forge_client().is_err());
}

#[test]
fn code_forge_name_defaults_to_configured_forge() {
	let (_, env, _dir) = recording_env(0);
	assert_eq!(env.code_forge_name(), "the configured forge");
}

#[test]
fn with_code_forge_client_captures_forge_name() {
	let (_, env, _dir) = recording_env(0);
	let client = Arc::new(RecordingCodeForgeClient::new().with_forge_name("GitLab"))
		as Arc<dyn CodeForgeClient>;
	let env = env.with_code_forge_client(client);
	assert_eq!(env.code_forge_name(), "GitLab");
}

#[test]
fn with_code_forge_client_result_ok_captures_forge_name() {
	let (_, env, _dir) = recording_env(0);
	let client = Arc::new(RecordingCodeForgeClient::new().with_forge_name("GitLab"))
		as Arc<dyn CodeForgeClient>;
	let env = env.with_code_forge_client_result(Ok(client));
	assert_eq!(env.code_forge_name(), "GitLab");
}

#[test]
fn with_code_forge_client_result_err_leaves_default_name() {
	// Initial Err sets no client and keeps the default name; the contract
	// the orchestration layer relies on is that `code_forge_name` is the
	// label of the *active* client when one is configured.
	let (_, env, _dir) = recording_env(0);
	let env = env.with_code_forge_client_result(Err("no token".into()));
	assert_eq!(env.code_forge_name(), "the configured forge");
}

#[tokio::test]
async fn run_delegates_to_runner() {
	let (runner, env, _dir) = recording_env(0);
	env.run("echo", &["hello"], Path::new(".")).await.unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations[0].program, "echo");
	assert_eq!(invocations[0].args, ["hello"]);
}

#[tokio::test]
async fn run_mut_delegates_to_runner() {
	let (runner, env, _dir) = recording_env(0);
	env.run_mut("git", &["commit", "-m", "msg"], Path::new("."))
		.await
		.unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["commit", "-m", "msg"]);
}

#[tokio::test]
async fn run_streaming_delegates_to_runner() {
	let (runner, env, _dir) = recording_env(0);
	env.run_streaming("npm install", Path::new("."))
		.await
		.unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations[0].program, shell_program());
	assert!(invocations[0].is_shell);
	assert!(invocations[0].is_streaming);
}

#[tokio::test]
async fn run_interactive_delegates_to_runner() {
	let (runner, env, _dir) = recording_env(0);
	env.run_interactive("vim", &[], Path::new("."))
		.await
		.unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations[0].program, "vim");
	assert!(invocations[0].is_interactive);
}

#[tokio::test]
async fn with_dry_run_runner_suppresses_run_mut() {
	let (runner, env, _dir) = recording_env(0);
	let dry_env = env.with_dry_run_runner();
	dry_env
		.run_mut("git", &["push", "origin", "HEAD"], Path::new("."))
		.await
		.unwrap();
	// The inner recording runner must NOT have been called (DryRunCommandRunner intercepts)
	assert!(runner.invocations().is_empty());
}

#[tokio::test]
async fn with_dry_run_runner_still_forwards_run() {
	let (runner, env, _dir) = recording_env(0);
	let dry_env = env.with_dry_run_runner();
	dry_env
		.run("git", &["status"], Path::new("."))
		.await
		.unwrap();
	// Read-only run is forwarded to the inner runner
	assert_eq!(runner.invocations().len(), 1);
	assert_eq!(runner.invocations()[0].program, "git");
}

// run_editor_on tests

#[tokio::test]
async fn run_editor_on_uses_editor_when_set() {
	let workdir = tempfile::tempdir().unwrap();
	let path = workdir.path().join("config.toml");
	std::fs::write(&path, "").unwrap();

	let (runner, env, _dir) = recording_env(0);
	let env = env.with_editor("vim".to_string());
	env.run_editor_on(&path, workdir.path()).await.unwrap();

	let invocations = runner.invocations();
	let editor_call = invocations
		.iter()
		.find(|i| i.is_interactive && i.is_shell)
		.expect("Expected a shell interactive invocation");
	let expected = format!("vim {}", crate::shell::shell_quote(&path.to_string_lossy()));
	assert_eq!(editor_call.args[1], expected);
}

#[tokio::test]
async fn run_editor_on_ignores_empty_editor_string() {
	let workdir = tempfile::tempdir().unwrap();
	let path = workdir.path().join("config.toml");
	std::fs::write(&path, "").unwrap();

	// Empty editor → falls back to find_default_editor → runner returns 0 → "nano"
	let (runner, env, _dir) = recording_env(0);
	let env = env.with_editor(String::new());
	env.run_editor_on(&path, workdir.path()).await.unwrap();

	let invocations = runner.invocations();
	let editor_call = invocations
		.iter()
		.find(|i| i.is_interactive && i.is_shell)
		.expect("Expected a shell interactive invocation");
	let expected = format!(
		"nano {}",
		crate::shell::shell_quote(&path.to_string_lossy())
	);
	assert_eq!(editor_call.args[1], expected, "Should fall back to nano");
}

#[tokio::test]
async fn run_editor_on_nonzero_exit_returns_error() {
	let workdir = tempfile::tempdir().unwrap();
	let path = workdir.path().join("config.toml");
	std::fs::write(&path, "").unwrap();

	let (_, env, _dir) = recording_env(1);
	let env = env.with_editor("vim".to_string());
	let result = env.run_editor_on(&path, workdir.path()).await;

	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("Editor exited with status")
	);
}

#[tokio::test]
async fn run_editor_on_falls_back_to_default_editor() {
	let workdir = tempfile::tempdir().unwrap();
	let path = workdir.path().join("config.toml");
	std::fs::write(&path, "").unwrap();

	// No editor set, runner exit_code=0 → which nano succeeds → "nano"
	let (runner, env, _dir) = recording_env(0);
	env.run_editor_on(&path, workdir.path()).await.unwrap();

	let invocations = runner.invocations();
	let editor_call = invocations
		.iter()
		.find(|i| i.is_interactive && i.is_shell)
		.expect("Expected a shell interactive invocation");
	let expected = format!(
		"nano {}",
		crate::shell::shell_quote(&path.to_string_lossy())
	);
	assert_eq!(editor_call.args[1], expected);
}

#[tokio::test]
async fn run_editor_on_no_editor_found_returns_error() {
	let workdir = tempfile::tempdir().unwrap();
	let path = workdir.path().join("config.toml");
	std::fs::write(&path, "").unwrap();

	// Runner exit_code=1 → all which calls fail → no default found
	let (_, env, _dir) = recording_env(1);
	let result = env.run_editor_on(&path, workdir.path()).await;

	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("No editor found"));
}

#[tokio::test]
async fn run_editor_on_uses_provided_cwd() {
	let workdir = tempfile::tempdir().unwrap();
	let cursus_dir = workdir.path().join(".cursus");
	std::fs::create_dir_all(&cursus_dir).unwrap();
	let path = cursus_dir.join("config.toml");
	std::fs::write(&path, "").unwrap();

	let (runner, env, _dir) = recording_env(0);
	let env = env.with_editor("vim".to_string());
	env.run_editor_on(&path, workdir.path()).await.unwrap();

	let invocations = runner.invocations();
	let editor_call = invocations
		.iter()
		.find(|i| i.is_interactive && i.is_shell)
		.expect("Expected a shell interactive editor invocation");
	assert_eq!(
		editor_call.cwd,
		workdir.path(),
		"Editor should be invoked with the provided cwd, not the file's parent"
	);
}

#[tokio::test]
async fn run_editor_on_handles_multi_word_editor() {
	let workdir = tempfile::tempdir().unwrap();
	let path = workdir.path().join("config.toml");
	std::fs::write(&path, "").unwrap();

	let (runner, env, _dir) = recording_env(0);
	let env = env.with_editor("code --wait".to_string());
	env.run_editor_on(&path, workdir.path()).await.unwrap();

	let invocations = runner.invocations();
	let editor_call = invocations
		.iter()
		.find(|i| i.is_interactive && i.is_shell)
		.expect("Expected a shell interactive invocation");
	let expected = format!(
		"code --wait {}",
		crate::shell::shell_quote(&path.to_string_lossy())
	);
	assert_eq!(editor_call.args[1], expected);
}

#[tokio::test]
async fn run_editor_on_handles_path_with_single_quote() {
	let workdir = tempfile::tempdir().unwrap();
	// Path whose name contains a single quote — tests the '\\'' escaping logic.
	let path = workdir.path().join("it's a file.toml");
	std::fs::write(&path, "").unwrap();

	let (runner, env, _dir) = recording_env(0);
	let env = env.with_editor("vim".to_string());
	env.run_editor_on(&path, workdir.path()).await.unwrap();

	let invocations = runner.invocations();
	let editor_call = invocations
		.iter()
		.find(|i| i.is_interactive && i.is_shell)
		.expect("Expected a shell interactive invocation");
	let expected = format!("vim {}", crate::shell::shell_quote(&path.to_string_lossy()));
	assert_eq!(editor_call.args[1], expected);
}

#[tokio::test]
async fn run_shell_interactive_delegates_to_runner() {
	let (runner, env, _dir) = recording_env(0);
	let cwd = tempfile::tempdir().unwrap();
	env.run_shell_interactive("echo hello", cwd.path())
		.await
		.unwrap();
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert!(invocations[0].is_shell);
	assert!(invocations[0].is_interactive);
}
