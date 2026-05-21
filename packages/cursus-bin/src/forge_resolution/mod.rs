//! Forge-client resolution at the binary boundary.
//!
//! Reads environment variables and config to pick between the GitHub and
//! GitLab forge implementations, returning a [`cursus::forge::CodeForgeClient`]
//! plus any auth-kind signals callers need.

mod github;
mod gitlab;

#[cfg(test)]
mod tests;

use std::sync::Arc;

pub(crate) use github::build_octocrab;

/// Outcome of resolving the active forge client.
pub(crate) struct ForgeClientOutcome {
	pub(crate) client: Arc<dyn cursus::forge::CodeForgeClient>,
	pub(crate) gitlab_uses_job_token_only: bool,
}

/// Dispatches to the appropriate forge client constructor based on which
/// `[github].enabled` / `[gitlab].enabled` flag is set.
///
/// The "more than one enabled" case is rejected at `Config::load`, so it
/// should be unreachable here; the defensive arm returns an error rather
/// than panicking.
pub(crate) async fn resolve_forge_client(
	env: &cursus::Env,
	config: &Option<cursus::model::config::Config>,
	octocrab: Option<Arc<octocrab::Octocrab>>,
) -> Result<ForgeClientOutcome, String> {
	let cfg = config.as_ref();
	let github_enabled = cfg.is_some_and(|c| c.github.enabled);
	let gitlab_enabled = cfg.is_some_and(|c| c.gitlab.enabled);
	match (github_enabled, gitlab_enabled) {
		(true, false) => Ok(ForgeClientOutcome {
			client: github::resolve_github_forge_client(env, config, octocrab).await?,
			gitlab_uses_job_token_only: false,
		}),
		(false, true) => {
			let gitlab_outcome = gitlab::resolve_gitlab_forge_client(env, config).await?;
			Ok(ForgeClientOutcome {
				client: gitlab_outcome.client,
				gitlab_uses_job_token_only: gitlab_outcome.uses_job_token_only,
			})
		}
		(false, false) => Err(
			"No forge enabled; set [github].enabled or [gitlab].enabled in .cursus/config.toml"
				.to_string(),
		),
		_ => Err("More than one forge is enabled in .cursus/config.toml.".to_string()),
	}
}

/// Wrapper around [`resolve_forge_client`] that unpacks the outcome into the
/// shape `Env`'s builder expects. Kept separate so the `try_main` function
/// can stay compact.
///
/// Returns `(Result<client, reason>, gitlab_uses_job_token_only)`. The bool
/// is `false` whenever forge resolution fails or the active forge is GitHub.
pub(crate) async fn resolve_forge_client_for_env(
	env: &cursus::Env,
	config: &Option<cursus::model::config::Config>,
	octocrab: Option<Arc<octocrab::Octocrab>>,
) -> (
	Result<Arc<dyn cursus::forge::CodeForgeClient>, String>,
	bool,
) {
	match resolve_forge_client(env, config, octocrab).await {
		Ok(outcome) => (Ok(outcome.client), outcome.gitlab_uses_job_token_only),
		Err(reason) => (Err(reason), false),
	}
}
