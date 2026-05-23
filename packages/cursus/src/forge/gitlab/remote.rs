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

/// A parsed GitLab project identity: scheme, host, group path, and project name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabProject {
	/// URL scheme the GitLab API and asset URLs should use
	/// (`"https"` or `"http"`). Defaults to `"https"` from [`GitLabProject::new`];
	/// `parse_url` preserves the scheme detected from the input URL, and the
	/// binary boundary may override it via [`GitLabProject::with_scheme`] when
	/// `CI_API_V4_URL` or `[gitlab].host` specifies a non-default scheme.
	pub scheme: String,
	/// Hostname of the GitLab instance (e.g. `gitlab.com`, `gitlab.example.com`).
	///
	/// The scheme is stored in [`Self::scheme`]; callers compose the base URL
	/// by combining the two.
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
			scheme: "https".to_string(),
			host,
			group,
			project,
		})
	}

	/// Builder-style override for the URL scheme (`"https"` or `"http"`).
	///
	/// Returns the project unchanged for any scheme other than `"https"` or
	/// `"http"` so callers cannot inject arbitrary scheme strings into the
	/// composed base URL.
	pub fn with_scheme(mut self, scheme: impl Into<String>) -> Self {
		let s = scheme.into();
		if s == "https" || s == "http" {
			self.scheme = s;
		}
		self
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
	pub(crate) fn parse_url(url: &str) -> Option<Self> {
		let url = url.trim();

		let (scheme, host, path) = if let Some(rest) = url.strip_prefix("https://") {
			let (h, p) = split_host_and_path(rest)?;
			("https", h, p)
		} else if let Some(rest) = url.strip_prefix("http://") {
			let (h, p) = split_host_and_path(rest)?;
			("http", h, p)
		} else if let Some(rest) = url.strip_prefix("ssh://") {
			// ssh:// scheme: optional 'user@', then host[:port]/path. The web
			// API the asset URLs target is virtually always HTTPS even when the
			// repo is cloned over SSH, so default to "https" here.
			let rest = rest.split_once('@').map_or(rest, |(_, after)| after);
			let (h, p) = split_host_and_path(rest)?;
			("https", h, p)
		} else {
			// SCP syntax: git@host:group/project — same HTTPS default as ssh://.
			let rest = url.strip_prefix("git@")?;
			let (host, path) = rest.split_once(':')?;
			("https", host.to_string(), path.to_string())
		};

		let path = path.strip_suffix(".git").unwrap_or(&path);
		let (group, project) = path.rsplit_once('/')?;
		if group.is_empty() {
			return None;
		}
		GitLabProject::new(host, group, project)
			.map(|p| p.with_scheme(scheme))
			.ok()
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
				let (scheme, host) = scheme_and_host_from_config(&gitlab_config.host);
				return GitLabProject::new(host, group, project).map(|p| p.with_scheme(scheme));
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

/// Splits a `[gitlab].host` config value into its `(scheme, host)` parts.
///
/// Returns `("https", "gitlab.com")` for an empty/whitespace value. An
/// explicit `http://` prefix is preserved so a self-managed instance served
/// over plain HTTP propagates the correct scheme into the constructed
/// [`GitLabProject`]. Any other `<word>://` prefix is stripped (with the
/// scheme defaulted to `"https"`) so downstream host validation rejects the
/// bare host cleanly rather than echoing the bogus scheme back in the error.
pub(crate) fn scheme_and_host_from_config(host: &str) -> (String, String) {
	let host = host.trim();
	if host.is_empty() {
		return ("https".to_string(), "gitlab.com".to_string());
	}
	let (scheme, rest) = if let Some(rest) = host.strip_prefix("https://") {
		("https", rest)
	} else if let Some(rest) = host.strip_prefix("http://") {
		("http", rest)
	} else if let Some(idx) = host.find("://") {
		("https", &host[idx + 3..])
	} else {
		("https", host)
	};
	(scheme.to_string(), rest.trim_end_matches('/').to_string())
}
