use std::sync::Arc;

use crate::cli::prepare::ReleaseInfo;
use crate::cli::prepare::git_lifecycle::*;
use crate::command::CommandRunner;
use crate::command::test_support::{DispatchingCommandRunner, RecordingCommandRunner};
use crate::filesystem::LocalFilesystem;
use crate::model::config;

// ── stage_and_commit ──────────────────────────────────────────────────────

#[tokio::test]
async fn stage_and_commit_empty_releases_is_noop() {
	let dir = tempfile::tempdir().unwrap();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let git = crate::git::GitWorkdir::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		dir_abs.clone(),
	);
	let result = stage_and_commit(
		&git,
		&[],
		&[],
		&[],
		"ci(release): version packages",
		&crate::filesystem::LocalFilesystem,
	)
	.await;
	assert!(result.is_ok());
}

#[tokio::test]
async fn stage_and_commit_dry_run_suppresses_git_commands() {
	let dir = tempfile::tempdir().unwrap();
	let release_infos = vec![ReleaseInfo {
		package_name: "my-pkg".to_string(),
		new_version: "1.0.0".parse().unwrap(),
		changelog_entry: String::new(),
	}];
	let inner = Arc::new(RecordingCommandRunner::new(0));
	let dry_run_runner =
		crate::command::DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let git = crate::git::GitWorkdir::new(
		Arc::new(dry_run_runner) as Arc<dyn CommandRunner>,
		dir_abs.clone(),
	);
	let result = stage_and_commit(
		&git,
		&[],
		&release_infos,
		&[],
		"ci(release): version packages",
		&crate::filesystem::LocalFilesystem,
	)
	.await;
	assert!(result.is_ok());
	// DryRunCommandRunner suppresses run_mut calls — the inner recorder receives nothing.
	assert!(inner.invocations().is_empty());
}

#[tokio::test]
async fn extra_files_outside_repo_is_rejected() {
	// Create an outer temp dir with a repo subdir and a secret file alongside it.
	let outer = tempfile::tempdir().unwrap();
	let repo_dir = outer.path().join("repo");
	std::fs::create_dir(&repo_dir).unwrap();
	let secret = outer.path().join("secret.txt");
	std::fs::write(&secret, "secret").unwrap();
	// "../secret.txt" from repo_dir resolves to outer/secret.txt (outside the repo).
	let extra_files = vec!["../secret.txt".to_string()];
	let release_infos = vec![ReleaseInfo {
		package_name: "my-pkg".to_string(),
		new_version: "1.0.0".parse().unwrap(),
		changelog_entry: String::new(),
	}];
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let dir_abs = crate::path::AbsolutePath::new(&repo_dir).unwrap();
	let git = crate::git::GitWorkdir::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		dir_abs.clone(),
	);
	let result = stage_and_commit(
		&git,
		&extra_files,
		&release_infos,
		&[],
		"ci(release): version packages",
		&crate::filesystem::LocalFilesystem,
	)
	.await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("resolves outside the repository root")
	);
}

#[cfg(unix)]
#[tokio::test]
async fn extra_files_symlink_outside_repo_is_rejected() {
	let dir = tempfile::tempdir().unwrap();
	// Create a symlink inside the tempdir pointing to /tmp (outside the repo).
	let symlink_path = dir.path().join("escape");
	std::os::unix::fs::symlink("/tmp", &symlink_path).unwrap();
	let extra_files = vec!["escape".to_string()];
	let release_infos = vec![ReleaseInfo {
		package_name: "my-pkg".to_string(),
		new_version: "1.0.0".parse().unwrap(),
		changelog_entry: String::new(),
	}];
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let git = crate::git::GitWorkdir::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		dir_abs.clone(),
	);
	let result = stage_and_commit(
		&git,
		&extra_files,
		&release_infos,
		&[],
		"ci(release): version packages",
		&crate::filesystem::LocalFilesystem,
	)
	.await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("resolves outside the repository root")
	);
}

#[tokio::test]
async fn extra_files_nonexistent_is_skipped() {
	let dir = tempfile::tempdir().unwrap();
	let extra_files = vec!["does-not-exist.txt".to_string()];
	let release_infos = vec![ReleaseInfo {
		package_name: "my-pkg".to_string(),
		new_version: "1.0.0".parse().unwrap(),
		changelog_entry: String::new(),
	}];
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let git = crate::git::GitWorkdir::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		dir_abs.clone(),
	);
	// Should succeed (non-existent file is a warning, not an error).
	let result = stage_and_commit(
		&git,
		&extra_files,
		&release_infos,
		&[],
		"ci(release): version packages",
		&crate::filesystem::LocalFilesystem,
	)
	.await;
	assert!(result.is_ok());
}

#[tokio::test]
async fn extra_files_absolute_path_is_rejected() {
	let dir = tempfile::tempdir().unwrap();
	let extra_files = vec!["/etc/passwd".to_string()];
	let release_infos = vec![ReleaseInfo {
		package_name: "my-pkg".to_string(),
		new_version: "1.0.0".parse().unwrap(),
		changelog_entry: String::new(),
	}];
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let git = crate::git::GitWorkdir::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		dir_abs.clone(),
	);
	let result = stage_and_commit(
		&git,
		&extra_files,
		&release_infos,
		&[],
		"ci(release): version packages",
		&crate::filesystem::LocalFilesystem,
	)
	.await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("resolves outside the repository root")
	);
}

#[tokio::test]
async fn compute_release_branch_uses_flag_over_all() {
	assert_eq!(
		compute_release_branch(Some("my-branch"), "release/", Some("main")).unwrap(),
		"my-branch"
	);
}

#[tokio::test]
async fn compute_release_branch_uses_config_prefix() {
	assert_eq!(
		compute_release_branch(None, "release/", Some("main")).unwrap(),
		"release/main"
	);
}

#[tokio::test]
async fn compute_release_branch_uses_default_prefix() {
	assert_eq!(
		compute_release_branch(None, "cursus-release/", Some("main")).unwrap(),
		"cursus-release/main"
	);
}

#[tokio::test]
async fn compute_release_branch_detached_fallback() {
	assert_eq!(
		compute_release_branch(None, "cursus-release/", None).unwrap(),
		"cursus-release/detached"
	);
}

#[tokio::test]
async fn compute_release_branch_rejects_dash_prefix() {
	let result = compute_release_branch(Some("--detach"), "release/", Some("main"));
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("must not start with '-'")
	);
}

#[tokio::test]
async fn compute_release_branch_rejects_single_dash() {
	let result = compute_release_branch(Some("-"), "release/", None);
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("must not start with '-'")
	);
}

#[tokio::test]
async fn compute_release_branch_rejects_empty() {
	let result = compute_release_branch(Some(""), "release/", Some("main"));
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("must not be empty")
	);
}

#[tokio::test]
async fn compute_release_branch_rejects_composed_leading_dash() {
	// config_prefix itself is attacker-controlled via .cursus/config.toml;
	// the composed result must still be rejected when it starts with '-'.
	let result = compute_release_branch(None, "--", Some("main"));
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("must not start with '-'")
	);
}

#[tokio::test]
async fn compute_release_branch_rejects_composed_control_char() {
	// A current_branch containing a control character (e.g. BEL) must be
	// caught before the composed name reaches git.
	let result = compute_release_branch(None, "cursus-release/", Some("feat\x07ure"));
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("control character")
	);
}

#[tokio::test]
async fn check_dirty_tree_succeeds_when_clean() {
	let dir = tempfile::tempdir().unwrap();
	let runner = Arc::new(RecordingCommandRunner::new(0)); // empty stdout → clean
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let git = crate::git::GitWorkdir::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		dir_abs.clone(),
	);
	let result = check_dirty_tree(&git).await;
	assert!(result.is_ok());
}

#[tokio::test]
async fn check_dirty_tree_fails_when_dirty() {
	let dir = tempfile::tempdir().unwrap();
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(b" M src/main.rs\n".to_vec()));
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let git = crate::git::GitWorkdir::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		dir_abs.clone(),
	);
	let result = check_dirty_tree(&git).await;
	assert!(result.is_err());
	assert!(
		result.unwrap_err().to_string().contains("dirty"),
		"Expected 'dirty' in error message"
	);
}

fn make_test_env(dir: &std::path::Path) -> crate::Env {
	let r = Arc::new(crate::command::test_support::RecordingCommandRunner::new(0))
		as Arc<dyn CommandRunner>;
	crate::Env::new(
		Arc::clone(&r),
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			r,
			crate::path::AbsolutePath::new(dir).unwrap(),
		)),
	)
}

/// Sets up a temp dir with a Cargo project, branch strategy git config, and GitHub config.
async fn setup_branch_strategy_with_github() -> tempfile::TempDir {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let setup_env = make_test_env(dir.path());
	crate::model::config::Config::new()
		.with_cargo(crate::model::config::CargoConfig::enabled())
		.with_git(
			crate::model::config::GitConfig::enabled_config()
				.with_strategy(crate::model::config::Strategy::Branch),
		)
		.with_github(
			crate::model::config::GitHubConfig::enabled_config()
				.with_owner("acme".to_string())
				.with_repo("app".to_string())
				.with_pull_request_title("My Release PR".to_string()),
		)
		.save(setup_env.fs(), setup_env.git().path())
		.await
		.unwrap();
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"test-pkg\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
	)
	.unwrap();
	std::fs::create_dir_all(dir.path().join("src")).unwrap();
	std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
	let cursus_dir = dir.path().join(".cursus");
	std::fs::write(
		cursus_dir.join("change.md"),
		"+++\ntest-pkg = \"patch\"\n+++\n\nFix\n",
	)
	.unwrap();
	dir
}

#[tokio::test]
async fn cmd_prepare_branch_strategy_with_github_creates_pr() {
	use crate::forge::CodeForgeClient;
	use crate::forge::test_support::{CodeForgeInvocation, RecordingCodeForgeClient};
	let dir = setup_branch_strategy_with_github().await;
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec![
			"rev-parse".to_string(),
			"--abbrev-ref".to_string(),
			"HEAD".to_string(),
		],
		0,
		b"main\n".to_vec(),
	));
	let client = Arc::new(RecordingCodeForgeClient::new());
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		)),
	)
	.with_code_forge_client(Arc::clone(&client) as Arc<dyn CodeForgeClient>);
	let config = config::load(env.fs(), env.git().path())
		.await
		.unwrap()
		.unwrap();
	let args = crate::cli::prepare::PrepareArgs::default();

	let result = crate::cli::prepare::cmd_prepare(&args, false, &env, config).await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");

	let invocations = client.invocations();
	let pr = invocations
		.iter()
		.find(|i| matches!(i, CodeForgeInvocation::CreatePullRequest { .. }));
	assert!(pr.is_some(), "Expected PR creation, got: {invocations:?}");
	if let Some(CodeForgeInvocation::CreatePullRequest { title, .. }) = pr {
		assert_eq!(title, "My Release PR");
	}
}

#[tokio::test]
async fn cmd_prepare_branch_strategy_pr_failure_is_fatal() {
	use crate::forge::CodeForgeClient;
	use crate::forge::test_support::RecordingCodeForgeClient;
	let dir = setup_branch_strategy_with_github().await;
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec![
			"rev-parse".to_string(),
			"--abbrev-ref".to_string(),
			"HEAD".to_string(),
		],
		0,
		b"main\n".to_vec(),
	));
	let client = Arc::new(RecordingCodeForgeClient::new().with_create_pr_failure());
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		)),
	)
	.with_code_forge_client(Arc::clone(&client) as Arc<dyn CodeForgeClient>);
	let config = config::load(env.fs(), env.git().path())
		.await
		.unwrap()
		.unwrap();
	let args = crate::cli::prepare::PrepareArgs::default();

	let result = crate::cli::prepare::cmd_prepare(&args, false, &env, config).await;
	assert!(
		result.is_err(),
		"PR failure should be fatal, got: {result:?}"
	);
}

#[tokio::test]
async fn cmd_prepare_no_code_forge_client_errors() {
	let dir = setup_branch_strategy_with_github().await;
	let runner = Arc::new(RecordingCommandRunner::new(0));
	// No github client — pre-flight check should error
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		)),
	);
	let config = config::load(env.fs(), env.git().path())
		.await
		.unwrap()
		.unwrap();
	let args = crate::cli::prepare::PrepareArgs::default();

	let result = crate::cli::prepare::cmd_prepare(&args, false, &env, config).await;
	assert!(result.is_err(), "Expected Err without github client");
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("code forge client is unavailable"),
		"Expected 'code forge client is unavailable' error, got: {msg}"
	);
}

// ── check_forge_preconditions ─────────────────────────────────────────────

fn minimal_env_with(
	client: Arc<dyn crate::forge::CodeForgeClient>,
	gitlab_uses_job_token_only: bool,
) -> crate::Env {
	let dir = tempfile::tempdir().unwrap();
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	std::mem::forget(dir);
	let runner: Arc<dyn CommandRunner> = Arc::new(RecordingCommandRunner::new(0));
	let git = Arc::new(crate::git::GitWorkdir::new(Arc::clone(&runner), dir_abs));
	crate::Env::new(runner, Arc::new(LocalFilesystem), git)
		.with_code_forge_client(client)
		.with_gitlab_uses_job_token_only(gitlab_uses_job_token_only)
}

#[test]
fn check_forge_preconditions_succeeds_for_github_pat() {
	use crate::forge::test_support::RecordingCodeForgeClient;
	let client =
		Arc::new(RecordingCodeForgeClient::new()) as Arc<dyn crate::forge::CodeForgeClient>;
	let env = minimal_env_with(client, false);
	check_forge_preconditions(&env).expect("GitHub PAT path should succeed");
}

#[test]
fn check_forge_preconditions_succeeds_for_gitlab_with_pat() {
	use crate::forge::test_support::RecordingCodeForgeClient;
	let client = Arc::new(RecordingCodeForgeClient::new().with_forge_name("GitLab"))
		as Arc<dyn crate::forge::CodeForgeClient>;
	let env = minimal_env_with(client, false);
	check_forge_preconditions(&env).expect("GitLab with PAT (job_token_only=false) should succeed");
}

#[test]
fn check_forge_preconditions_bails_for_gitlab_with_job_token_only() {
	use crate::forge::test_support::RecordingCodeForgeClient;
	let client = Arc::new(RecordingCodeForgeClient::new().with_forge_name("GitLab"))
		as Arc<dyn crate::forge::CodeForgeClient>;
	let env = minimal_env_with(client, true);
	let result = check_forge_preconditions(&env);
	assert!(result.is_err(), "GitLab + job-token-only must bail");
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("GITLAB_TOKEN"),
		"Expected error message to name GITLAB_TOKEN, got: {msg}"
	);
	assert!(
		msg.contains("CI_JOB_TOKEN"),
		"Expected error message to name CI_JOB_TOKEN, got: {msg}"
	);
}

#[test]
fn check_forge_preconditions_bails_when_client_unavailable() {
	let dir = tempfile::tempdir().unwrap();
	let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
	std::mem::forget(dir);
	let runner: Arc<dyn CommandRunner> = Arc::new(RecordingCommandRunner::new(0));
	let git = Arc::new(crate::git::GitWorkdir::new(Arc::clone(&runner), dir_abs));
	let env = crate::Env::new(runner, Arc::new(LocalFilesystem), git);
	// Default Env has no forge client configured (Err("No code forge client configured")).
	let result = check_forge_preconditions(&env);
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("code forge client is unavailable"),
		"Expected unavailable-client error, got: {msg}"
	);
}
