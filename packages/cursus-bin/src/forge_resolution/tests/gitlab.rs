use cursus::forge::gitlab::GitLabProject;

use crate::forge_resolution::gitlab::{
	gitlab_endpoint_from, pin_endpoint_on_project, split_scheme, validate_gitlab_host,
};

// ── split_scheme ─────────────────────────────────────────────────────────

#[test]
fn split_scheme_extracts_https_prefix() {
	assert_eq!(
		split_scheme("https://gitlab.example.com"),
		("https".to_string(), "gitlab.example.com".to_string())
	);
}

#[test]
fn split_scheme_extracts_http_prefix() {
	assert_eq!(
		split_scheme("http://gitlab.example.com"),
		("http".to_string(), "gitlab.example.com".to_string())
	);
}

#[test]
fn split_scheme_defaults_to_https_for_bare_host() {
	assert_eq!(
		split_scheme("gitlab.example.com"),
		("https".to_string(), "gitlab.example.com".to_string())
	);
}

#[test]
fn split_scheme_strips_unknown_scheme_and_defaults_to_https() {
	// Unknown schemes (e.g. ftp://) are stripped so downstream host
	// validation rejects the bare host cleanly rather than echoing the
	// bogus scheme back in the error message.
	assert_eq!(
		split_scheme("ftp://gitlab.example.com"),
		("https".to_string(), "gitlab.example.com".to_string())
	);
}

// ── gitlab_endpoint_from ─────────────────────────────────────────────────

#[test]
fn gitlab_endpoint_ci_api_v4_url_takes_precedence() {
	let (scheme, host) = gitlab_endpoint_from(
		Some("https://gitlab.example.com/api/v4"),
		"https://override.example.com",
	);
	assert_eq!(scheme, "https");
	assert_eq!(host, "gitlab.example.com");
}

#[test]
fn gitlab_endpoint_ci_api_v4_url_trailing_slash() {
	let (scheme, host) = gitlab_endpoint_from(Some("https://gitlab.example.com/api/v4/"), "");
	assert_eq!(scheme, "https");
	assert_eq!(host, "gitlab.example.com");
}

#[test]
fn gitlab_endpoint_ci_api_v4_url_without_api_v4_suffix() {
	// `strip_suffix("/api/v4")` returns `None`, so the host falls through unchanged.
	let (scheme, host) = gitlab_endpoint_from(Some("https://gitlab.example.com/"), "");
	assert_eq!(scheme, "https");
	assert_eq!(host, "gitlab.example.com");
}

#[test]
fn gitlab_endpoint_ci_api_v4_url_http_scheme_preserved() {
	// Self-managed GitLab CI instances on plain HTTP must propagate the
	// scheme so the API client and asset URLs agree.
	let (scheme, host) = gitlab_endpoint_from(Some("http://gitlab.internal/api/v4"), "");
	assert_eq!(scheme, "http");
	assert_eq!(host, "gitlab.internal");
}

#[test]
fn gitlab_endpoint_falls_back_to_config_host() {
	let (scheme, host) = gitlab_endpoint_from(None, "https://gitlab.example.com/");
	assert_eq!(scheme, "https");
	assert_eq!(host, "gitlab.example.com");
}

#[test]
fn gitlab_endpoint_config_host_http_scheme_preserved() {
	let (scheme, host) = gitlab_endpoint_from(None, "http://gitlab.internal");
	assert_eq!(scheme, "http");
	assert_eq!(host, "gitlab.internal");
}

#[test]
fn gitlab_endpoint_config_host_without_scheme() {
	let (scheme, host) = gitlab_endpoint_from(None, "gitlab.example.com");
	assert_eq!(scheme, "https");
	assert_eq!(host, "gitlab.example.com");
}

#[test]
fn gitlab_endpoint_defaults_to_gitlab_com_when_empty() {
	assert_eq!(
		gitlab_endpoint_from(None, ""),
		("https".to_string(), "gitlab.com".to_string())
	);
	assert_eq!(
		gitlab_endpoint_from(None, "   "),
		("https".to_string(), "gitlab.com".to_string())
	);
}

// ── pin_endpoint_on_project ──────────────────────────────────────────────

#[test]
fn pin_endpoint_overrides_resolved_host_and_scheme() {
	// `CI_API_V4_URL` says HTTPS at one host, but the resolved project
	// (e.g. from a stale mirror remote) points at another. The pin must
	// overwrite both so asset URLs target the host the upload actually
	// went to.
	let resolved = GitLabProject::new("mirror.example.com", "acme", "app").unwrap();
	let pinned = pin_endpoint_on_project("https", "gitlab.example.com", resolved);
	assert_eq!(pinned.scheme, "https");
	assert_eq!(pinned.host, "gitlab.example.com");
	assert_eq!(pinned.group, "acme");
	assert_eq!(pinned.project, "app");
}

#[test]
fn pin_endpoint_downgrades_scheme_when_endpoint_is_http() {
	// `CI_API_V4_URL=http://gitlab.internal/api/v4` against an HTTPS-default
	// resolved project — the API client will use HTTP, so asset URLs must
	// match.
	let resolved = GitLabProject::new("gitlab.example.com", "acme", "app").unwrap();
	assert_eq!(resolved.scheme, "https");
	let pinned = pin_endpoint_on_project("http", "gitlab.internal", resolved);
	assert_eq!(pinned.scheme, "http");
	assert_eq!(pinned.host, "gitlab.internal");
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
