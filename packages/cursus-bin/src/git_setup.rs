//! Production [`cursus::git::Git`] construction for the cursus binary.
//!
//! Selects between [`cursus::git::GitWorkdir`] (subprocess `git`) and the
//! [`cursus::git::GitHubSignedCommit`] decorator that routes commits through
//! the GitHub Git Data API for Verified commits (ADR-050).

use std::sync::Arc;

use anyhow::Context as _;

/// Constructs the `Git` implementation for the current environment.
///
/// Returns a [`cursus::git::GitHubSignedCommit`] decorator when
/// [`resolve_signed_commits_mode`] indicates the API path is warranted, or a plain
/// [`cursus::git::GitWorkdir`] otherwise.
#[coverage(off)]
#[mutants::skip]
pub(crate) async fn build_git(
	inner: Arc<cursus::git::GitWorkdir>,
	filesystem: Arc<dyn cursus::filesystem::Filesystem>,
	runner: Arc<dyn cursus::command::CommandRunner>,
	config: &Option<cursus::model::config::Config>,
	dry_run: bool,
	octocrab: Option<Arc<octocrab::Octocrab>>,
) -> anyhow::Result<Arc<dyn cursus::git::Git>> {
	let mode = config
		.as_ref()
		.map(|c| c.git.signed_commits)
		.unwrap_or_default();
	let on_gha = std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true");
	let use_api = resolve_signed_commits_mode(mode, octocrab.is_some(), on_gha);

	if use_api {
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
		let g: Arc<dyn cursus::git::Git> = Arc::new(cursus::git::GitHubSignedCommit::new(
			inner, filesystem, octocrab, runner, repo.owner, repo.repo, dry_run,
		));
		Ok(g)
	} else {
		let g: Arc<dyn cursus::git::Git> = inner;
		Ok(g)
	}
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
