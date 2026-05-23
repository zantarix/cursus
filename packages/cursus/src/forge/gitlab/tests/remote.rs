use std::sync::Arc;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::forge::gitlab::remote::*;
use crate::git::GitWorkdir;
use crate::model::config::GitLabConfig;

fn workdir() -> crate::path::AbsolutePath {
	crate::path::AbsolutePath::new("/tmp").unwrap()
}

// --- GitLabProject::parse_url ---

#[tokio::test]
async fn parse_https_gitlab_com() {
	let result = GitLabProject::parse_url("https://gitlab.com/acme/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_https_self_managed() {
	let result = GitLabProject::parse_url("https://gitlab.example.com/acme/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_https_subgroup() {
	let result = GitLabProject::parse_url("https://gitlab.com/acme/sub/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.com", "acme/sub", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_https_deep_subgroup() {
	let result = GitLabProject::parse_url("https://gitlab.com/a/b/c/d/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.com", "a/b/c/d", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_https_without_git_suffix() {
	let result = GitLabProject::parse_url("https://gitlab.com/acme/app");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_https_with_port_preserves_port() {
	let result = GitLabProject::parse_url("https://gitlab.example.com:8443/acme/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.example.com:8443", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_https_no_group_returns_none() {
	// `host/project` alone — no group separator.
	assert!(GitLabProject::parse_url("https://gitlab.com/app.git").is_none());
}

#[tokio::test]
async fn parse_scp_gitlab_com() {
	let result = GitLabProject::parse_url("git@gitlab.com:acme/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_scp_self_managed() {
	let result = GitLabProject::parse_url("git@gitlab.example.com:acme/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_scp_subgroup() {
	let result = GitLabProject::parse_url("git@gitlab.com:acme/sub/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.com", "acme/sub", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_ssh_url_with_user() {
	let result = GitLabProject::parse_url("ssh://git@gitlab.example.com/acme/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_ssh_url_without_user() {
	let result = GitLabProject::parse_url("ssh://gitlab.example.com/acme/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_ssh_url_with_port_preserves_port() {
	let result = GitLabProject::parse_url("ssh://git@gitlab.example.com:2222/acme/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.example.com:2222", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_ssh_url_subgroup() {
	let result = GitLabProject::parse_url("ssh://git@gitlab.example.com/acme/sub/app.git");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.example.com", "acme/sub", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_empty_returns_none() {
	assert!(GitLabProject::parse_url("").is_none());
}

#[tokio::test]
async fn parse_unsupported_scheme_returns_none() {
	assert!(GitLabProject::parse_url("ftp://gitlab.example.com/acme/app.git").is_none());
}

#[tokio::test]
async fn parse_trims_whitespace() {
	let result = GitLabProject::parse_url("  git@gitlab.com:acme/app.git\n");
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn parse_https_malformed_port_returns_none() {
	assert!(GitLabProject::parse_url("https://gitlab.com:/acme/app.git").is_none());
}

// --- GitLabProject scheme handling ---

#[tokio::test]
async fn new_defaults_to_https_scheme() {
	let project = GitLabProject::new("gitlab.com", "acme", "app").unwrap();
	assert_eq!(project.scheme, "https");
}

#[tokio::test]
async fn with_scheme_accepts_https_and_http() {
	let project = GitLabProject::new("gitlab.com", "acme", "app").unwrap();
	assert_eq!(project.clone().with_scheme("https").scheme, "https");
	assert_eq!(project.with_scheme("http").scheme, "http");
}

#[tokio::test]
async fn with_scheme_ignores_unknown_schemes() {
	// A typo or attacker-controlled config value cannot inject `ftp` or
	// `file` into the composed asset URL — unknown schemes leave the field
	// at its previous value.
	let project = GitLabProject::new("gitlab.com", "acme", "app").unwrap();
	let updated = project.with_scheme("ftp");
	assert_eq!(updated.scheme, "https");
}

#[tokio::test]
async fn parse_url_preserves_http_scheme() {
	let project = GitLabProject::parse_url("http://gitlab.internal/acme/app.git").unwrap();
	assert_eq!(project.scheme, "http");
	assert_eq!(project.host, "gitlab.internal");
}

#[tokio::test]
async fn parse_url_defaults_https_for_ssh_remotes() {
	// SSH-cloned remotes still surface assets over HTTPS in practice.
	let project = GitLabProject::parse_url("git@gitlab.example.com:acme/app.git").unwrap();
	assert_eq!(project.scheme, "https");
}

// --- GitLabProject::new validation ---

#[tokio::test]
async fn new_accepts_valid_segments() {
	assert!(GitLabProject::new("gitlab.com", "acme", "my-app").is_ok());
	assert!(GitLabProject::new("gitlab.example.com", "acme/sub", "app.svc").is_ok());
	assert!(GitLabProject::new("gitlab.com", "Org123", "repo").is_ok());
}

#[tokio::test]
async fn new_accepts_explicit_port_in_host() {
	// Self-managed GitLab instances on non-standard ports.
	assert!(GitLabProject::new("gitlab.example.com:8443", "acme", "app").is_ok());
	assert!(GitLabProject::new("gitlab.example.com:22", "acme", "app").is_ok());
}

#[tokio::test]
async fn new_rejects_invalid_host() {
	assert!(GitLabProject::new("", "acme", "app").is_err());
	assert!(GitLabProject::new("git lab.com", "acme", "app").is_err());
	assert!(GitLabProject::new("../evil", "acme", "app").is_err());
}

#[tokio::test]
async fn new_rejects_malformed_port_in_host() {
	// Empty port, non-digit port, and double-colon forms must all fail.
	assert!(GitLabProject::new("gitlab.example.com:", "acme", "app").is_err());
	assert!(GitLabProject::new("gitlab.example.com:abc", "acme", "app").is_err());
	assert!(GitLabProject::new("gitlab.example.com:80:443", "acme", "app").is_err());
}

#[tokio::test]
async fn new_rejects_invalid_group() {
	assert!(GitLabProject::new("gitlab.com", "", "app").is_err());
	assert!(GitLabProject::new("gitlab.com", "ac me", "app").is_err());
	assert!(GitLabProject::new("gitlab.com", "ac/../evil", "app").is_err());
}

#[tokio::test]
async fn new_rejects_invalid_project() {
	assert!(GitLabProject::new("gitlab.com", "acme", "").is_err());
	assert!(GitLabProject::new("gitlab.com", "acme", "a/b").is_err());
	assert!(GitLabProject::new("gitlab.com", "acme", "../evil").is_err());
}

// --- GitLabProject::detect_in ---

#[tokio::test]
async fn detect_returns_project_for_https_remote() {
	let runner = Arc::new(
		RecordingCommandRunner::new(0).with_stdout(b"https://gitlab.com/acme/app.git\n".to_vec()),
	);
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitLabProject::detect_in(&git).await.unwrap();
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
	);
}

#[tokio::test]
async fn detect_returns_project_for_ssh_remote() {
	let runner = Arc::new(
		RecordingCommandRunner::new(0)
			.with_stdout(b"git@gitlab.example.com:acme/sub/app.git\n".to_vec()),
	);
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitLabProject::detect_in(&git).await.unwrap();
	assert_eq!(
		result,
		Some(GitLabProject::new("gitlab.example.com", "acme/sub", "app").unwrap())
	);
}

#[tokio::test]
async fn detect_returns_none_when_git_fails() {
	let runner = Arc::new(RecordingCommandRunner::new(1));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitLabProject::detect_in(&git).await.unwrap();
	assert_eq!(result, None);
}

// --- GitLabProject::resolve ---

fn make_config(group: Option<&str>, project: Option<&str>, host: &str) -> GitLabConfig {
	let mut config = GitLabConfig::enabled_config().with_host(host.to_string());
	if let Some(g) = group {
		config = config.with_group(g.to_string());
	}
	if let Some(p) = project {
		config = config.with_project(p.to_string());
	}
	config
}

#[tokio::test]
async fn resolve_uses_config_when_set() {
	let config = make_config(Some("acme"), Some("app"), "");
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let project = GitLabProject::resolve(&config, &git).await.unwrap();
	assert_eq!(project.host, "gitlab.com");
	assert_eq!(project.group, "acme");
	assert_eq!(project.project, "app");
	assert!(runner.invocations().is_empty());
}

#[tokio::test]
async fn resolve_uses_config_host_when_set() {
	let config = make_config(Some("acme"), Some("app"), "https://gitlab.example.com/");
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let project = GitLabProject::resolve(&config, &git).await.unwrap();
	assert_eq!(project.host, "gitlab.example.com");
	assert_eq!(project.scheme, "https");
}

#[tokio::test]
async fn resolve_preserves_http_scheme_from_config_host() {
	// Self-managed instances served over plain HTTP must propagate the
	// scheme through `resolve` so callers that do not perform the
	// binary-boundary endpoint override still produce reachable URLs.
	let config = make_config(Some("acme"), Some("app"), "http://gitlab.internal");
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let project = GitLabProject::resolve(&config, &git).await.unwrap();
	assert_eq!(project.host, "gitlab.internal");
	assert_eq!(project.scheme, "http");
}

#[tokio::test]
async fn resolve_subgroup_from_config() {
	let config = make_config(Some("acme/sub"), Some("app"), "");
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let project = GitLabProject::resolve(&config, &git).await.unwrap();
	assert_eq!(project.group, "acme/sub");
}

#[tokio::test]
async fn resolve_falls_back_to_git_remote() {
	let config = make_config(None, None, "");
	let runner = Arc::new(
		RecordingCommandRunner::new(0)
			.with_stdout(b"https://gitlab.com/myorg/myapp.git\n".to_vec()),
	);
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let project = GitLabProject::resolve(&config, &git).await.unwrap();
	assert_eq!(project.host, "gitlab.com");
	assert_eq!(project.group, "myorg");
	assert_eq!(project.project, "myapp");
}

#[tokio::test]
async fn resolve_errors_when_neither_config_nor_remote() {
	let config = make_config(None, None, "");
	let runner = Arc::new(RecordingCommandRunner::new(1));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitLabProject::resolve(&config, &git).await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Could not determine GitLab project"),
		"Expected project detection error, got: {msg}"
	);
}

#[tokio::test]
async fn resolve_errors_when_only_group_set() {
	let config = make_config(Some("acme"), None, "");
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitLabProject::resolve(&config, &git).await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("must be set together"),
		"Expected partial config error, got: {msg}"
	);
}

#[tokio::test]
async fn resolve_errors_when_only_project_set() {
	let config = make_config(None, Some("app"), "");
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let wd = workdir();
	let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
	let result = GitLabProject::resolve(&config, &git).await;
	assert!(result.is_err());
}

// --- scheme_and_host_from_config ---

#[test]
fn host_from_empty_returns_gitlab_com() {
	assert_eq!(
		scheme_and_host_from_config(""),
		("https".to_string(), "gitlab.com".to_string())
	);
}

#[test]
fn host_from_whitespace_returns_gitlab_com() {
	assert_eq!(
		scheme_and_host_from_config("   "),
		("https".to_string(), "gitlab.com".to_string())
	);
}

#[test]
fn host_strips_https_scheme() {
	assert_eq!(
		scheme_and_host_from_config("https://gitlab.example.com"),
		("https".to_string(), "gitlab.example.com".to_string())
	);
}

#[test]
fn host_preserves_http_scheme() {
	assert_eq!(
		scheme_and_host_from_config("http://gitlab.internal"),
		("http".to_string(), "gitlab.internal".to_string())
	);
}

#[test]
fn host_strips_trailing_slash() {
	assert_eq!(
		scheme_and_host_from_config("https://gitlab.example.com/"),
		("https".to_string(), "gitlab.example.com".to_string())
	);
}

#[test]
fn host_passes_through_bare_host() {
	assert_eq!(
		scheme_and_host_from_config("gitlab.example.com"),
		("https".to_string(), "gitlab.example.com".to_string())
	);
}

#[test]
fn host_strips_unknown_scheme_and_defaults_to_https() {
	// Mirrors `split_scheme` semantics so library-layer and binary-layer
	// scheme handling stay consistent. The bare host that comes back then
	// fails validation cleanly inside `GitLabProject::new`.
	assert_eq!(
		scheme_and_host_from_config("ftp://gitlab.internal"),
		("https".to_string(), "gitlab.internal".to_string())
	);
}
