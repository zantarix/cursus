use anyhow::Context;
use log::info;

use crate::forge::CodeForgeClient;
use crate::model::config::Config;

use super::ReleaseInfo;

/// Creates or updates a pull request for the given head branch.
///
/// If an open pull request already exists for `head`, it is updated with the
/// new `title` and `body`. Otherwise a new pull request is created from `head`
/// into `base`. Returns the URL of the created or updated request. The
/// concrete client emits its own user-visible success log line using
/// forge-native vocabulary; this layer does not log success.
///
/// # Errors
///
/// Returns an error if the find, create, or update API call fails.
pub(crate) async fn upsert_pull_request(
	client: &dyn CodeForgeClient,
	title: &str,
	body: &str,
	head: &str,
	base: &str,
) -> anyhow::Result<String> {
	match client.find_open_pull_request(head).await? {
		Some(pr) => client.update_pull_request(pr.number, title, body).await,
		None => client.create_pull_request(title, body, head, base).await,
	}
}

/// Builds the pull request body with an introduction and per-package changelog sections.
pub(crate) fn build_pr_body(releases: &[ReleaseInfo], base_branch: &str) -> String {
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

/// Creates or updates the forge release request (pull request on GitHub,
/// merge request on GitLab) for the release branch.
///
/// No-ops in dry-run mode or when no forge client is available. The dry-run
/// short-circuit is intentional per ADR-017: the request upsert is the
/// side-effecting operation being guarded, so the check lives here rather
/// than at the call site.
pub(super) async fn upsert_release_pull_request(
	config: &Config,
	env: &crate::Env,
	release_infos: &[ReleaseInfo],
	branch: &str,
	original_branch: Option<&str>,
	dry_run: bool,
) -> anyhow::Result<()> {
	if dry_run {
		info!(
			"Would attempt to create or update a release request on {}.",
			env.code_forge_name()
		);
		return Ok(());
	}
	let Ok(client) = env.code_forge_client() else {
		return Ok(());
	};
	let base = original_branch.context("HEAD is detached; cannot determine release base branch")?;
	let title = config.release_request_title();
	let pr_body = build_pr_body(release_infos, base);
	upsert_pull_request(client, title, &pr_body, branch, base)
		.await
		.with_context(|| {
			format!(
				"Failed to create or update release request on {}",
				client.forge_name()
			)
		})?;
	Ok(())
}
