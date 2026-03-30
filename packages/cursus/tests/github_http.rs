//! Integration tests for [`OctocrabGitHubClient`] against a mock HTTP server.
//!
//! These tests verify that the octocrab-based client correctly maps API
//! responses to `CodeForgeClient` trait return values and propagates errors.

use std::path::Path;

use httpmock::prelude::*;

use cursus::github::OctocrabGitHubClient;
use cursus::github::client::CodeForgeClient;
use cursus::github::remote::GitHubRepo;

/// Builds an `OctocrabGitHubClient` pointing at the given mock server.
fn mock_client(server: &MockServer) -> OctocrabGitHubClient {
	let client = octocrab::Octocrab::builder()
		.personal_token("test-token".to_string())
		.base_uri(server.base_url())
		.unwrap()
		.build()
		.unwrap();
	OctocrabGitHubClient::new(client)
}

fn repo() -> GitHubRepo {
	GitHubRepo::new("owner", "repo").unwrap()
}

// Minimal JSON that satisfies octocrab's Release model.
const RELEASE_JSON: &str = r#"{
	"url": "https://api.github.com/repos/owner/repo/releases/42",
	"html_url": "https://github.com/owner/repo/releases/tag/v1.0.0",
	"assets_url": "https://api.github.com/repos/owner/repo/releases/42/assets",
	"upload_url": "https://uploads.github.com/repos/owner/repo/releases/42/assets{?name,label}",
	"id": 42,
	"node_id": "abc",
	"tag_name": "v1.0.0",
	"target_commitish": "main",
	"draft": true,
	"prerelease": false,
	"created_at": "2026-01-01T00:00:00Z",
	"assets": []
}"#;

// Minimal JSON for a PR response.
const PR_JSON: &str = r#"{
	"url": "https://api.github.com/repos/owner/repo/pulls/7",
	"id": 1,
	"number": 7,
	"html_url": "https://github.com/owner/repo/pull/7",
	"head": {"ref": "feature", "sha": "abc123"},
	"base": {"ref": "main", "sha": "def456"}
}"#;

// ── create_release ────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_release_returns_id() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST).path("/repos/owner/repo/releases");
		then.status(201)
			.header("Content-Type", "application/json")
			.body(RELEASE_JSON);
	});

	let client = mock_client(&server);
	let id = client
		.create_release(&repo(), "v1.0.0", "Release 1.0.0", "Body")
		.await
		.unwrap();
	assert_eq!(id, "42");
}

#[tokio::test]
async fn create_release_propagates_api_error() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST).path("/repos/owner/repo/releases");
		then.status(422)
			.header("Content-Type", "application/json")
			.body(r#"{"message": "Validation Failed"}"#);
	});

	let client = mock_client(&server);
	let result = client
		.create_release(&repo(), "v1.0.0", "Release", "Body")
		.await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to create GitHub Release"),
		"Error should contain context, got: {msg}"
	);
}

// ── upload_asset ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn upload_asset_succeeds_with_valid_file() {
	let server = MockServer::start();
	let base = server.base_url();

	// octocrab first GETs the release to obtain upload_url, then POSTs the asset.
	let release_json = format!(
		r#"{{
		"url": "{base}/repos/owner/repo/releases/42",
		"html_url": "https://github.com/owner/repo/releases/tag/v1.0.0",
		"assets_url": "{base}/repos/owner/repo/releases/42/assets",
		"upload_url": "{base}/repos/owner/repo/releases/42/assets{{?name,label}}",
		"id": 42, "node_id": "abc", "tag_name": "v1.0.0", "target_commitish": "main",
		"draft": true, "prerelease": false, "created_at": "2026-01-01T00:00:00Z", "assets": []
	}}"#
	);
	let _mock_get = server.mock(|when, then| {
		when.method(GET).path("/repos/owner/repo/releases/42");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(&release_json);
	});
	let _mock_upload = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/owner/repo/releases/42/assets");
		then.status(201)
			.header("Content-Type", "application/json")
			.body(r#"{"id": 1, "node_id": "abc", "name": "file.tar.gz", "url": "https://example.com", "browser_download_url": "https://example.com/file.tar.gz", "state": "uploaded", "size": 4, "download_count": 0, "content_type": "application/octet-stream", "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"}"#);
	});

	let tmp = tempfile::NamedTempFile::new().unwrap();
	std::fs::write(tmp.path(), b"data").unwrap();

	let client = mock_client(&server);
	let result = client
		.upload_asset(&repo(), "42", "file.tar.gz", tmp.path())
		.await;
	assert!(result.is_ok(), "upload_asset failed: {result:?}");
}

#[tokio::test]
async fn upload_asset_rejects_non_numeric_release_id() {
	let server = MockServer::start();
	let client = mock_client(&server);
	let result = client
		.upload_asset(&repo(), "abc", "file.tar.gz", Path::new("/tmp/file"))
		.await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Invalid GitHub release_id"),
		"Error should mention invalid ID, got: {msg}"
	);
}

#[tokio::test]
async fn upload_asset_errors_on_missing_file() {
	let server = MockServer::start();
	let client = mock_client(&server);
	let result = client
		.upload_asset(
			&repo(),
			"42",
			"missing.tar.gz",
			Path::new("/nonexistent/path/missing.tar.gz"),
		)
		.await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("missing.tar.gz"),
		"Error should mention file, got: {msg}"
	);
}

// ── create_pull_request ────────────────────────────────────────────────���──────

#[tokio::test]
async fn create_pull_request_returns_url() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST).path("/repos/owner/repo/pulls");
		then.status(201)
			.header("Content-Type", "application/json")
			.body(PR_JSON);
	});

	let client = mock_client(&server);
	let url = client
		.create_pull_request(&repo(), "Title", "Body", "feature", "main")
		.await
		.unwrap();
	assert!(url.contains("pull/7"), "Expected PR URL, got: {url}");
}

#[tokio::test]
async fn create_pull_request_propagates_api_error() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST).path("/repos/owner/repo/pulls");
		then.status(422)
			.header("Content-Type", "application/json")
			.body(r#"{"message": "Validation Failed"}"#);
	});

	let client = mock_client(&server);
	let result = client
		.create_pull_request(&repo(), "Title", "Body", "feature", "main")
		.await;
	assert!(result.is_err());
}

// ── find_open_pull_request ────────────────────────────────────────────────────

#[tokio::test]
async fn find_open_pull_request_returns_pr_when_found() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(GET).path("/repos/owner/repo/pulls");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(format!("[{PR_JSON}]"));
	});

	let client = mock_client(&server);
	let pr = client
		.find_open_pull_request(&repo(), "feature")
		.await
		.unwrap();
	assert!(pr.is_some());
	assert_eq!(pr.unwrap().number, 7);
}

#[tokio::test]
async fn find_open_pull_request_returns_none_when_empty() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(GET).path("/repos/owner/repo/pulls");
		then.status(200)
			.header("Content-Type", "application/json")
			.body("[]");
	});

	let client = mock_client(&server);
	let pr = client
		.find_open_pull_request(&repo(), "feature")
		.await
		.unwrap();
	assert!(pr.is_none());
}

// ── update_pull_request ────────────────────────────────────────────────��──────

#[tokio::test]
async fn update_pull_request_returns_url() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(PATCH).path("/repos/owner/repo/pulls/7");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(PR_JSON);
	});

	let client = mock_client(&server);
	let url = client
		.update_pull_request(&repo(), 7, "Updated", "New body")
		.await
		.unwrap();
	assert!(url.contains("pull/7"), "Expected PR URL, got: {url}");
}

#[tokio::test]
async fn update_pull_request_propagates_api_error() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(PATCH).path("/repos/owner/repo/pulls/7");
		then.status(404)
			.header("Content-Type", "application/json")
			.body(r#"{"message": "Not Found"}"#);
	});

	let client = mock_client(&server);
	let result = client
		.update_pull_request(&repo(), 7, "Title", "Body")
		.await;
	assert!(result.is_err());
}

// ── publish_release ───────────────────────────────────────────────────────────

#[tokio::test]
async fn publish_release_succeeds() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(PATCH).path("/repos/owner/repo/releases/42");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(RELEASE_JSON);
	});

	let client = mock_client(&server);
	let result = client.publish_release(&repo(), "42").await;
	assert!(result.is_ok(), "publish_release failed: {result:?}");
}

#[tokio::test]
async fn publish_release_rejects_non_numeric_release_id() {
	let server = MockServer::start();
	let client = mock_client(&server);
	let result = client.publish_release(&repo(), "abc").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Invalid GitHub release_id"),
		"Error should mention invalid ID, got: {msg}"
	);
}

#[tokio::test]
async fn publish_release_propagates_api_error() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(PATCH).path("/repos/owner/repo/releases/42");
		then.status(404)
			.header("Content-Type", "application/json")
			.body(r#"{"message": "Not Found"}"#);
	});

	let client = mock_client(&server);
	let result = client.publish_release(&repo(), "42").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to publish GitHub Release"),
		"Error should contain context, got: {msg}"
	);
}
