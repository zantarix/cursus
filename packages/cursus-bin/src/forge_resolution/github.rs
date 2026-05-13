//! GitHub forge-client construction at the binary boundary.

use std::sync::Arc;

use crate::env_helpers::env_first;

/// Builds an [`octocrab::Octocrab`] client configured with standard timeouts.
pub(crate) fn build_octocrab(token: &str) -> Result<octocrab::Octocrab, octocrab::Error> {
	octocrab::Octocrab::builder()
		.personal_token(token.to_string())
		.set_connect_timeout(Some(std::time::Duration::from_secs(10)))
		.set_read_timeout(Some(std::time::Duration::from_secs(30)))
		.set_write_timeout(Some(std::time::Duration::from_secs(30)))
		.build()
}

/// Attempts to construct the GitHub code forge client from environment and config.
///
/// Returns `Ok(client)` when a token is present and the repo identity can be
/// resolved, or `Err(reason)` describing why the client is unavailable.
///
/// Accepts a pre-built `octocrab` instance (shared with the Git decorator when
/// signed commits are enabled) to avoid constructing a second HTTP client.
pub(crate) async fn resolve_github_forge_client(
	env: &cursus::Env,
	config: &Option<cursus::model::config::Config>,
	octocrab: Option<Arc<octocrab::Octocrab>>,
) -> Result<Arc<dyn cursus::forge::CodeForgeClient>, String> {
	let octocrab = match octocrab {
		Some(o) => (*o).clone(),
		None => {
			let token = env_first(&["GH_TOKEN", "GITHUB_TOKEN"])
				.ok_or_else(|| "No GitHub token found (GH_TOKEN / GITHUB_TOKEN)".to_string())?;
			build_octocrab(&token).map_err(|e| format!("Failed to build GitHub client: {e}"))?
		}
	};
	let cfg = config
		.as_ref()
		.ok_or_else(|| "No configuration file found".to_string())?;
	let repo = cursus::forge::github::remote::GitHubRepo::resolve(&cfg.github, env.git())
		.await
		.map_err(|e| format!("{e:#}"))?;
	Ok(Arc::new(cursus::forge::github::OctocrabGitHubClient::new(
		octocrab, repo,
	)) as Arc<dyn cursus::forge::CodeForgeClient>)
}
