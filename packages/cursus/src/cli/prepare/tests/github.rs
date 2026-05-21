use crate::cli::prepare::ReleaseInfo;
use crate::cli::prepare::github::*;

#[tokio::test]
async fn build_pr_body_empty_releases() {
	let body = build_pr_body(&[], "main");
	assert!(body.contains("# Releases"));
	assert!(body.contains("`main`"));
	assert!(body.contains("Cursus"));
}

#[tokio::test]
async fn build_pr_body_formats_single_release() {
	let releases = vec![ReleaseInfo {
		package_name: "my-pkg".to_string(),
		new_version: "1.2.0".parse().unwrap(),
		changelog_entry: "### Features\n\n- Added something\n".to_string(),
	}];
	let body = build_pr_body(&releases, "main");
	assert!(body.contains("## my-pkg@1.2.0"));
	assert!(body.contains("### Features"));
	assert!(body.contains("- Added something"));
	assert!(body.contains("`main`"));
}

#[tokio::test]
async fn build_pr_body_formats_multiple_releases() {
	let releases = vec![
		ReleaseInfo {
			package_name: "pkg-a".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: "### Bug Fixes\n\n- Fixed a bug\n".to_string(),
		},
		ReleaseInfo {
			package_name: "pkg-b".to_string(),
			new_version: "2.1.0".parse().unwrap(),
			changelog_entry: String::new(),
		},
	];
	let body = build_pr_body(&releases, "develop");
	assert!(body.contains("## pkg-a@1.0.0"));
	assert!(body.contains("### Bug Fixes"));
	assert!(body.contains("- Fixed a bug"));
	assert!(body.contains("## pkg-b@2.1.0"));
	assert!(body.contains("`develop`"));
	// pkg-a section must appear before pkg-b
	let pos_a = body.find("## pkg-a").unwrap();
	let pos_b = body.find("## pkg-b").unwrap();
	assert!(pos_a < pos_b);
}

#[tokio::test]
async fn build_pr_body_includes_base_branch_in_intro() {
	let body = build_pr_body(&[], "my-feature-branch");
	assert!(body.contains("`my-feature-branch`"));
}

#[tokio::test]
async fn build_pr_body_snapshot() {
	let releases = vec![
		ReleaseInfo {
			package_name: "pkg-a".to_string(),
			new_version: "2.0.0".parse().unwrap(),
			changelog_entry: "### Breaking Changes\n\n- Removed old API\n".to_string(),
		},
		ReleaseInfo {
			package_name: "pkg-b".to_string(),
			new_version: "1.3.0".parse().unwrap(),
			changelog_entry: "### Features\n\n- Added widget\n\n### Bug Fixes\n\n- Fixed crash\n"
				.to_string(),
		},
		ReleaseInfo {
			package_name: "pkg-c".to_string(),
			new_version: "0.9.1".parse().unwrap(),
			changelog_entry: String::new(),
		},
	];
	insta::assert_snapshot!(build_pr_body(&releases, "main"));
}

// ── upsert_pull_request ───────────────────────────────────────────────────

#[tokio::test]
async fn upsert_pull_request_creates_when_no_existing() {
	use crate::forge::test_support::{CodeForgeInvocation, RecordingCodeForgeClient};
	let client = RecordingCodeForgeClient::new(); // no existing PR
	let result =
		upsert_pull_request(&client, "Release PR", "body", "cursus-release/main", "main").await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
	let invocations = client.invocations();
	// Should have called find then create
	assert!(
		invocations
			.iter()
			.any(|i| matches!(i, CodeForgeInvocation::FindOpenPullRequest { .. })),
		"Expected FindOpenPullRequest invocation"
	);
	assert!(
		invocations
			.iter()
			.any(|i| matches!(i, CodeForgeInvocation::CreatePullRequest { .. })),
		"Expected CreatePullRequest invocation"
	);
}

#[tokio::test]
async fn upsert_pull_request_updates_when_existing() {
	use crate::forge::PullRequest;
	use crate::forge::test_support::{CodeForgeInvocation, RecordingCodeForgeClient};
	let existing_pr = PullRequest {
		number: 7,
		html_url: "https://github.com/acme/app/pull/7".to_string(),
	};
	let client = RecordingCodeForgeClient::new().with_existing_pr(existing_pr);
	let result = upsert_pull_request(
		&client,
		"Release PR",
		"updated body",
		"cursus-release/main",
		"main",
	)
	.await;
	assert!(result.is_ok(), "Expected Ok, got: {result:?}");
	let invocations = client.invocations();
	assert!(
		invocations
			.iter()
			.any(|i| matches!(i, CodeForgeInvocation::FindOpenPullRequest { .. })),
		"Expected FindOpenPullRequest invocation"
	);
	assert!(
		invocations.iter().any(|i| matches!(
			i,
			CodeForgeInvocation::UpdatePullRequest { pull_number, .. } if *pull_number == 7
		)),
		"Expected UpdatePullRequest invocation for PR #7"
	);
	assert!(
		!invocations
			.iter()
			.any(|i| matches!(i, CodeForgeInvocation::CreatePullRequest { .. })),
		"Should NOT call CreatePullRequest when existing PR found"
	);
}

#[tokio::test]
async fn upsert_pull_request_propagates_find_error() {
	use crate::forge::test_support::RecordingCodeForgeClient;
	let client = RecordingCodeForgeClient::new().with_find_pr_failure();
	let result = upsert_pull_request(&client, "Release PR", "body", "release-branch", "main").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("simulated find_open_pull_request failure"),
		"Expected find failure error, got: {msg}"
	);
}

#[tokio::test]
async fn upsert_pull_request_propagates_update_error() {
	use crate::forge::PullRequest;
	use crate::forge::test_support::RecordingCodeForgeClient;
	let existing_pr = PullRequest {
		number: 1,
		html_url: "https://github.com/acme/app/pull/1".to_string(),
	};
	let client = RecordingCodeForgeClient::new()
		.with_existing_pr(existing_pr)
		.with_update_pr_failure();
	let result = upsert_pull_request(&client, "Release PR", "body", "release-branch", "main").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("simulated update_pull_request failure"),
		"Expected update failure error, got: {msg}"
	);
}

#[tokio::test]
async fn upsert_pull_request_propagates_create_error() {
	use crate::forge::test_support::RecordingCodeForgeClient;
	let client = RecordingCodeForgeClient::new().with_create_pr_failure();
	let result = upsert_pull_request(&client, "Release PR", "body", "release-branch", "main").await;
	assert!(result.is_err());
	let msg = format!("{:#}", result.unwrap_err());
	assert!(
		msg.contains("simulated create_pull_request failure"),
		"Expected create failure error, got: {msg}"
	);
}
