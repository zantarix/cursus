//! GitLab remote URL detection and parsing.
//!
//! Unlike the GitHub parser, GitLab URLs are not anchored to a fixed
//! hostname (`gitlab.com`); self-managed instances on arbitrary hostnames
//! must be supported. The hostname is therefore extracted from the URL
//! itself. Subgroup paths (`group/subgroup/project`) are supported by
//! treating everything up to the final `/` as the group path.

use anyhow::bail;

use crate::git::Git;
use crate::model::config::GitLabConfig;

/// A parsed GitLab project identity: host, group path, and project name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabProject {
	/// Hostname of the GitLab instance (e.g. `gitlab.com`, `gitlab.example.com`).
	///
	/// The scheme is not included; callers compose the base URL by combining
	/// this with the configured or detected scheme.
	pub host: String,
	/// Group (or namespace) path the project belongs to.
	///
	/// May contain `/` for subgroup paths (e.g. `acme/subgroup`). Each segment
	/// has been individually validated.
	pub group: String,
	/// Project name — the final path segment.
	pub project: String,
}

impl GitLabProject {
	/// Creates a new [`GitLabProject`], validating that `host`, every group
	/// segment, and `project` contain only safe characters for URL interpolation.
	///
	/// GitLab project, group, and host segments allow alphanumerics, hyphens,
	/// underscores, and dots. Rejecting anything else prevents path-traversal
	/// attacks when values are interpolated into API URLs.
	///
	/// # Errors
	///
	/// Returns an error if `host`, any group segment, or `project` is empty or
	/// contains invalid characters.
	pub fn new(
		host: impl Into<String>,
		group: impl Into<String>,
		project: impl Into<String>,
	) -> anyhow::Result<Self> {
		let host = host.into();
		let group = group.into();
		let project = project.into();
		Self::validate_host(&host)?;
		Self::validate_group_path(&group)?;
		Self::validate_identifier(&project, "project")?;
		Ok(Self {
			host,
			group,
			project,
		})
	}

	fn validate_identifier(value: &str, field: &str) -> anyhow::Result<()> {
		if value.is_empty()
			|| value == "."
			|| value == ".."
			|| !value
				.chars()
				.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
		{
			bail!("Invalid GitLab {field}: {value:?}");
		}
		Ok(())
	}

	fn validate_host(value: &str) -> anyhow::Result<()> {
		// Hosts use the same character class as identifiers — alphanumerics,
		// hyphens, dots, and underscores — but never contain `/`. A single
		// optional `:<digits>` port suffix is permitted for self-managed
		// instances on non-standard ports (e.g. `gitlab.example.com:8443`).
		let (hostname, port) = match value.split_once(':') {
			Some((h, p)) => (h, Some(p)),
			None => (value, None),
		};
		// Reject hosts with more than one `:` — that would mean the port
		// segment itself contains `:`, which is never valid.
		if port.is_some_and(|p| p.contains(':')) {
			bail!("Invalid GitLab host: {value:?}");
		}
		Self::validate_identifier(hostname, "host")?;
		if let Some(p) = port
			&& (p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
		{
			bail!("Invalid GitLab host: {value:?}");
		}
		Ok(())
	}

	fn validate_group_path(value: &str) -> anyhow::Result<()> {
		if value.is_empty() {
			bail!("Invalid GitLab group: {value:?}");
		}
		for segment in value.split('/') {
			Self::validate_identifier(segment, "group")?;
		}
		Ok(())
	}

	/// Parses a git remote URL into a [`GitLabProject`].
	///
	/// Supported formats:
	/// - HTTPS: `https://<host>[:<port>]/<group...>/<project>[.git]`
	/// - SCP-syntax SSH: `git@<host>:<group...>/<project>[.git]`
	/// - SSH URL: `ssh://[user@]<host>[:<port>]/<group...>/<project>[.git]`
	///
	/// The hostname is extracted from the URL itself (no `gitlab.com`
	/// hard-coding) so self-managed instances are supported without
	/// configuration heroics.
	///
	/// Returns `None` for URLs whose path does not contain at least one `/`
	/// separator (i.e. no group, just `host/project`) or whose segments fail
	/// validation.
	fn parse_url(url: &str) -> Option<Self> {
		let url = url.trim();

		let (host, path) = if let Some(rest) = url.strip_prefix("https://") {
			split_host_and_path(rest)?
		} else if let Some(rest) = url.strip_prefix("http://") {
			split_host_and_path(rest)?
		} else if let Some(rest) = url.strip_prefix("ssh://") {
			// ssh:// scheme: optional 'user@', then host[:port]/path
			let rest = rest.split_once('@').map_or(rest, |(_, after)| after);
			split_host_and_path(rest)?
		} else {
			// SCP syntax: git@host:group/project
			let rest = url.strip_prefix("git@")?;
			let (host, path) = rest.split_once(':')?;
			(host.to_string(), path.to_string())
		};

		let path = path.strip_suffix(".git").unwrap_or(&path);
		let (group, project) = path.rsplit_once('/')?;
		if group.is_empty() {
			return None;
		}
		GitLabProject::new(host, group, project).ok()
	}

	/// Detects the GitLab project for a git working directory.
	///
	/// Queries the `origin` remote URL via [`Git::remote_origin_url`] and
	/// parses the output. Returns `Ok(None)` if there is no `origin` remote or
	/// the URL cannot be parsed as a GitLab project.
	///
	/// # Errors
	///
	/// Returns an error if the git command cannot be executed.
	pub(crate) async fn detect_in(git: &dyn Git) -> anyhow::Result<Option<Self>> {
		match git.remote_origin_url().await? {
			Some(url) => Ok(Self::parse_url(&url)),
			None => Ok(None),
		}
	}

	/// Resolves the GitLab project from config or by detecting from the git remote.
	///
	/// Config takes priority: when both `group` and `project` are set, the
	/// host comes from `gitlab_config.host` (empty → `gitlab.com`). When the
	/// config fields are absent, falls back to detection from the git remote
	/// URL, which provides the host as well.
	///
	/// # Errors
	///
	/// Returns an error if `group` and `project` are partially set (one set,
	/// one not), or if neither config nor remote detection can determine the
	/// project.
	pub async fn resolve(gitlab_config: &GitLabConfig, git: &dyn Git) -> anyhow::Result<Self> {
		match (gitlab_config.group(), gitlab_config.project()) {
			(Some(group), Some(project)) => {
				let host = host_from_config(&gitlab_config.host);
				return GitLabProject::new(host, group, project);
			}
			(Some(_), None) | (None, Some(_)) => bail!(
				"[gitlab].group and [gitlab].project must be set together; \
				 set both or omit both for auto-detection."
			),
			(None, None) => {}
		}

		match Self::detect_in(git).await? {
			Some(project) => Ok(project),
			None => bail!(
				"Could not determine GitLab project. Set [gitlab] group and project in config, \
				 or ensure the git remote 'origin' points to a GitLab project."
			),
		}
	}
}

/// Splits a `host[:port]/path` string and returns `(host, path)` where `host`
/// retains the port suffix if one was present.
///
/// Returns `None` if there is no `/` separating host from path, or if the
/// optional port is malformed.
fn split_host_and_path(s: &str) -> Option<(String, String)> {
	let (host_with_port, path) = s.split_once('/')?;
	if let Some((_, port)) = host_with_port.split_once(':')
		&& (port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()))
	{
		// Malformed port — caller treats this as an unparseable URL.
		return None;
	}
	Some((host_with_port.to_string(), path.to_string()))
}

/// Resolves the host from a `[gitlab].host` config value.
///
/// Empty resolves to `gitlab.com`. Otherwise strips a leading scheme
/// (`https://`, `http://`) so the returned value is the bare hostname only.
fn host_from_config(host: &str) -> String {
	let host = host.trim();
	if host.is_empty() {
		return "gitlab.com".to_string();
	}
	host.strip_prefix("https://")
		.or_else(|| host.strip_prefix("http://"))
		.unwrap_or(host)
		.trim_end_matches('/')
		.to_string()
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;
	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::git::GitWorkdir;

	fn workdir() -> crate::path::AbsolutePath {
		crate::path::AbsolutePath::new("/tmp").unwrap()
	}

	// --- GitLabProject::parse_url ---

	#[tokio::test]
	async fn parse_https_gitlab_com() {
		let result = GitLabProject::parse_url("https://gitlab.com/acme/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_https_self_managed() {
		let result = GitLabProject::parse_url("https://gitlab.example.com/acme/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_https_subgroup() {
		let result = GitLabProject::parse_url("https://gitlab.com/acme/sub/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.com", "acme/sub", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_https_deep_subgroup() {
		let result = GitLabProject::parse_url("https://gitlab.com/a/b/c/d/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.com", "a/b/c/d", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_https_without_git_suffix() {
		let result = GitLabProject::parse_url("https://gitlab.com/acme/app");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_https_with_port_preserves_port() {
		let result = GitLabProject::parse_url("https://gitlab.example.com:8443/acme/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.example.com:8443", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_https_no_group_returns_none() {
		// `host/project` alone — no group separator.
		assert!(GitLabProject::parse_url("https://gitlab.com/app.git").is_none());
	}

	#[tokio::test]
	async fn parse_scp_gitlab_com() {
		let result = GitLabProject::parse_url("git@gitlab.com:acme/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_scp_self_managed() {
		let result = GitLabProject::parse_url("git@gitlab.example.com:acme/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_scp_subgroup() {
		let result = GitLabProject::parse_url("git@gitlab.com:acme/sub/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.com", "acme/sub", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_ssh_url_with_user() {
		let result = GitLabProject::parse_url("ssh://git@gitlab.example.com/acme/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_ssh_url_without_user() {
		let result = GitLabProject::parse_url("ssh://gitlab.example.com/acme/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_ssh_url_with_port_preserves_port() {
		let result = GitLabProject::parse_url("ssh://git@gitlab.example.com:2222/acme/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.example.com:2222", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_ssh_url_subgroup() {
		let result = GitLabProject::parse_url("ssh://git@gitlab.example.com/acme/sub/app.git");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.example.com", "acme/sub", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_empty_returns_none() {
		assert!(GitLabProject::parse_url("").is_none());
	}

	#[tokio::test]
	async fn parse_unsupported_scheme_returns_none() {
		assert!(GitLabProject::parse_url("ftp://gitlab.example.com/acme/app.git").is_none());
	}

	#[tokio::test]
	async fn parse_trims_whitespace() {
		let result = GitLabProject::parse_url("  git@gitlab.com:acme/app.git\n");
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn parse_https_malformed_port_returns_none() {
		assert!(GitLabProject::parse_url("https://gitlab.com:/acme/app.git").is_none());
	}

	// --- GitLabProject::new validation ---

	#[tokio::test]
	async fn new_accepts_valid_segments() {
		assert!(GitLabProject::new("gitlab.com", "acme", "my-app").is_ok());
		assert!(GitLabProject::new("gitlab.example.com", "acme/sub", "app.svc").is_ok());
		assert!(GitLabProject::new("gitlab.com", "Org123", "repo").is_ok());
	}

	#[tokio::test]
	async fn new_accepts_explicit_port_in_host() {
		// Self-managed GitLab instances on non-standard ports.
		assert!(GitLabProject::new("gitlab.example.com:8443", "acme", "app").is_ok());
		assert!(GitLabProject::new("gitlab.example.com:22", "acme", "app").is_ok());
	}

	#[tokio::test]
	async fn new_rejects_invalid_host() {
		assert!(GitLabProject::new("", "acme", "app").is_err());
		assert!(GitLabProject::new("git lab.com", "acme", "app").is_err());
		assert!(GitLabProject::new("../evil", "acme", "app").is_err());
	}

	#[tokio::test]
	async fn new_rejects_malformed_port_in_host() {
		// Empty port, non-digit port, and double-colon forms must all fail.
		assert!(GitLabProject::new("gitlab.example.com:", "acme", "app").is_err());
		assert!(GitLabProject::new("gitlab.example.com:abc", "acme", "app").is_err());
		assert!(GitLabProject::new("gitlab.example.com:80:443", "acme", "app").is_err());
	}

	#[tokio::test]
	async fn new_rejects_invalid_group() {
		assert!(GitLabProject::new("gitlab.com", "", "app").is_err());
		assert!(GitLabProject::new("gitlab.com", "ac me", "app").is_err());
		assert!(GitLabProject::new("gitlab.com", "ac/../evil", "app").is_err());
	}

	#[tokio::test]
	async fn new_rejects_invalid_project() {
		assert!(GitLabProject::new("gitlab.com", "acme", "").is_err());
		assert!(GitLabProject::new("gitlab.com", "acme", "a/b").is_err());
		assert!(GitLabProject::new("gitlab.com", "acme", "../evil").is_err());
	}

	// --- GitLabProject::detect_in ---

	#[tokio::test]
	async fn detect_returns_project_for_https_remote() {
		let runner = Arc::new(
			RecordingCommandRunner::new(0)
				.with_stdout(b"https://gitlab.com/acme/app.git\n".to_vec()),
		);
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let result = GitLabProject::detect_in(&git).await.unwrap();
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn detect_returns_project_for_ssh_remote() {
		let runner = Arc::new(
			RecordingCommandRunner::new(0)
				.with_stdout(b"git@gitlab.example.com:acme/sub/app.git\n".to_vec()),
		);
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let result = GitLabProject::detect_in(&git).await.unwrap();
		assert_eq!(
			result,
			Some(GitLabProject::new("gitlab.example.com", "acme/sub", "app").unwrap())
		);
	}

	#[tokio::test]
	async fn detect_returns_none_when_git_fails() {
		let runner = Arc::new(RecordingCommandRunner::new(1));
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let result = GitLabProject::detect_in(&git).await.unwrap();
		assert_eq!(result, None);
	}

	// --- GitLabProject::resolve ---

	fn make_config(group: Option<&str>, project: Option<&str>, host: &str) -> GitLabConfig {
		let mut config = GitLabConfig::enabled_config().with_host(host.to_string());
		if let Some(g) = group {
			config = config.with_group(g.to_string());
		}
		if let Some(p) = project {
			config = config.with_project(p.to_string());
		}
		config
	}

	#[tokio::test]
	async fn resolve_uses_config_when_set() {
		let config = make_config(Some("acme"), Some("app"), "");
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let project = GitLabProject::resolve(&config, &git).await.unwrap();
		assert_eq!(project.host, "gitlab.com");
		assert_eq!(project.group, "acme");
		assert_eq!(project.project, "app");
		assert!(runner.invocations().is_empty());
	}

	#[tokio::test]
	async fn resolve_uses_config_host_when_set() {
		let config = make_config(Some("acme"), Some("app"), "https://gitlab.example.com/");
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let project = GitLabProject::resolve(&config, &git).await.unwrap();
		assert_eq!(project.host, "gitlab.example.com");
	}

	#[tokio::test]
	async fn resolve_subgroup_from_config() {
		let config = make_config(Some("acme/sub"), Some("app"), "");
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let project = GitLabProject::resolve(&config, &git).await.unwrap();
		assert_eq!(project.group, "acme/sub");
	}

	#[tokio::test]
	async fn resolve_falls_back_to_git_remote() {
		let config = make_config(None, None, "");
		let runner = Arc::new(
			RecordingCommandRunner::new(0)
				.with_stdout(b"https://gitlab.com/myorg/myapp.git\n".to_vec()),
		);
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let project = GitLabProject::resolve(&config, &git).await.unwrap();
		assert_eq!(project.host, "gitlab.com");
		assert_eq!(project.group, "myorg");
		assert_eq!(project.project, "myapp");
	}

	#[tokio::test]
	async fn resolve_errors_when_neither_config_nor_remote() {
		let config = make_config(None, None, "");
		let runner = Arc::new(RecordingCommandRunner::new(1));
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let result = GitLabProject::resolve(&config, &git).await;
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("Could not determine GitLab project"),
			"Expected project detection error, got: {msg}"
		);
	}

	#[tokio::test]
	async fn resolve_errors_when_only_group_set() {
		let config = make_config(Some("acme"), None, "");
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let result = GitLabProject::resolve(&config, &git).await;
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("must be set together"),
			"Expected partial config error, got: {msg}"
		);
	}

	#[tokio::test]
	async fn resolve_errors_when_only_project_set() {
		let config = make_config(None, Some("app"), "");
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let result = GitLabProject::resolve(&config, &git).await;
		assert!(result.is_err());
	}

	// --- host_from_config ---

	#[test]
	fn host_from_empty_returns_gitlab_com() {
		assert_eq!(host_from_config(""), "gitlab.com");
	}

	#[test]
	fn host_from_whitespace_returns_gitlab_com() {
		assert_eq!(host_from_config("   "), "gitlab.com");
	}

	#[test]
	fn host_strips_https_scheme() {
		assert_eq!(
			host_from_config("https://gitlab.example.com"),
			"gitlab.example.com"
		);
	}

	#[test]
	fn host_strips_trailing_slash() {
		assert_eq!(
			host_from_config("https://gitlab.example.com/"),
			"gitlab.example.com"
		);
	}

	#[test]
	fn host_passes_through_bare_host() {
		assert_eq!(host_from_config("gitlab.example.com"), "gitlab.example.com");
	}
}
