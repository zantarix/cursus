use anyhow::Context;
use log::info;

use crate::git::Git;
use crate::github::GitHubRepo;
use crate::github::client::GitHubClient;
use crate::model::config::Config;

use super::ReleaseInfo;

/// Creates or updates a pull request for the given head branch.
///
/// If an open pull request already exists for `head`, it is updated with the
/// new `title` and `body`. Otherwise a new pull request is created from `head`
/// into `base`. Returns the URL of the created or updated pull request.
///
/// # Errors
///
/// Returns an error if the find, create, or update API call fails.
pub(super) async fn upsert_pull_request(
	client: &dyn GitHubClient,
	gh_repo: &GitHubRepo,
	title: &str,
	body: &str,
	head: &str,
	base: &str,
) -> anyhow::Result<String> {
	match client.find_open_pull_request(gh_repo, head).await? {
		Some(pr) => {
			let url = client
				.update_pull_request(gh_repo, pr.number, title, body)
				.await?;
			info!("Updated pull request: {url}");
			Ok(url)
		}
		None => {
			let url = client
				.create_pull_request(gh_repo, title, body, head, base)
				.await?;
			info!("Created pull request: {url}");
			Ok(url)
		}
	}
}

/// Builds the pull request body with an introduction and per-package changelog sections.
pub(super) fn build_pr_body(releases: &[ReleaseInfo], base_branch: &str) -> String {
	let mut body = format!(
		"This PR was opened by Cursus. When ready to release, you should merge this PR \
		 which will trigger a release. If you're not ready to do a release then simply leave \
		 this PR and it will be updated as you merge more changesets into `{base_branch}`.\n\
		 \n\
		 # Releases\n"
	);
	for r in releases {
		let _ = std::fmt::Write::write_fmt(
			&mut body,
			format_args!(
				"\n## {}@{}\n\n{}",
				r.package_name, r.new_version, r.changelog_entry
			),
		);
	}
	body
}

/// Creates or updates the GitHub pull request for the release branch.
///
/// No-ops in dry-run mode or when no GitHub client is available. The dry-run
/// short-circuit is intentional per ADR-017: the PR upsert is the side-effecting
/// operation being guarded, so the check lives here rather than at the call site.
pub(super) async fn upsert_release_pull_request(
	git: &dyn Git,
	config: &Config,
	env: &crate::Env,
	release_infos: &[ReleaseInfo],
	branch: &str,
	original_branch: Option<&str>,
	dry_run: bool,
) -> anyhow::Result<()> {
	if dry_run {
		info!("Would attempt to create or update a PR in GitHub.");
		return Ok(());
	}
	let Some(client) = env.github_client() else {
		return Ok(());
	};
	let base = original_branch.context("HEAD is detached; cannot determine PR base branch")?;
	let gh_repo = GitHubRepo::resolve(&config.github, git)
		.await
		.context("Could not resolve GitHub repository for PR creation")?;
	let title = config.github.pull_request_title();
	let pr_body = build_pr_body(release_infos, base);
	upsert_pull_request(client, &gh_repo, title, &pr_body, branch, base)
		.await
		.context("Failed to create or update pull request")?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

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
				changelog_entry:
					"### Features\n\n- Added widget\n\n### Bug Fixes\n\n- Fixed crash\n".to_string(),
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
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let client = RecordingGitHubClient::new(); // no existing PR
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"cursus-release/main",
			"main",
		)
		.await;
		assert!(result.is_ok(), "Expected Ok, got: {result:?}");
		let invocations = client.invocations();
		// Should have called find then create
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::FindOpenPullRequest { .. })),
			"Expected FindOpenPullRequest invocation"
		);
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. })),
			"Expected CreatePullRequest invocation"
		);
	}

	#[tokio::test]
	async fn upsert_pull_request_updates_when_existing() {
		use crate::github::client::PullRequest;
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let existing_pr = PullRequest {
			number: 7,
			html_url: "https://github.com/acme/app/pull/7".to_string(),
		};
		let client = RecordingGitHubClient::new().with_existing_pr(existing_pr);
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
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
				.any(|i| matches!(i, GitHubInvocation::FindOpenPullRequest { .. })),
			"Expected FindOpenPullRequest invocation"
		);
		assert!(
			invocations.iter().any(|i| matches!(
				i,
				GitHubInvocation::UpdatePullRequest { pull_number, .. } if *pull_number == 7
			)),
			"Expected UpdatePullRequest invocation for PR #7"
		);
		assert!(
			!invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. })),
			"Should NOT call CreatePullRequest when existing PR found"
		);
	}

	#[tokio::test]
	async fn upsert_pull_request_propagates_find_error() {
		use crate::github::client::test_support::RecordingGitHubClient;
		let client = RecordingGitHubClient::new().with_find_pr_failure();
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"release-branch",
			"main",
		)
		.await;
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("simulated find_open_pull_request failure"),
			"Expected find failure error, got: {msg}"
		);
	}

	#[tokio::test]
	async fn upsert_pull_request_propagates_update_error() {
		use crate::github::client::PullRequest;
		use crate::github::client::test_support::RecordingGitHubClient;
		let existing_pr = PullRequest {
			number: 1,
			html_url: "https://github.com/acme/app/pull/1".to_string(),
		};
		let client = RecordingGitHubClient::new()
			.with_existing_pr(existing_pr)
			.with_update_pr_failure();
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"release-branch",
			"main",
		)
		.await;
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("simulated update_pull_request failure"),
			"Expected update failure error, got: {msg}"
		);
	}

	#[tokio::test]
	async fn upsert_pull_request_propagates_create_error() {
		use crate::github::client::test_support::RecordingGitHubClient;
		let client = RecordingGitHubClient::new().with_create_pr_failure();
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"release-branch",
			"main",
		)
		.await;
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("simulated create_pull_request failure"),
			"Expected create failure error, got: {msg}"
		);
	}
}
