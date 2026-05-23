//! Production [`cursus::git::Git`] construction for the cursus binary.
//!
//! Selects between [`cursus::git::GitWorkdir`] (subprocess `git`) and one of
//! the signed-commit decorators that route commits through a forge API for
//! Verified commits: [`cursus::git::GitHubSignedCommit`] (ADR-050) or
//! [`cursus::git::GitLabSignedCommit`] (ADR-058).

use std::sync::Arc;

use anyhow::Context as _;

use crate::forge_resolution::GitLabHandles;

/// Constructs the `Git` implementation for the current environment.
///
/// Selects between the plain [`cursus::git::GitWorkdir`] and a signed-commit
/// decorator based on the active forge (`[github].enabled` vs
/// `[gitlab].enabled`, mutually exclusive per ADR-059) and the
/// `signed_commits` mode resolution.
#[coverage(off)]
#[mutants::skip]
pub(crate) async fn build_git(
	inner: Arc<cursus::git::GitWorkdir>,
	filesystem: Arc<dyn cursus::filesystem::Filesystem>,
	runner: Arc<dyn cursus::command::CommandRunner>,
	config: &Option<cursus::model::config::Config>,
	dry_run: bool,
	octocrab: Option<Arc<octocrab::Octocrab>>,
	gitlab_handles: Option<&GitLabHandles>,
) -> anyhow::Result<Arc<dyn cursus::git::Git>> {
	let mode = config
		.as_ref()
		.map(|c| c.git.signed_commits)
		.unwrap_or_default();
	let github_enabled = config.as_ref().is_some_and(|c| c.github.enabled);
	let gitlab_enabled = config.as_ref().is_some_and(|c| c.gitlab.enabled);

	if gitlab_enabled
		&& resolve_signed_commits_mode_gitlab(
			mode,
			gitlab_handles.is_some(),
			std::env::var("GITLAB_CI").as_deref() == Ok("true"),
		) {
		return build_gitlab_signed(inner, filesystem, runner, gitlab_handles, dry_run);
	}

	if github_enabled
		&& resolve_signed_commits_mode(
			mode,
			octocrab.is_some(),
			std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"),
		) {
		return build_github_signed(inner, filesystem, runner, config, octocrab, dry_run).await;
	}

	let g: Arc<dyn cursus::git::Git> = inner;
	Ok(g)
}

#[coverage(off)]
#[mutants::skip]
async fn build_github_signed(
	inner: Arc<cursus::git::GitWorkdir>,
	filesystem: Arc<dyn cursus::filesystem::Filesystem>,
	runner: Arc<dyn cursus::command::CommandRunner>,
	config: &Option<cursus::model::config::Config>,
	octocrab: Option<Arc<octocrab::Octocrab>>,
	dry_run: bool,
) -> anyhow::Result<Arc<dyn cursus::git::Git>> {
	let octocrab = octocrab.context(
		"GitHub token required for signed commits but none found (GH_TOKEN / GITHUB_TOKEN)",
	)?;
	let github_config = config
		.as_ref()
		.map(|c| c.github.clone())
		.unwrap_or_default();
	let repo = cursus::forge::github::remote::GitHubRepo::resolve(&github_config, &*inner)
		.await
		.context("cannot enable signed commits: failed to determine GitHub repository")?;
	log::info!(
		"Routing git commit and push operations through the GitHub API for verified commits."
	);
	Ok(Arc::new(cursus::git::GitHubSignedCommit::new(
		inner, filesystem, octocrab, runner, repo.owner, repo.repo, dry_run,
	)))
}

#[coverage(off)]
#[mutants::skip]
fn build_gitlab_signed(
	inner: Arc<cursus::git::GitWorkdir>,
	filesystem: Arc<dyn cursus::filesystem::Filesystem>,
	runner: Arc<dyn cursus::command::CommandRunner>,
	gitlab_handles: Option<&GitLabHandles>,
	dry_run: bool,
) -> anyhow::Result<Arc<dyn cursus::git::Git>> {
	let handles = gitlab_handles.context(
		"GitLab token required for signed commits but none found (GITLAB_TOKEN / CI_JOB_TOKEN)",
	)?;
	log::info!(
		"Routing git commit and push operations through the GitLab API for verified commits."
	);
	Ok(Arc::new(cursus::git::GitLabSignedCommit::new(
		inner,
		filesystem,
		Arc::clone(&handles.client),
		runner,
		handles.project.clone(),
		dry_run,
	)))
}

/// Returns `true` when the GitHub API commit path should be engaged.
///
/// Pure function over the resolved mode, token presence, and GHA detection;
/// accepts these as parameters so the policy can be tested without env mocking.
///
/// `Auto` engages when `GITHUB_ACTIONS=true` AND a token is available.
/// `Force` engages whenever a token is available.
/// `Off` never engages.
///
/// Dry-run is intentionally NOT checked here — the [`cursus::git::GitHubSignedCommit`]
/// decorator is constructed even in dry-run mode but short-circuits all API calls
/// via its own explicit dry-run guard (ADR-050, ADR-017 exception).
pub(crate) fn resolve_signed_commits_mode(
	mode: cursus::model::config::SignedCommitsMode,
	token_present: bool,
	on_gha: bool,
) -> bool {
	use cursus::model::config::SignedCommitsMode;
	match mode {
		SignedCommitsMode::Off => false,
		SignedCommitsMode::Force => token_present,
		SignedCommitsMode::Auto => on_gha && token_present,
	}
}

/// Returns `true` when the GitLab API commit path should be engaged.
///
/// Pure function over the resolved mode, token presence, and GitLab CI
/// detection; accepts these as parameters so the policy can be tested
/// without env mocking.
///
/// `Auto` engages when `GITLAB_CI=true` AND a token is available.
/// `Force` engages whenever a token is available.
/// `Off` never engages.
///
/// Dry-run handling matches the GitHub branch — checked inside the
/// decorator itself, not here (ADR-058 / ADR-017 exception).
pub(crate) fn resolve_signed_commits_mode_gitlab(
	mode: cursus::model::config::SignedCommitsMode,
	token_present: bool,
	on_gitlab_ci: bool,
) -> bool {
	use cursus::model::config::SignedCommitsMode;
	match mode {
		SignedCommitsMode::Off => false,
		SignedCommitsMode::Force => token_present,
		SignedCommitsMode::Auto => on_gitlab_ci && token_present,
	}
}
