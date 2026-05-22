//! GitLab forge-client construction at the binary boundary.
//!
//! Per ADR-056: token precedence is `GITLAB_TOKEN` (PAT) over `CI_JOB_TOKEN`
//! (CI job token); base-URL precedence is `CI_API_V4_URL` over `[gitlab].host`
//! over `gitlab.com`. The `CI_JOB_TOKEN`-only fail-fast for merge-request
//! creation is enforced at the prepare preflight, not here — the client is
//! constructed successfully whenever any token is available.

use std::sync::Arc;

use gitlab::{AsyncGitlab, GitlabBuilder};

use crate::env_helpers::env_first;

/// Shared GitLab connection handles built once at the binary boundary and
/// passed to both the [`cursus::git::GitLabSignedCommit`] decorator and the
/// [`cursus::forge::gitlab::ReqwestGitLabClient`] forge client so they share
/// a single underlying HTTP connection.
pub(crate) struct GitLabHandles {
	pub(crate) client: Arc<AsyncGitlab>,
	pub(crate) project: cursus::forge::gitlab::GitLabProject,
	/// `true` when the client was built from `CI_JOB_TOKEN` with no
	/// `GITLAB_TOKEN` PAT fallback. Used by the `prepare` preflight to fail
	/// fast before any merge-request API call (ADR-056 — `CI_JOB_TOKEN` cannot
	/// create or update merge requests).
	pub(crate) uses_job_token_only: bool,
}

/// Outcome of constructing a GitLab forge client, carrying the auth-kind
/// signal that callers need to apply token-scope preconditions later in the
/// pipeline.
pub(crate) struct GitLabClientOutcome {
	pub(crate) client: Arc<dyn cursus::forge::CodeForgeClient>,
	pub(crate) uses_job_token_only: bool,
}

/// Builds the GitLab `AsyncGitlab` HTTP client and resolves the project
/// identity from environment, config, and the git remote. Used by both
/// `build_git` (signed commits) and `resolve_gitlab_forge_client` (forge
/// client) so the underlying HTTP connection is shared.
///
/// Token precedence: `GITLAB_TOKEN` (project- or group-access PAT) first,
/// falling back to `CI_JOB_TOKEN` (GitLab CI).
///
/// Base URL precedence: `CI_API_V4_URL` (set on every GitLab CI job) →
/// `[gitlab].host` from config → `https://gitlab.com`.
///
/// Binary-boundary glue: reads env vars, builds an HTTP client, and resolves
/// the project identity from a real Git checkout. Excluded from coverage in
/// line with the `cursus-bin/src/main.rs` convention for IO entrypoints.
#[coverage(off)]
pub(crate) async fn gitlab_handles(
	git: &dyn cursus::git::Git,
	config: &Option<cursus::model::config::Config>,
) -> Result<GitLabHandles, String> {
	let cfg = config
		.as_ref()
		.ok_or_else(|| "No configuration file found".to_string())?;
	let project = cursus::forge::gitlab::GitLabProject::resolve(&cfg.gitlab, git)
		.await
		.map_err(|e| format!("{e:#}"))?;

	let pat = env_first(&["GITLAB_TOKEN"]);
	let job_token = env_first(&["CI_JOB_TOKEN"]);
	let host = gitlab_base_url(&cfg.gitlab.host);
	validate_gitlab_host(&host)?;
	let (token, token_kind, uses_job_token_only) = match (pat, job_token) {
		(Some(token), _) => (
			token,
			cursus::forge::gitlab::GitLabTokenKind::PersonalAccessToken,
			false,
		),
		(_, Some(token)) => (
			token,
			cursus::forge::gitlab::GitLabTokenKind::JobToken,
			true,
		),
		(None, None) => {
			return Err(
				"No GitLab token found (GITLAB_TOKEN, or CI_JOB_TOKEN for publish flows)"
					.to_string(),
			);
		}
	};
	let builder = match token_kind {
		cursus::forge::gitlab::GitLabTokenKind::PersonalAccessToken => {
			GitlabBuilder::new(&host, &token)
		}
		cursus::forge::gitlab::GitLabTokenKind::JobToken => {
			GitlabBuilder::new_with_job_token(&host, &token)
		}
	};
	let async_client = builder
		.build_async()
		.await
		.map_err(|e| format!("Failed to initialise GitLab client for host '{host}': {e:#}"))?;
	Ok(GitLabHandles {
		client: Arc::new(async_client),
		project,
		uses_job_token_only,
	})
}

/// Constructs the GitLab code forge client from pre-built [`GitLabHandles`].
///
/// Binary-boundary glue; excluded from coverage in line with the
/// `cursus-bin/src/main.rs` convention for IO entrypoints.
#[coverage(off)]
pub(crate) fn resolve_gitlab_forge_client_from_handles(
	handles: &GitLabHandles,
) -> GitLabClientOutcome {
	let client = cursus::forge::gitlab::ReqwestGitLabClient::new(
		(*handles.client).clone(),
		handles.project.clone(),
	);
	GitLabClientOutcome {
		client: Arc::new(client) as Arc<dyn cursus::forge::CodeForgeClient>,
		uses_job_token_only: handles.uses_job_token_only,
	}
}

/// Resolves the GitLab API base host the client should target by reading the
/// `CI_API_V4_URL` env var and the configured host.
///
/// Binary-boundary env-var reader; the precedence logic lives in
/// [`gitlab_base_url_from`] (which is fully tested). Excluded from coverage
/// in line with the `cursus-bin/src/main.rs` convention for IO entrypoints.
#[coverage(off)]
fn gitlab_base_url(config_host: &str) -> String {
	gitlab_base_url_from(env_first(&["CI_API_V4_URL"]).as_deref(), config_host)
}

/// Pure resolution of the GitLab API base host. Split from
/// [`gitlab_base_url`] so the precedence rules are unit-testable without
/// env-var manipulation.
///
/// `CI_API_V4_URL` (provided by every GitLab CI job and the most reliable
/// indicator of the correct base on self-managed instances) wins. Otherwise
/// the `[gitlab].host` config value is used. Empty fall back to `gitlab.com`.
pub(super) fn gitlab_base_url_from(ci_api_v4_url: Option<&str>, config_host: &str) -> String {
	if let Some(ci_url) = ci_api_v4_url {
		// CI_API_V4_URL ends in `/api/v4`; strip it to recover the bare host.
		let host = ci_url.trim_end_matches('/');
		let host = host.strip_suffix("/api/v4").unwrap_or(host);
		strip_scheme(host).to_string()
	} else if !config_host.trim().is_empty() {
		strip_scheme(config_host.trim().trim_end_matches('/')).to_string()
	} else {
		"gitlab.com".to_string()
	}
}

/// Strips a leading `https://` or `http://` from a URL-like host string.
pub(super) fn strip_scheme(s: &str) -> &str {
	s.strip_prefix("https://")
		.or_else(|| s.strip_prefix("http://"))
		.unwrap_or(s)
}

/// Validates that a resolved GitLab host contains only characters that are
/// safe to interpolate into the API base URL.
///
/// Mirrors the validation that `GitLabProject::new` applies to the host
/// stored on the project identity, applied here as defence-in-depth to the
/// independent path flowing into `GitlabBuilder::new` via `CI_API_V4_URL` or
/// `[gitlab].host`.
///
/// Accepts an optional `:<digits>` port suffix so self-managed GitLab
/// instances on non-standard ports (e.g. `gitlab.example.com:8443`) are
/// supported.
pub(super) fn validate_gitlab_host(host: &str) -> Result<(), String> {
	let (hostname, port) = match host.split_once(':') {
		Some((h, p)) => (h, Some(p)),
		None => (host, None),
	};
	// Reject hostnames containing more than one `:` — that would mean the
	// port segment itself contains `:`, which is never valid.
	if hostname.contains(':') || port.is_some_and(|p| p.contains(':')) {
		return Err(format!("Invalid GitLab host: {host:?}"));
	}
	if hostname.is_empty()
		|| hostname == "."
		|| hostname == ".."
		|| !hostname
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
	{
		return Err(format!("Invalid GitLab host: {host:?}"));
	}
	if let Some(p) = port
		&& (p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
	{
		return Err(format!("Invalid GitLab host: {host:?}"));
	}
	Ok(())
}
