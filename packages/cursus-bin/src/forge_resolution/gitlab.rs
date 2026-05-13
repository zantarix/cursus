//! GitLab forge-client construction at the binary boundary.
//!
//! Per ADR-056: token precedence is `GITLAB_TOKEN` (PAT) over `CI_JOB_TOKEN`
//! (CI job token); base-URL precedence is `CI_API_V4_URL` over `[gitlab].host`
//! over `gitlab.com`. The `CI_JOB_TOKEN`-only fail-fast for merge-request
//! creation is enforced at the prepare preflight, not here — the client is
//! constructed successfully whenever any token is available.

use std::sync::Arc;

use crate::env_helpers::env_first;

/// Outcome of constructing a GitLab forge client, carrying the auth-kind
/// signal that callers need to apply token-scope preconditions later in the
/// pipeline.
pub(crate) struct GitLabClientOutcome {
	pub(crate) client: Arc<dyn cursus::forge::CodeForgeClient>,
	/// `true` when the client was built from `CI_JOB_TOKEN` with no
	/// `GITLAB_TOKEN` PAT fallback. Used by the `prepare` preflight to fail
	/// fast before any merge-request API call (ADR-056 — `CI_JOB_TOKEN` cannot
	/// create or update merge requests).
	pub(crate) uses_job_token_only: bool,
}

/// Attempts to construct the GitLab code forge client from environment and config.
///
/// Token precedence: `GITLAB_TOKEN` (project- or group-access PAT) first,
/// falling back to `CI_JOB_TOKEN` (GitLab CI). When only `CI_JOB_TOKEN` is
/// available the client is still constructed (publish flows work fine with a
/// job token); the prepare preflight checks
/// [`cursus::Env::gitlab_uses_job_token_only`] before any merge-request API call.
///
/// Base URL precedence: `CI_API_V4_URL` (set on every GitLab CI job) →
/// `[gitlab].host` from config → `https://gitlab.com`.
pub(crate) async fn resolve_gitlab_forge_client(
	env: &cursus::Env,
	config: &Option<cursus::model::config::Config>,
) -> Result<GitLabClientOutcome, String> {
	let cfg = config
		.as_ref()
		.ok_or_else(|| "No configuration file found".to_string())?;
	let project = cursus::forge::gitlab::GitLabProject::resolve(&cfg.gitlab, env.git())
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
	let client =
		cursus::forge::gitlab::ReqwestGitLabClient::build(&host, &token, token_kind, project)
			.await
			.map_err(|e| format!("{e:#}"))?;
	Ok(GitLabClientOutcome {
		client: Arc::new(client) as Arc<dyn cursus::forge::CodeForgeClient>,
		uses_job_token_only,
	})
}

/// Resolves the GitLab API base host the client should target by reading the
/// `CI_API_V4_URL` env var and the configured host.
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
fn gitlab_base_url_from(ci_api_v4_url: Option<&str>, config_host: &str) -> String {
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
fn strip_scheme(s: &str) -> &str {
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
fn validate_gitlab_host(host: &str) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
	use super::{gitlab_base_url_from, strip_scheme, validate_gitlab_host};

	// ── strip_scheme ─────────────────────────────────────────────────────────

	#[test]
	fn strip_scheme_removes_https_prefix() {
		assert_eq!(
			strip_scheme("https://gitlab.example.com"),
			"gitlab.example.com"
		);
	}

	#[test]
	fn strip_scheme_removes_http_prefix() {
		assert_eq!(
			strip_scheme("http://gitlab.example.com"),
			"gitlab.example.com"
		);
	}

	#[test]
	fn strip_scheme_passes_through_bare_host() {
		assert_eq!(strip_scheme("gitlab.example.com"), "gitlab.example.com");
	}

	#[test]
	fn strip_scheme_does_not_strip_other_schemes() {
		assert_eq!(
			strip_scheme("ftp://gitlab.example.com"),
			"ftp://gitlab.example.com"
		);
	}

	// ── gitlab_base_url_from ─────────────────────────────────────────────────

	#[test]
	fn gitlab_base_url_ci_api_v4_url_takes_precedence() {
		let host = gitlab_base_url_from(
			Some("https://gitlab.example.com/api/v4"),
			"https://override.example.com",
		);
		assert_eq!(host, "gitlab.example.com");
	}

	#[test]
	fn gitlab_base_url_ci_api_v4_url_trailing_slash() {
		let host = gitlab_base_url_from(Some("https://gitlab.example.com/api/v4/"), "");
		assert_eq!(host, "gitlab.example.com");
	}

	#[test]
	fn gitlab_base_url_ci_api_v4_url_without_api_v4_suffix() {
		// `strip_suffix("/api/v4")` returns `None`, so the host falls through unchanged.
		let host = gitlab_base_url_from(Some("https://gitlab.example.com/"), "");
		assert_eq!(host, "gitlab.example.com");
	}

	#[test]
	fn gitlab_base_url_falls_back_to_config_host() {
		let host = gitlab_base_url_from(None, "https://gitlab.example.com/");
		assert_eq!(host, "gitlab.example.com");
	}

	#[test]
	fn gitlab_base_url_config_host_without_scheme() {
		let host = gitlab_base_url_from(None, "gitlab.example.com");
		assert_eq!(host, "gitlab.example.com");
	}

	#[test]
	fn gitlab_base_url_defaults_to_gitlab_com_when_empty() {
		assert_eq!(gitlab_base_url_from(None, ""), "gitlab.com");
		assert_eq!(gitlab_base_url_from(None, "   "), "gitlab.com");
	}

	// ── validate_gitlab_host ─────────────────────────────────────────────────

	#[test]
	fn validate_gitlab_host_accepts_alphanumeric_with_dots_and_hyphens() {
		assert!(validate_gitlab_host("gitlab.com").is_ok());
		assert!(validate_gitlab_host("gitlab.example.com").is_ok());
		assert!(validate_gitlab_host("self-managed.example.com").is_ok());
		assert!(validate_gitlab_host("a_b.example").is_ok());
	}

	#[test]
	fn validate_gitlab_host_rejects_empty() {
		assert!(validate_gitlab_host("").is_err());
	}

	#[test]
	fn validate_gitlab_host_rejects_dot_segments() {
		assert!(validate_gitlab_host(".").is_err());
		assert!(validate_gitlab_host("..").is_err());
	}

	#[test]
	fn validate_gitlab_host_rejects_slashes() {
		// A `/` in the host would smuggle path components into the URL template.
		assert!(validate_gitlab_host("evil.com/@gitlab.com").is_err());
		assert!(validate_gitlab_host("gitlab.com/").is_err());
	}

	#[test]
	fn validate_gitlab_host_accepts_explicit_port_form() {
		// Self-managed GitLab instances on non-standard ports flow through with
		// the port preserved; the validator allows a single `:<digits>` suffix.
		assert!(validate_gitlab_host("gitlab.example.com:8443").is_ok());
		assert!(validate_gitlab_host("gitlab.example.com:22").is_ok());
	}

	#[test]
	fn validate_gitlab_host_rejects_malformed_ports() {
		// Empty port, non-digit port, and double-colon forms must all fail.
		assert!(validate_gitlab_host("gitlab.example.com:").is_err());
		assert!(validate_gitlab_host("gitlab.example.com:abc").is_err());
		assert!(validate_gitlab_host("gitlab.example.com:80:443").is_err());
		assert!(validate_gitlab_host(":8443").is_err());
	}

	#[test]
	fn validate_gitlab_host_rejects_control_characters_and_spaces() {
		assert!(validate_gitlab_host("git lab.com").is_err());
		assert!(validate_gitlab_host("git\nlab.com").is_err());
	}
}
