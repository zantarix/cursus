use std::sync::Arc;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::forge::github::remote::*;
use crate::git::GitWorkdir;
use crate::model::config::GitHubConfig;

fn workdir() -> crate::path::AbsolutePath {
	crate::path::AbsolutePath::new("/tmp").unwrap()
}

// --- GitHubRepo::parse_url ---

#[tokio::test]
async fn parse_https_with_git_suffix() {
	let result = GitHubRepo::parse_url("https://github.com/owner/repo.git");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_https_without_git_suffix() {
	let result = GitHubRepo::parse_url("https://github.com/owner/repo");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_ssh_with_git_suffix() {
	let result = GitHubRepo::parse_url("git@github.com:owner/repo.git");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_ssh_without_git_suffix() {
	let result = GitHubRepo::parse_url("git@github.com:owner/repo");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_non_github_https_returns_none() {
	assert!(GitHubRepo::parse_url("https://gitlab.com/owner/repo.git").is_none());
}

#[tokio::test]
async fn parse_non_github_ssh_returns_none() {
	assert!(GitHubRepo::parse_url("git@gitlab.com:owner/repo.git").is_none());
}

#[tokio::test]
async fn parse_empty_returns_none() {
	assert!(GitHubRepo::parse_url("").is_none());
}

#[tokio::test]
async fn parse_malformed_returns_none() {
	assert!(GitHubRepo::parse_url("not-a-url").is_none());
}

#[tokio::test]
async fn parse_extra_path_segments_returns_none() {
	assert!(GitHubRepo::parse_url("https://github.com/owner/repo/extra").is_none());
}

#[tokio::test]
async fn parse_ssh_extra_path_segments_returns_none() {
	assert!(GitHubRepo::parse_url("git@github.com:owner/repo/extra").is_none());
}

#[tokio::test]
async fn parse_trailing_slash_returns_none() {
	// Trailing slash is not a standard git remote format; reject it.
	assert!(GitHubRepo::parse_url("https://github.com/owner/repo/").is_none());
}

#[tokio::test]
async fn parse_ssh_url_with_git_suffix() {
	let result = GitHubRepo::parse_url("ssh://git@github.com/owner/repo.git");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_ssh_url_without_git_suffix() {
	let result = GitHubRepo::parse_url("ssh://git@github.com/owner/repo");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_ssh_url_without_user() {
	let result = GitHubRepo::parse_url("ssh://github.com/owner/repo.git");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_ssh_url_with_port() {
	let result = GitHubRepo::parse_url("ssh://git@github.com:22/owner/repo.git");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_https_with_port() {
	let result = GitHubRepo::parse_url("https://github.com:443/owner/repo.git");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_https_with_port_no_git_suffix() {
	let result = GitHubRepo::parse_url("https://github.com:8080/owner/repo");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

#[tokio::test]
async fn parse_https_colon_no_digits_returns_none() {
	assert!(GitHubRepo::parse_url("https://github.com:/owner/repo").is_none());
}

#[tokio::test]
async fn parse_ssh_url_non_github_returns_none() {
	assert!(GitHubRepo::parse_url("ssh://git@gitlab.com/owner/repo.git").is_none());
}

#[tokio::test]
async fn parse_trims_whitespace() {
	let result = GitHubRepo::parse_url("  https://github.com/owner/repo.git\n");
	assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
}

// --- GitHubRepo::detect_in ---

#[tokio::test]
async fn detect_returns_repo_for_https_remote() {
	let runner = Arc::new(
		RecordingCommandRunner::new(0).with_stdout(b"https://github.com/acme/app.git\n".to_vec()),
	);
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitHubRepo::detect_in(&git).await.unwrap();
	assert_eq!(result, Some(GitHubRepo::new("acme", "app").unwrap()));
	let invocations = runner.invocations();
	assert_eq!(invocations.len(), 1);
	assert_eq!(invocations[0].program, "git");
	assert_eq!(invocations[0].args, ["remote", "get-url", "origin"]);
}

#[tokio::test]
async fn detect_returns_repo_for_ssh_remote() {
	let runner = Arc::new(
		RecordingCommandRunner::new(0).with_stdout(b"git@github.com:acme/app.git\n".to_vec()),
	);
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitHubRepo::detect_in(&git).await.unwrap();
	assert_eq!(result, Some(GitHubRepo::new("acme", "app").unwrap()));
}

#[tokio::test]
async fn detect_returns_none_when_git_fails() {
	let runner = Arc::new(RecordingCommandRunner::new(1));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitHubRepo::detect_in(&git).await.unwrap();
	assert_eq!(result, None);
}

#[tokio::test]
async fn detect_returns_none_for_non_github_url() {
	let runner = Arc::new(
		RecordingCommandRunner::new(0).with_stdout(b"https://gitlab.com/owner/repo.git\n".to_vec()),
	);
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitHubRepo::detect_in(&git).await.unwrap();
	assert_eq!(result, None);
}

// --- GitHubRepo::resolve ---

fn make_github_config(owner: Option<&str>, repo: Option<&str>) -> GitHubConfig {
	let mut config = GitHubConfig::enabled_config();
	if let Some(o) = owner {
		config = config.with_owner(o.to_string());
	}
	if let Some(r) = repo {
		config = config.with_repo(r.to_string());
	}
	config
}

#[tokio::test]
async fn resolve_github_repo_uses_config_when_set() {
	let config = make_github_config(Some("acme"), Some("app"));
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());

	let gh_repo = GitHubRepo::resolve(&config, &git).await.unwrap();
	assert_eq!(gh_repo.owner, "acme");
	assert_eq!(gh_repo.repo, "app");
	// Config values take priority — no git command should run
	assert!(runner.invocations().is_empty());
}

#[tokio::test]
async fn resolve_github_repo_falls_back_to_git_remote() {
	let config = make_github_config(None, None);
	let runner = Arc::new(
		RecordingCommandRunner::new(0)
			.with_stdout(b"https://github.com/myorg/myapp.git\n".to_vec()),
	);
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());

	let gh_repo = GitHubRepo::resolve(&config, &git).await.unwrap();
	assert_eq!(gh_repo.owner, "myorg");
	assert_eq!(gh_repo.repo, "myapp");
}

#[tokio::test]
async fn resolve_github_repo_errors_when_neither_config_nor_remote() {
	let config = make_github_config(None, None);
	let runner = Arc::new(RecordingCommandRunner::new(1)); // no origin remote
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());

	let result = GitHubRepo::resolve(&config, &git).await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Could not determine GitHub repository"),
		"Expected repo detection error, got: {msg}"
	);
}

#[tokio::test]
async fn resolve_github_repo_errors_when_only_owner_set() {
	let config = make_github_config(Some("acme"), None);
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());

	let result = GitHubRepo::resolve(&config, &git).await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("must be set together"),
		"Expected partial config error, got: {msg}"
	);
}

#[tokio::test]
async fn resolve_github_repo_errors_when_only_repo_set() {
	let config = make_github_config(None, Some("app"));
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());

	let result = GitHubRepo::resolve(&config, &git).await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("must be set together"),
		"Expected partial config error, got: {msg}"
	);
}

// --- GitHubRepo::new validation ---

#[tokio::test]
async fn new_accepts_valid_names() {
	assert!(GitHubRepo::new("acme", "my-repo").is_ok());
	assert!(GitHubRepo::new("my-org", "my_repo.js").is_ok());
	assert!(GitHubRepo::new("Org123", "repo").is_ok());
}

#[tokio::test]
async fn new_rejects_invalid_owner() {
	assert!(GitHubRepo::new("", "repo").is_err());
	assert!(GitHubRepo::new("a/b", "repo").is_err());
	assert!(GitHubRepo::new("../evil", "repo").is_err());
	assert!(GitHubRepo::new("a b", "repo").is_err());
}

#[tokio::test]
async fn new_rejects_invalid_repo() {
	assert!(GitHubRepo::new("owner", "").is_err());
	assert!(GitHubRepo::new("owner", "a/b").is_err());
	assert!(GitHubRepo::new("owner", "../evil").is_err());
}
