//! Integration tests for [`ReqwestGitLabClient`] against a mock HTTP server.
//!
//! These tests verify that the gitlab-crate-backed client correctly maps API
//! responses to `CodeForgeClient` trait return values and propagates errors.
//! They exercise the trait surface, not every endpoint variant.

use httpmock::prelude::*;
use serde_json::json;

use cursus::forge::CodeForgeClient;
use cursus::forge::gitlab::{GitLabProject, GitLabTokenKind, ReqwestGitLabClient};

/// Builds a `ReqwestGitLabClient` pointing at the given mock HTTP server.
///
/// Uses a Personal Access Token and the `insecure()` builder path (HTTP
/// rather than HTTPS) so the mock can serve requests without TLS. Also
/// installs the `/api/v4/user` mock the gitlab crate hits during
/// `build_async()` to verify the connection works.
async fn mock_client(server: &MockServer) -> ReqwestGitLabClient {
	server.mock(|when, then| {
		when.method(GET).path("/api/v4/user");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(json!({"id": 1, "username": "test"}).to_string());
	});
	let host = server
		.base_url()
		.trim_start_matches("http://")
		.trim_start_matches("https://")
		.to_string();
	let project = GitLabProject::new("gitlab.example.com", "acme", "app").unwrap();
	let mut builder = gitlab::GitlabBuilder::new(&host, "test-token");
	builder.insecure();
	let async_client = builder.build_async().await.unwrap();
	ReqwestGitLabClient::new(async_client, project)
}

// ── create_release ────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_release_returns_tag_as_id() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v4/projects/acme%2Fapp/releases");
		then.status(201)
			.header("Content-Type", "application/json")
			.body(
				json!({
					"tag_name": "v1.0.0",
					"name": "Release 1.0.0",
					"description": "Body"
				})
				.to_string(),
			);
	});

	let client = mock_client(&server).await;
	let id = client
		.create_release("v1.0.0", "Release 1.0.0", "Body")
		.await
		.unwrap();
	assert_eq!(id, "v1.0.0");
}

#[tokio::test]
async fn create_release_propagates_api_error() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v4/projects/acme%2Fapp/releases");
		then.status(400)
			.header("Content-Type", "application/json")
			.body(json!({"message": "Bad Request"}).to_string());
	});

	let client = mock_client(&server).await;
	let result = client.create_release("v1.0.0", "Release", "Body").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to create GitLab release"),
		"Error should contain GitLab vocabulary, got: {msg}"
	);
}

// ── find_release_by_tag ───────────────────────────────────────────────────────

#[tokio::test]
async fn find_release_by_tag_returns_some_on_hit() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/acme%2Fapp/releases/v1.0.0");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(json!({"tag_name": "v1.0.0"}).to_string());
	});

	let client = mock_client(&server).await;
	let result = client.find_release_by_tag("v1.0.0").await.unwrap();
	let release = result.expect("release should be Some when GitLab returns 200");
	assert_eq!(release.id, "v1.0.0");
	assert!(
		!release.is_draft,
		"GitLab releases never report is_draft=true (no draft concept)"
	);
}

#[tokio::test]
async fn find_release_by_tag_returns_none_on_404() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/acme%2Fapp/releases/missing");
		then.status(404)
			.header("Content-Type", "application/json")
			.body(json!({"message": "404 Not Found"}).to_string());
	});

	let client = mock_client(&server).await;
	let result = client.find_release_by_tag("missing").await.unwrap();
	assert!(
		result.is_none(),
		"find_release_by_tag must map HTTP 404 to Ok(None) (ADR-055)"
	);
}

#[tokio::test]
async fn find_release_by_tag_propagates_non_404_errors() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/acme%2Fapp/releases/v1.0.0");
		then.status(500)
			.header("Content-Type", "application/json")
			.body(json!({"message": "boom"}).to_string());
	});

	let client = mock_client(&server).await;
	let result = client.find_release_by_tag("v1.0.0").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to look up GitLab release"),
		"Error should use GitLab vocabulary, got: {msg}"
	);
}

// ── publish_release ────────────────────────────────────────────────────────────

#[tokio::test]
async fn publish_release_is_a_noop_for_gitlab() {
	let server = MockServer::start();
	// Intentionally configure no mocks; if the GitLab client tries to issue an
	// HTTP request from publish_release, the mock server will return 404 and the
	// test will fail. The expected behaviour is no network traffic at all.
	let client = mock_client(&server).await;
	let result = client.publish_release("v1.0.0").await;
	assert!(
		result.is_ok(),
		"publish_release must be a no-op on GitLab (ADR-056): {result:?}"
	);
}

// ── create_pull_request → CreateMergeRequest ───────────────────────────────────

#[tokio::test]
async fn create_pull_request_creates_merge_request_and_returns_url() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v4/projects/acme%2Fapp/merge_requests")
			.body_includes("source_branch=release-branch")
			.body_includes("target_branch=main")
			.body_includes("title=Release+updates");
		then.status(201)
			.header("Content-Type", "application/json")
			.body(
				json!({
					"iid": 42,
					"web_url": "https://gitlab.example.com/acme/app/-/merge_requests/42"
				})
				.to_string(),
			);
	});

	let client = mock_client(&server).await;
	let url = client
		.create_pull_request("Release updates", "Body", "release-branch", "main")
		.await
		.unwrap();
	mock.assert();
	assert_eq!(
		url,
		"https://gitlab.example.com/acme/app/-/merge_requests/42"
	);
}

#[tokio::test]
async fn create_pull_request_propagates_api_error_with_gitlab_vocab() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v4/projects/acme%2Fapp/merge_requests");
		then.status(409)
			.header("Content-Type", "application/json")
			.body(json!({"message": "duplicate"}).to_string());
	});

	let client = mock_client(&server).await;
	let result = client
		.create_pull_request("Release updates", "Body", "release-branch", "main")
		.await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to create merge request"),
		"Error should say 'merge request', got: {msg}"
	);
}

// ── find_open_pull_request → MergeRequests list ───────────────────────────────

#[tokio::test]
async fn find_open_pull_request_returns_first_open_mr() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/acme%2Fapp/merge_requests")
			.query_param("source_branch", "release-branch")
			.query_param("state", "opened");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(
				json!([{
					"iid": 7,
					"web_url": "https://gitlab.example.com/acme/app/-/merge_requests/7"
				}])
				.to_string(),
			);
	});

	let client = mock_client(&server).await;
	let result = client
		.find_open_pull_request("release-branch")
		.await
		.unwrap();
	let pr = result.expect("should return Some(_)");
	assert_eq!(pr.number, 7);
	assert!(pr.html_url.contains("/merge_requests/7"));
}

#[tokio::test]
async fn find_open_pull_request_returns_none_when_list_empty() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/acme%2Fapp/merge_requests");
		then.status(200)
			.header("Content-Type", "application/json")
			.body("[]");
	});

	let client = mock_client(&server).await;
	let result = client
		.find_open_pull_request("release-branch")
		.await
		.unwrap();
	assert!(result.is_none());
}

// ── update_pull_request → EditMergeRequest ────────────────────────────────────

#[tokio::test]
async fn update_pull_request_uses_iid_in_path() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(PUT)
			.path("/api/v4/projects/acme%2Fapp/merge_requests/42")
			.body_includes("title=Updated+Title");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(
				json!({
					"iid": 42,
					"web_url": "https://gitlab.example.com/acme/app/-/merge_requests/42"
				})
				.to_string(),
			);
	});

	let client = mock_client(&server).await;
	let url = client
		.update_pull_request(42, "Updated Title", "Updated body")
		.await
		.unwrap();
	mock.assert();
	assert_eq!(
		url,
		"https://gitlab.example.com/acme/app/-/merge_requests/42"
	);
}

// ── upload_asset → two-step generic-package + release-link ────────────────────

#[tokio::test]
async fn upload_asset_uploads_package_then_creates_release_link() {
	let server = MockServer::start();

	let upload_mock = server.mock(|when, then| {
		when.method(PUT)
			.path_includes("/api/v4/projects/acme%2Fapp/packages/generic/release-assets/v1.0.0/");
		then.status(201)
			.header("Content-Type", "application/json")
			.body("{}");
	});

	let link_mock = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v4/projects/acme%2Fapp/releases/v1.0.0/assets/links")
			.body_includes("name=linux-amd64");
		then.status(201)
			.header("Content-Type", "application/json")
			.body(
				json!({"id": 1, "name": "linux-amd64", "url": "https://example.com"}).to_string(),
			);
	});

	let tmp = tempfile::NamedTempFile::new().unwrap();
	std::fs::write(tmp.path(), b"binary contents").unwrap();

	let client = mock_client(&server).await;
	let result = client
		.upload_asset("v1.0.0", "linux-amd64", tmp.path())
		.await;
	assert!(result.is_ok(), "upload_asset failed: {result:?}");
	upload_mock.assert();
	link_mock.assert();
}

#[tokio::test]
async fn upload_asset_errors_on_missing_file() {
	let server = MockServer::start();
	let client = mock_client(&server).await;
	let result = client
		.upload_asset(
			"v1.0.0",
			"missing.tar.gz",
			std::path::Path::new("/nonexistent/path/missing.tar.gz"),
		)
		.await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("missing.tar.gz"),
		"Error should mention the missing file, got: {msg}"
	);
}

// ── builder constructors (token kinds) ─────────────────────────────────────────

#[tokio::test]
async fn build_with_pat_uses_private_token_header() {
	let server = MockServer::start();
	let host = server
		.base_url()
		.trim_start_matches("http://")
		.trim_start_matches("https://")
		.to_string();
	let project = GitLabProject::new("gitlab.example.com", "acme", "app").unwrap();

	// The gitlab crate verifies the connection at build_async() by GETting /api/v4/user.
	server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/user")
			.header("private-token", "pat-token");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(json!({"id": 1, "username": "test"}).to_string());
	});

	let _mock = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/acme%2Fapp/releases/v1.0.0")
			.header("private-token", "pat-token");
		then.status(404).body("{}");
	});

	let mut builder = gitlab::GitlabBuilder::new(&host, "pat-token");
	builder.insecure();
	let async_client = builder.build_async().await.unwrap();
	let client = ReqwestGitLabClient::new(async_client, project);
	let _ = GitLabTokenKind::PersonalAccessToken;
	let result = client.find_release_by_tag("v1.0.0").await.unwrap();
	assert!(result.is_none());
}
