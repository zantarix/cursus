//! httpmock-based tests for [`ReqwestGitLabClient`].
//!
//! Each test stands up a [`httpmock::MockServer`], constructs an
//! [`AsyncGitlab`] pointed at it via [`GitlabBuilder::insecure`] (so the
//! Kitware crate dials `http://` instead of `https://`), wraps the result in
//! a [`ReqwestGitLabClient`], and exercises one trait method end-to-end.
//!
//! The Kitware crate prefixes every REST call with `/api/v4/`, and project
//! paths are URL-escaped (`group/project` → `group%2Fproject`).

use gitlab::GitlabBuilder;
use httpmock::MockServer;
use serde_json::json;

use crate::forge::CodeForgeClient;
use crate::forge::gitlab::client::GitLabTokenKind;
use crate::forge::gitlab::{ReqwestGitLabClient, remote::GitLabProject};

/// Constructs a [`ReqwestGitLabClient`] pointed at `server` and registers the
/// `/api/v4/user` probe that the Kitware crate sends during connection setup
/// when authenticating with a personal access token.
async fn make_client(server: &MockServer) -> ReqwestGitLabClient {
	server.mock(|when, then| {
		when.method(httpmock::Method::GET).path("/api/v4/user");
		then.status(200)
			.json_body(json!({"id": 1, "username": "test"}));
	});

	let host = server.address().to_string();
	let async_client = GitlabBuilder::new(host.as_str(), "test-token")
		.insecure()
		.build_async()
		.await
		.unwrap();
	ReqwestGitLabClient::new(
		async_client,
		GitLabProject::new(host, "group", "project").unwrap(),
	)
}

#[tokio::test]
async fn forge_name_returns_gitlab() {
	let server = MockServer::start_async().await;
	let client = make_client(&server).await;
	assert_eq!(client.forge_name(), "GitLab");
}

#[tokio::test]
async fn create_release_returns_tag_as_release_id() {
	let server = MockServer::start_async().await;
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/api/v4/projects/group%2Fproject/releases");
		then.status(201).json_body(json!({}));
	});

	let client = make_client(&server).await;
	let release_id = client
		.create_release("v1.0.0", "Release 1.0.0", "body")
		.await
		.unwrap();

	mock.assert();
	// GitLab uses the tag as the opaque release id.
	assert_eq!(release_id, "v1.0.0");
}

#[tokio::test]
async fn create_release_propagates_error() {
	let server = MockServer::start_async().await;
	server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/api/v4/projects/group%2Fproject/releases");
		then.status(500).json_body(json!({"message": "boom"}));
	});

	let client = make_client(&server).await;
	let result = client.create_release("v1.0.0", "name", "body").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to create GitLab release for tag 'v1.0.0'"),
		"unexpected error: {msg}"
	);
}

#[tokio::test]
async fn upload_asset_uploads_package_and_attaches_link() {
	let server = MockServer::start_async().await;
	let upload_mock = server.mock(|when, then| {
		when.method(httpmock::Method::PUT).path(
			"/api/v4/projects/group%2Fproject/packages/generic/release-assets/v1.0.0/asset.bin",
		);
		then.status(201).json_body(json!({}));
	});
	let link_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/api/v4/projects/group%2Fproject/releases/v1.0.0/assets/links");
		then.status(201).json_body(json!({}));
	});

	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("asset.bin");
	std::fs::write(&path, b"hello").unwrap();

	let client = make_client(&server).await;
	client
		.upload_asset("v1.0.0", "asset.bin", &path)
		.await
		.unwrap();

	upload_mock.assert();
	link_mock.assert();
}

#[tokio::test]
async fn upload_asset_propagates_read_error() {
	let server = MockServer::start_async().await;
	let client = make_client(&server).await;
	let result = client
		.upload_asset("v1.0.0", "f.bin", std::path::Path::new("/nonexistent/path"))
		.await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(msg.contains("Failed to read asset file"), "got: {msg}");
}

#[tokio::test]
async fn create_pull_request_returns_web_url() {
	let server = MockServer::start_async().await;
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/api/v4/projects/group%2Fproject/merge_requests");
		then.status(201).json_body(json!({
			"iid": 7,
			"web_url": "https://gitlab.example.com/group/project/-/merge_requests/7",
		}));
	});

	let client = make_client(&server).await;
	let url = client
		.create_pull_request("Release v1.0.0", "body", "release", "main")
		.await
		.unwrap();

	mock.assert();
	assert_eq!(
		url,
		"https://gitlab.example.com/group/project/-/merge_requests/7"
	);
}

#[tokio::test]
async fn find_open_pull_request_returns_some_when_found() {
	let server = MockServer::start_async().await;
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/api/v4/projects/group%2Fproject/merge_requests")
			.query_param("source_branch", "release")
			.query_param("state", "opened");
		then.status(200).json_body(json!([
			{
				"iid": 12,
				"web_url": "https://gitlab.example.com/group/project/-/merge_requests/12",
			}
		]));
	});

	let client = make_client(&server).await;
	let pr = client.find_open_pull_request("release").await.unwrap();
	mock.assert();
	let pr = pr.expect("expected MR");
	assert_eq!(pr.number, 12);
	assert_eq!(
		pr.html_url,
		"https://gitlab.example.com/group/project/-/merge_requests/12"
	);
}

#[tokio::test]
async fn find_open_pull_request_returns_none_for_empty_list() {
	let server = MockServer::start_async().await;
	server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/api/v4/projects/group%2Fproject/merge_requests");
		then.status(200).json_body(json!([]));
	});

	let client = make_client(&server).await;
	let pr = client.find_open_pull_request("release").await.unwrap();
	assert!(pr.is_none());
}

#[tokio::test]
async fn update_pull_request_returns_web_url() {
	let server = MockServer::start_async().await;
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::PUT)
			.path("/api/v4/projects/group%2Fproject/merge_requests/3");
		then.status(200).json_body(json!({
			"iid": 3,
			"web_url": "https://gitlab.example.com/group/project/-/merge_requests/3",
		}));
	});

	let client = make_client(&server).await;
	let url = client
		.update_pull_request(3, "Updated title", "new body")
		.await
		.unwrap();

	mock.assert();
	assert_eq!(
		url,
		"https://gitlab.example.com/group/project/-/merge_requests/3"
	);
}

#[tokio::test]
async fn find_release_by_tag_returns_existing_release() {
	let server = MockServer::start_async().await;
	let mock = server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/api/v4/projects/group%2Fproject/releases/v1.0.0");
		then.status(200).json_body(json!({}));
	});

	let client = make_client(&server).await;
	let existing = client.find_release_by_tag("v1.0.0").await.unwrap();
	mock.assert();
	let existing = existing.expect("expected existing release");
	assert_eq!(existing.id, "v1.0.0");
	// GitLab releases never have a draft state.
	assert!(!existing.is_draft);
}

#[tokio::test]
async fn find_release_by_tag_returns_none_for_404() {
	let server = MockServer::start_async().await;
	server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/api/v4/projects/group%2Fproject/releases/missing");
		then.status(404)
			.json_body(json!({"message": "404 Not Found"}));
	});

	let client = make_client(&server).await;
	let existing = client.find_release_by_tag("missing").await.unwrap();
	assert!(existing.is_none());
}

#[tokio::test]
async fn find_release_by_tag_propagates_non_404_errors() {
	let server = MockServer::start_async().await;
	server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/api/v4/projects/group%2Fproject/releases/v1.0.0");
		then.status(500).json_body(json!({"message": "boom"}));
	});

	let client = make_client(&server).await;
	let result = client.find_release_by_tag("v1.0.0").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to look up GitLab release for tag 'v1.0.0'"),
		"got: {msg}"
	);
}

#[tokio::test]
async fn publish_release_is_noop() {
	let server = MockServer::start_async().await;
	let client = make_client(&server).await;
	// Register the catch-all *after* make_client so the connection probe it
	// performs does not get counted against our expectation.
	let catch_all = server.mock(|when, then| {
		when.any_request().path_excludes("/api/v4/user");
		then.status(200).body("{}");
	});

	client.publish_release("v1.0.0").await.unwrap();
	// GitLab releases are visible the moment they're created, so publish_release
	// must not issue any HTTP requests.
	catch_all.assert_calls(0);
}

#[tokio::test]
async fn build_with_invalid_host_returns_context_error() {
	// ReqwestGitLabClient::build always uses https and cannot be redirected at a
	// mock server, so we exercise its error-context wrapper by feeding it a host
	// the underlying GitlabBuilder will reject.
	let project = GitLabProject::new("gitlab.example.com", "g", "p").unwrap();
	let result = ReqwestGitLabClient::build(
		"not a host",
		"token",
		GitLabTokenKind::PersonalAccessToken,
		project,
	)
	.await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to initialise GitLab client for host 'not a host'"),
		"unexpected error: {msg}"
	);
}

#[tokio::test]
async fn build_with_job_token_kind_returns_context_error() {
	// Same fail-fast as above but threads through the JobToken match arm so
	// both branches of the token_kind dispatch are covered.
	let project = GitLabProject::new("gitlab.example.com", "g", "p").unwrap();
	let result =
		ReqwestGitLabClient::build("not a host", "token", GitLabTokenKind::JobToken, project).await;
	assert!(result.is_err());
}

#[tokio::test]
async fn upload_asset_propagates_upload_error() {
	let server = MockServer::start_async().await;
	let client = make_client(&server).await;
	server.mock(|when, then| {
		when.method(httpmock::Method::PUT).path(
			"/api/v4/projects/group%2Fproject/packages/generic/release-assets/v1.0.0/asset.bin",
		);
		then.status(500).json_body(json!({"message": "boom"}));
	});

	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("asset.bin");
	std::fs::write(&path, b"x").unwrap();

	let result = client.upload_asset("v1.0.0", "asset.bin", &path).await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to upload 'asset.bin'"),
		"unexpected error: {msg}"
	);
}

#[tokio::test]
async fn upload_asset_propagates_link_error() {
	let server = MockServer::start_async().await;
	let client = make_client(&server).await;
	server.mock(|when, then| {
		when.method(httpmock::Method::PUT).path(
			"/api/v4/projects/group%2Fproject/packages/generic/release-assets/v1.0.0/asset.bin",
		);
		then.status(201).json_body(json!({}));
	});
	server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/api/v4/projects/group%2Fproject/releases/v1.0.0/assets/links");
		then.status(500).json_body(json!({"message": "boom"}));
	});

	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("asset.bin");
	std::fs::write(&path, b"x").unwrap();

	let result = client.upload_asset("v1.0.0", "asset.bin", &path).await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to attach 'asset.bin' to GitLab release 'v1.0.0'"),
		"unexpected error: {msg}"
	);
}

#[tokio::test]
async fn create_pull_request_propagates_error() {
	let server = MockServer::start_async().await;
	let client = make_client(&server).await;
	server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/api/v4/projects/group%2Fproject/merge_requests");
		then.status(500).json_body(json!({"message": "boom"}));
	});

	let result = client
		.create_pull_request("title", "body", "head", "main")
		.await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to create merge request 'title'"),
		"unexpected error: {msg}"
	);
}

#[tokio::test]
async fn update_pull_request_propagates_error() {
	let server = MockServer::start_async().await;
	let client = make_client(&server).await;
	server.mock(|when, then| {
		when.method(httpmock::Method::PUT)
			.path("/api/v4/projects/group%2Fproject/merge_requests/9");
		then.status(500).json_body(json!({"message": "boom"}));
	});

	let result = client.update_pull_request(9, "t", "b").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to update merge request !9"),
		"unexpected error: {msg}"
	);
}

#[tokio::test]
async fn find_open_pull_request_propagates_error() {
	let server = MockServer::start_async().await;
	let client = make_client(&server).await;
	server.mock(|when, then| {
		when.method(httpmock::Method::GET)
			.path("/api/v4/projects/group%2Fproject/merge_requests");
		then.status(500).json_body(json!({"message": "boom"}));
	});

	let result = client.find_open_pull_request("release").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("Failed to list merge requests for branch 'release'"),
		"unexpected error: {msg}"
	);
}

#[tokio::test]
async fn debug_impl_shows_project_identity() {
	let server = MockServer::start_async().await;
	let client = make_client(&server).await;
	let formatted = format!("{client:?}");
	assert!(formatted.contains("group"));
	assert!(formatted.contains("project"));
}
