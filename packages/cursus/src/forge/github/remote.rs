//! GitHub remote URL detection and parsing.

use anyhow::bail;

use crate::git::Git;
use crate::model::config::GitHubConfig;

/// A parsed GitHub repository owner and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepo {
	/// GitHub organisation or user name.
	pub owner: String,
	/// GitHub repository name.
	pub repo: String,
}

impl GitHubRepo {
	/// Creates a new [`GitHubRepo`], validating that `owner` and `repo` contain only
	/// safe characters for URL interpolation.
	///
	/// GitHub allows alphanumeric characters, hyphens, underscores, and dots. Rejecting
	/// anything else prevents path-traversal attacks when values are interpolated into URLs.
	///
	/// # Errors
	///
	/// Returns an error if either `owner` or `repo` is empty or contains invalid characters.
	pub fn new(owner: impl Into<String>, repo: impl Into<String>) -> anyhow::Result<Self> {
		let owner = owner.into();
		let repo = repo.into();
		Self::validate_identifier(&owner, "owner")?;
		Self::validate_identifier(&repo, "repo")?;
		Ok(Self { owner, repo })
	}

	fn validate_identifier(value: &str, field: &str) -> anyhow::Result<()> {
		if value.is_empty()
			|| !value
				.chars()
				.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
		{
			anyhow::bail!("Invalid GitHub {field}: {value:?}");
		}
		Ok(())
	}

	/// Parses a git remote URL into a [`GitHubRepo`] if it points to GitHub.
	///
	/// Supported formats:
	/// - HTTPS: `https://github.com[:<port>]/owner/repo[.git]`
	/// - SCP-syntax SSH: `git@github.com:owner/repo[.git]`
	/// - SSH URL: `ssh://[user@]github.com[:<port>]/owner/repo[.git]`
	///
	/// Returns `None` for non-GitHub URLs, URLs with extra path segments, or
	/// empty/malformed input.
	pub(crate) fn parse_url(url: &str) -> Option<Self> {
		let url = url.trim();

		let path = if let Some(rest) = url.strip_prefix("https://github.com") {
			// HTTPS: optional port then '/owner/repo'
			let rest = strip_optional_port(rest)?;
			rest.strip_prefix('/')?
		} else if let Some(rest) = url.strip_prefix("ssh://") {
			// ssh:// scheme: optional 'user@', then 'github.com', optional port, then '/owner/repo'
			let rest = rest.split_once('@').map_or(rest, |(_, after)| after);
			let rest = rest.strip_prefix("github.com")?;
			let rest = strip_optional_port(rest)?;
			rest.strip_prefix('/')?
		} else {
			// SCP syntax: git@github.com:owner/repo
			url.strip_prefix("git@github.com:")?
		};

		let path = path.strip_suffix(".git").unwrap_or(path);
		let (owner, repo) = path.split_once('/')?;
		GitHubRepo::new(owner, repo).ok()
	}

	/// Detects the GitHub repository for a git working directory.
	///
	/// Queries the `origin` remote URL via [`GitWorkdir::remote_origin_url`] and
	/// parses the output. Returns `Ok(None)` if there is no `origin` remote or
	/// the URL does not point to GitHub.
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

	/// Resolves the GitHub repository from config or by detecting from the git remote.
	///
	/// Checks `owner` and `repo` config fields first, then falls back to
	/// detecting from the git remote URL via [`GitWorkdir::remote_origin_url`].
	///
	/// # Errors
	///
	/// Returns an error if both config fields are partially set (one set, one not),
	/// or if neither config nor remote detection can determine the repository.
	pub async fn resolve(github_config: &GitHubConfig, git: &dyn Git) -> anyhow::Result<Self> {
		match (github_config.owner(), github_config.repo()) {
			(Some(owner), Some(repo)) => {
				return GitHubRepo::new(owner, repo);
			}
			(Some(_), None) | (None, Some(_)) => bail!(
				"[github].owner and [github].repo must be set together; \
				 set both or omit both for auto-detection."
			),
			(None, None) => {}
		}

		match Self::detect_in(git).await? {
			Some(gh_repo) => Ok(gh_repo),
			None => bail!(
				"Could not determine GitHub repository. Set [github] owner and repo in config, \
				 or ensure the git remote 'origin' points to a GitHub repository."
			),
		}
	}
}

/// Strips an optional `:<port>` segment from the start of `s`.
///
/// Returns `Some(remainder)` where `remainder` is `s` with the port prefix
/// removed, or `None` if a colon is present but is not followed by at least
/// one ASCII digit.
fn strip_optional_port(s: &str) -> Option<&str> {
	let Some(after_colon) = s.strip_prefix(':') else {
		return Some(s);
	};
	// At least one digit must follow the colon.
	let digit_end = after_colon
		.find(|c: char| !c.is_ascii_digit())
		.unwrap_or(after_colon.len());
	if digit_end == 0 {
		return None;
	}
	Some(&after_colon[digit_end..])
}
