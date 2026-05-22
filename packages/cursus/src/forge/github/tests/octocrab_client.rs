//! httpmock-based tests for [`OctocrabGitHubClient`].
//!
//! Each test stands up a [`httpmock::MockServer`], points an [`Octocrab`]
//! instance at it via `base_uri`, wraps the result in an
//! [`OctocrabGitHubClient`], and exercises one trait method end-to-end so the
//! deserialisation + URL composition + happy/error branches are all covered
//! without touching the real GitHub API.

use httpmock::MockServer;
use octocrab::Octocrab;
use serde_json::{Value, json};

use crate::forge::CodeForgeClient;
use crate::forge::github::OctocrabGitHubClient;
use crate::forge::github::remote::GitHubRepo;

fn make_client(server: &MockServer) -> OctocrabGitHubClient {
	let octocrab = Octocrab::builder()
		.base_uri(server.base_url())
		.unwrap()
		.build()
		.unwrap();
	OctocrabGitHubClient::new(octocrab, GitHubRepo::new("owner", "repo").unwrap())
}

/// Returns a JSON body that satisfies octocrab's [`Release`] deserialiser.
///
/// `upload_base` controls the `upload_url` field; pass the [`MockServer`]'s
/// base URL when the test needs `upload_asset` to dispatch back to the mock,
/// otherwise pass a dummy URL.
fn release_body(id: u64, tag: &str, draft: bool, upload_base: &str) -> Value {
	json!({
		"url": format!("{upload_base}/api/release"),
		"html_url": format!("{upload_base}/release"),
		"assets_url": format!("{upload_base}/api/assets"),
		"upload_url": format!("{upload_base}/repos/owner/repo/releases/{id}/assets{{?name,label}}"),
		"tarball_url": null,
		"zipball_url": null,
		"id": id,
		"node_id": "n",
		"tag_name": tag,
		"target_commitish": "main",
		"name": null,
		"body": null,
		"draft": draft,
		"prerelease": false,
		"created_at": null,
		"published_at": null,
		"author": null,
		"assets": [],
	})
}

/// Returns a JSON body that satisfies octocrab's [`Asset`] deserialiser.
fn asset_body(name: &str) -> Value {
	json!({
		"url": "https://example.com/api/asset",
		"browser_download_url": "https://example.com/dl",
		"id": 1,
		"node_id": "n",
		"name": name,
		"label": null,
		"state": "uploaded",
		"content_type": "application/octet-stream",
		"size": 5,
		"digest": null,
		"download_count": 0,
		"created_at": "2025-01-01T00:00:00Z",
		"updated_at": "2025-01-01T00:00:00Z",
		"uploader": null,
	})
}

/// Returns a JSON body that satisfies octocrab's [`PullRequest`] deserialiser.
fn pull_request_body(number: u64, html_url: &str) -> Value {
	json!({
		"url": "https://example.com/api/pull",
		"id": 1,
		"html_url": html_url,
		"number": number,
		"head": {
			"ref": "release",
			"sha": "deadbeef",
		},
		"base": {
			"ref": "main",
			"sha": "cafebabe",
		},
	})
}

#[tokio::test]
async fn forge_name_returns_github() {
	let server = MockServer::start();
	let client = make_client(&server);
	assert_eq!(client.forge_name(), "GitHub");
}

#[tokio::test]
async fn create_release_returns_release_id() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/releases");
		then.status(201)
			.json_body(release_body(42, "v1.0.0", true, "https://example.com"));
	});

	let client = make_client(&server);
	let release_id = client
		.create_release("v1.0.0", "Release 1.0.0", "body text")
		.await
		.unwrap();

	mock.assert();
	assert_eq!(release_id, "42");
}

#[tokio::test]
async fn create_release_propagates_error() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/releases");
		then.status(500)
			.json_body(json!({"message": "boom", "documentation_url": ""}));
	});

	let client = make_client(&server);
	let result = client.create_release("v1.0.0", "name", "body").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to create GitHub Release for tag 'v1.0.0'"),
		"unexpected error: {msg}"
	);
}

#[tokio::test]
async fn upload_asset_invokes_upload_endpoint() {
	let server = MockServer::start();
	let base = server.base_url();
	// octocrab's upload_asset first GETs the release to read upload_url, then
	// POSTs to that URL. Mock both endpoints so the request chain completes.
	let get_mock = server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/repos/owner/repo/releases/99");
		then.status(200)
			.json_body(release_body(99, "v1.0.0", true, &base));
	});
	let upload_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/releases/99/assets")
			.query_param("name", "cursus-x86_64.tar.gz");
		then.status(201)
			.json_body(asset_body("cursus-x86_64.tar.gz"));
	});

	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("asset.bin");
	std::fs::write(&path, b"hello").unwrap();

	let client = make_client(&server);
	client
		.upload_asset("99", "cursus-x86_64.tar.gz", &path)
		.await
		.unwrap();

	get_mock.assert();
	upload_mock.assert();
}

#[tokio::test]
async fn upload_asset_rejects_non_numeric_release_id() {
	let server = MockServer::start();
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("asset.bin");
	std::fs::write(&path, b"x").unwrap();

	let client = make_client(&server);
	let result = client.upload_asset("not-a-number", "f.bin", &path).await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(msg.contains("Invalid GitHub release_id"), "got: {msg}");
}

#[tokio::test]
async fn upload_asset_propagates_read_error() {
	let server = MockServer::start();
	let client = make_client(&server);
	let result = client
		.upload_asset("1", "f.bin", std::path::Path::new("/nonexistent/path"))
		.await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(msg.contains("Failed to read asset file"), "got: {msg}");
}

#[tokio::test]
async fn create_pull_request_returns_html_url() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/pulls");
		then.status(201)
			.json_body(pull_request_body(7, "https://github.com/owner/repo/pull/7"));
	});

	let client = make_client(&server);
	let url = client
		.create_pull_request("Release v1.0.0", "body", "release", "main")
		.await
		.unwrap();

	mock.assert();
	assert_eq!(url, "https://github.com/owner/repo/pull/7");
}

#[tokio::test]
async fn find_open_pull_request_returns_some_when_found() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/repos/owner/repo/pulls")
			.query_param("state", "open")
			.query_param("head", "owner:release");
		then.status(200).json_body(json!([pull_request_body(
			12,
			"https://github.com/owner/repo/pull/12"
		)]));
	});

	let client = make_client(&server);
	let pr = client.find_open_pull_request("release").await.unwrap();
	mock.assert();
	let pr = pr.expect("expected PR");
	assert_eq!(pr.number, 12);
	assert_eq!(pr.html_url, "https://github.com/owner/repo/pull/12");
}

#[tokio::test]
async fn find_open_pull_request_returns_none_for_empty_list() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/repos/owner/repo/pulls");
		then.status(200).json_body(json!([]));
	});

	let client = make_client(&server);
	let pr = client.find_open_pull_request("release").await.unwrap();
	assert!(pr.is_none());
}

#[tokio::test]
async fn update_pull_request_returns_html_url() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::PATCH)
			.path("/repos/owner/repo/pulls/3");
		then.status(200)
			.json_body(pull_request_body(3, "https://github.com/owner/repo/pull/3"));
	});

	let client = make_client(&server);
	let url = client
		.update_pull_request(3, "Updated title", "new body")
		.await
		.unwrap();

	mock.assert();
	assert_eq!(url, "https://github.com/owner/repo/pull/3");
}

#[tokio::test]
async fn find_release_by_tag_returns_existing_release() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/repos/owner/repo/releases/tags/v1.0.0");
		then.status(200)
			.json_body(release_body(555, "v1.0.0", true, "https://example.com"));
	});

	let client = make_client(&server);
	let existing = client.find_release_by_tag("v1.0.0").await.unwrap();
	mock.assert();
	let existing = existing.expect("expected existing release");
	assert_eq!(existing.id, "555");
	assert!(existing.is_draft);
}

#[tokio::test]
async fn find_release_by_tag_returns_none_for_404() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/repos/owner/repo/releases/tags/missing");
		then.status(404)
			.json_body(json!({"message": "Not Found", "documentation_url": ""}));
	});

	let client = make_client(&server);
	let existing = client.find_release_by_tag("missing").await.unwrap();
	assert!(existing.is_none());
}

#[tokio::test]
async fn find_release_by_tag_propagates_non_404_errors() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/repos/owner/repo/releases/tags/v1.0.0");
		then.status(500)
			.json_body(json!({"message": "boom", "documentation_url": ""}));
	});

	let client = make_client(&server);
	let result = client.find_release_by_tag("v1.0.0").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to look up GitHub Release for tag 'v1.0.0'"),
		"got: {msg}"
	);
}

#[tokio::test]
async fn find_release_by_tag_encodes_slash_in_tag() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/repos/owner/repo/releases/tags/scope%2Fpkg@1.0.0");
		then.status(200).json_body(release_body(
			1,
			"scope/pkg@1.0.0",
			false,
			"https://example.com",
		));
	});

	let client = make_client(&server);
	let existing = client.find_release_by_tag("scope/pkg@1.0.0").await.unwrap();
	mock.assert();
	assert!(existing.is_some());
}

#[tokio::test]
async fn publish_release_updates_draft_false() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::PATCH)
			.path("/repos/owner/repo/releases/77");
		then.status(200)
			.json_body(release_body(77, "v1.0.0", false, "https://example.com"));
	});

	let client = make_client(&server);
	client.publish_release("77").await.unwrap();
	mock.assert();
}

#[tokio::test]
async fn publish_release_rejects_non_numeric_id() {
	let server = MockServer::start();
	let client = make_client(&server);
	let result = client.publish_release("abc").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(msg.contains("Invalid GitHub release_id"), "got: {msg}");
}

#[tokio::test]
async fn debug_impl_shows_repo_identity() {
	let server = MockServer::start();
	let client = make_client(&server);
	let formatted = format!("{client:?}");
	assert!(formatted.contains("owner"));
	assert!(formatted.contains("repo"));
}
