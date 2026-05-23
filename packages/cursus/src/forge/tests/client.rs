use std::path::{Path, PathBuf};

use crate::forge::client::test_support::*;
use crate::forge::{CodeForgeClient, ExistingRelease, PullRequest};

#[tokio::test]
async fn recording_client_records_create_release() {
	let client = RecordingCodeForgeClient::new().with_release_id("r-42");
	let id = client
		.create_release("v1.0.0", "Release 1.0.0", "body text")
		.await
		.unwrap();
	assert_eq!(id, "r-42");
	let invocations = client.invocations();
	assert_eq!(invocations.len(), 1);
	assert!(matches!(
		&invocations[0],
		CodeForgeInvocation::CreateRelease { tag_name, .. } if tag_name == "v1.0.0"
	));
}

#[tokio::test]
async fn recording_client_records_upload_asset() {
	let client = RecordingCodeForgeClient::new();
	let path = PathBuf::from("/tmp/app.tar.gz");
	client
		.upload_asset("r-1", "app.tar.gz", &path)
		.await
		.unwrap();
	let invocations = client.invocations();
	assert_eq!(invocations.len(), 1);
	assert!(matches!(
		&invocations[0],
		CodeForgeInvocation::UploadAsset { file_name, .. } if file_name == "app.tar.gz"
	));
}

#[tokio::test]
async fn recording_client_create_failure_returns_error() {
	let client = RecordingCodeForgeClient::new().with_create_failure();
	let result = client.create_release("v1.0.0", "Release", "body").await;
	assert!(result.is_err());
	// Invocation is still recorded even on failure
	assert_eq!(client.invocations().len(), 1);
}

#[tokio::test]
async fn recording_client_upload_failure_returns_error() {
	let client = RecordingCodeForgeClient::new().with_upload_failure();
	let result = client
		.upload_asset("r-1", "file.tar.gz", Path::new("/tmp/file.tar.gz"))
		.await;
	assert!(result.is_err());
	assert_eq!(client.invocations().len(), 1);
}

#[tokio::test]
async fn recording_client_records_create_pull_request() {
	let client = RecordingCodeForgeClient::new();
	let url = client
		.create_pull_request(
			"Release updates",
			"Release:\n\n- my-pkg@1.0.0",
			"cursus-release/main",
			"main",
		)
		.await
		.unwrap();
	assert!(url.contains("pull/1"), "URL should contain pull/1: {url}");
	let invocations = client.invocations();
	assert_eq!(invocations.len(), 1);
	assert!(matches!(
		&invocations[0],
		CodeForgeInvocation::CreatePullRequest { title, head, base, .. }
			if title == "Release updates" && head == "cursus-release/main" && base == "main"
	));
}

#[tokio::test]
async fn recording_client_create_pr_failure_returns_error() {
	let client = RecordingCodeForgeClient::new().with_create_pr_failure();
	let result = client
		.create_pull_request("Release", "body", "release-branch", "main")
		.await;
	assert!(result.is_err());
	// Invocation is still recorded even on failure
	assert_eq!(client.invocations().len(), 1);
}

#[tokio::test]
async fn recording_client_find_open_pr_returns_none_when_not_configured() {
	let client = RecordingCodeForgeClient::new();
	let result = client
		.find_open_pull_request("cursus-release/main")
		.await
		.unwrap();
	assert!(result.is_none());
	let invocations = client.invocations();
	assert_eq!(invocations.len(), 1);
	assert!(matches!(
		&invocations[0],
		CodeForgeInvocation::FindOpenPullRequest { head, .. }
			if head == "cursus-release/main"
	));
}

#[tokio::test]
async fn recording_client_find_open_pr_returns_configured_pr() {
	let pr = PullRequest {
		number: 42,
		html_url: "https://github.com/acme/app/pull/42".to_string(),
	};
	let client = RecordingCodeForgeClient::new().with_existing_pr(pr);
	let result = client
		.find_open_pull_request("cursus-release/main")
		.await
		.unwrap();
	assert!(result.is_some());
	let found = result.unwrap();
	assert_eq!(found.number, 42);
	assert!(found.html_url.contains("pull/42"));
}

#[tokio::test]
async fn recording_client_find_pr_failure_returns_error() {
	let client = RecordingCodeForgeClient::new().with_find_pr_failure();
	let result = client.find_open_pull_request("release-branch").await;
	assert!(result.is_err());
	assert_eq!(client.invocations().len(), 1);
}

#[tokio::test]
async fn recording_client_update_pull_request_records_invocation() {
	let client = RecordingCodeForgeClient::new();
	let url = client
		.update_pull_request(42, "Updated Title", "Updated body")
		.await
		.unwrap();
	assert!(url.contains("pull/42"), "URL should contain pull/42: {url}");
	let invocations = client.invocations();
	assert_eq!(invocations.len(), 1);
	assert!(matches!(
		&invocations[0],
		CodeForgeInvocation::UpdatePullRequest { pull_number, title, .. }
			if *pull_number == 42 && title == "Updated Title"
	));
}

#[tokio::test]
async fn recording_client_update_pr_failure_returns_error() {
	let client = RecordingCodeForgeClient::new().with_update_pr_failure();
	let result = client.update_pull_request(1, "Title", "body").await;
	assert!(result.is_err());
	assert_eq!(client.invocations().len(), 1);
}

#[tokio::test]
async fn recording_client_records_publish_release() {
	let client = RecordingCodeForgeClient::new();
	client.publish_release("release-1").await.unwrap();
	let invocations = client.invocations();
	assert_eq!(invocations.len(), 1);
	assert!(matches!(
		&invocations[0],
		CodeForgeInvocation::PublishRelease { release_id, .. } if release_id == "release-1"
	));
}

#[tokio::test]
async fn recording_client_publish_release_failure_returns_error() {
	let client = RecordingCodeForgeClient::new().with_publish_release_failure();
	let result = client.publish_release("release-1").await;
	assert!(result.is_err());
	// Invocation is still recorded even on failure
	assert_eq!(client.invocations().len(), 1);
}

#[tokio::test]
async fn recording_client_find_release_by_tag_returns_none_when_not_configured() {
	let client = RecordingCodeForgeClient::new();
	let result = client.find_release_by_tag("v1.0.0").await.unwrap();
	assert!(result.is_none());
	assert_eq!(client.invocations().len(), 1);
	assert!(matches!(
		&client.invocations()[0],
		CodeForgeInvocation::FindReleaseByTag { tag } if tag == "v1.0.0"
	));
}

#[tokio::test]
async fn recording_client_find_release_by_tag_returns_configured_release() {
	let client = RecordingCodeForgeClient::new().with_existing_release(
		"v1.0.0",
		ExistingRelease {
			id: "r-42".to_string(),
			is_draft: false,
		},
	);
	let result = client.find_release_by_tag("v1.0.0").await.unwrap();
	assert!(result.is_some());
	let release = result.unwrap();
	assert_eq!(release.id, "r-42");
	assert!(!release.is_draft);
}

#[tokio::test]
async fn recording_client_find_release_failure_returns_error() {
	let client = RecordingCodeForgeClient::new().with_find_release_failure();
	let result = client.find_release_by_tag("v1.0.0").await;
	assert!(result.is_err());
	assert_eq!(client.invocations().len(), 1);
}
