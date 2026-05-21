use crate::forge_resolution::gitlab::{gitlab_base_url_from, strip_scheme, validate_gitlab_host};

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
