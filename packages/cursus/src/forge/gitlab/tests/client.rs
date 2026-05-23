use gitlab::api::ApiError;

use crate::forge::gitlab::client::*;
use crate::forge::gitlab::remote::GitLabProject;

#[test]
fn sanitize_package_version_replaces_at() {
	assert_eq!(sanitize_package_version("pkg@1.0.0"), "pkg-1.0.0");
}

#[test]
fn sanitize_package_version_keeps_allowed_chars() {
	assert_eq!(
		sanitize_package_version("v1.0.0-rc.1+build"),
		"v1.0.0-rc.1+build"
	);
}

#[test]
fn sanitize_file_name_replaces_disallowed() {
	assert_eq!(sanitize_file_name("name with spaces"), "name-with-spaces");
}

#[test]
fn sanitize_file_name_keeps_allowed_chars() {
	assert_eq!(
		sanitize_file_name("cursus-x86_64-linux.tar.gz"),
		"cursus-x86_64-linux.tar.gz"
	);
}

#[test]
fn project_path_uses_group_and_project_with_slash() {
	let project = GitLabProject::new("gitlab.example.com", "acme/sub", "app").unwrap();
	assert_eq!(
		format!("{}/{}", project.group, project.project),
		"acme/sub/app"
	);
}

#[test]
fn package_file_url_encodes_slashes_in_project_path() {
	let project = GitLabProject::new("gitlab.example.com", "acme/sub", "app").unwrap();
	let path = format!("{}/{}", project.group, project.project);
	let encoded = percent_encode_path(&path);
	assert_eq!(encoded, "acme%2Fsub%2Fapp");
}

#[test]
fn compose_package_file_url_uses_project_scheme_and_host() {
	let project = GitLabProject::new("gitlab.example.com", "acme", "app").unwrap();
	let url = compose_package_file_url(&project, "v1.0.0", "asset.bin");
	assert_eq!(
		url,
		"https://gitlab.example.com/api/v4/projects/acme%2Fapp/packages/generic/release-assets/v1.0.0/asset.bin"
	);
}

#[test]
fn compose_package_file_url_preserves_http_scheme_for_self_managed_instances() {
	// HTTP-only self-managed instances must produce reachable asset URLs.
	let project = GitLabProject::new("gitlab.internal", "acme", "app")
		.unwrap()
		.with_scheme("http");
	let url = compose_package_file_url(&project, "v1.0.0", "asset.bin");
	assert!(
		url.starts_with("http://gitlab.internal/"),
		"expected http scheme, got: {url}"
	);
}

#[test]
fn compose_package_file_url_encodes_subgroup_paths() {
	let project = GitLabProject::new("gitlab.example.com", "acme/sub", "app").unwrap();
	let url = compose_package_file_url(&project, "v1.0.0", "asset.bin");
	assert!(
		url.contains("/projects/acme%2Fsub%2Fapp/packages/generic/"),
		"expected encoded subgroup path, got: {url}"
	);
}

#[test]
fn is_not_found_matches_404() {
	let err: ApiError<std::io::Error> = ApiError::GitlabWithStatus {
		status: 404u16.try_into().unwrap(),
		msg: "Not Found".to_string(),
	};
	assert!(is_not_found(&err));
}

#[test]
fn is_not_found_rejects_other_statuses() {
	let err: ApiError<std::io::Error> = ApiError::GitlabWithStatus {
		status: 500u16.try_into().unwrap(),
		msg: "boom".to_string(),
	};
	assert!(!is_not_found(&err));
}

#[test]
fn redact_api_error_strips_userinfo_from_embedded_urls() {
	// Simulate an upstream error string carrying a credentialed URL (e.g.
	// a misconfigured proxy URL the gitlab crate surfaces verbatim). The
	// returned anyhow error must not echo the userinfo.
	let err: ApiError<std::io::Error> = ApiError::GitlabWithStatus {
		status: 502u16.try_into().unwrap(),
		msg: "upstream failure at https://glpat-secret@proxy.example/api/v4".to_string(),
	};
	let redacted = redact_api_error(err);
	let text = format!("{redacted:#}");
	assert!(!text.contains("glpat-secret"), "credential leaked: {text}");
	assert!(
		text.contains("[REDACTED]"),
		"redaction marker missing: {text}"
	);
}
