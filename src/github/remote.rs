//! GitHub remote URL detection and parsing.

use anyhow::bail;

use crate::git::GitWorkdir;
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
	fn parse_url(url: &str) -> Option<Self> {
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
	pub(crate) fn detect_in(git: &GitWorkdir) -> anyhow::Result<Option<Self>> {
		match git.remote_origin_url()? {
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
	pub(crate) fn resolve(github_config: &GitHubConfig, git: &GitWorkdir) -> anyhow::Result<Self> {
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

		match Self::detect_in(git)? {
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

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;
	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::filesystem::LocalFilesystem;
	use crate::git::GitWorkdir;

	fn workdir() -> crate::path::AbsolutePath {
		crate::path::AbsolutePath::new("/tmp").unwrap()
	}

	// --- GitHubRepo::parse_url ---

	#[test]
	fn parse_https_with_git_suffix() {
		let result = GitHubRepo::parse_url("https://github.com/owner/repo.git");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_https_without_git_suffix() {
		let result = GitHubRepo::parse_url("https://github.com/owner/repo");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_ssh_with_git_suffix() {
		let result = GitHubRepo::parse_url("git@github.com:owner/repo.git");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_ssh_without_git_suffix() {
		let result = GitHubRepo::parse_url("git@github.com:owner/repo");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_non_github_https_returns_none() {
		assert!(GitHubRepo::parse_url("https://gitlab.com/owner/repo.git").is_none());
	}

	#[test]
	fn parse_non_github_ssh_returns_none() {
		assert!(GitHubRepo::parse_url("git@gitlab.com:owner/repo.git").is_none());
	}

	#[test]
	fn parse_empty_returns_none() {
		assert!(GitHubRepo::parse_url("").is_none());
	}

	#[test]
	fn parse_malformed_returns_none() {
		assert!(GitHubRepo::parse_url("not-a-url").is_none());
	}

	#[test]
	fn parse_extra_path_segments_returns_none() {
		assert!(GitHubRepo::parse_url("https://github.com/owner/repo/extra").is_none());
	}

	#[test]
	fn parse_ssh_extra_path_segments_returns_none() {
		assert!(GitHubRepo::parse_url("git@github.com:owner/repo/extra").is_none());
	}

	#[test]
	fn parse_trailing_slash_returns_none() {
		// Trailing slash is not a standard git remote format; reject it.
		assert!(GitHubRepo::parse_url("https://github.com/owner/repo/").is_none());
	}

	#[test]
	fn parse_ssh_url_with_git_suffix() {
		let result = GitHubRepo::parse_url("ssh://git@github.com/owner/repo.git");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_ssh_url_without_git_suffix() {
		let result = GitHubRepo::parse_url("ssh://git@github.com/owner/repo");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_ssh_url_without_user() {
		let result = GitHubRepo::parse_url("ssh://github.com/owner/repo.git");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_ssh_url_with_port() {
		let result = GitHubRepo::parse_url("ssh://git@github.com:22/owner/repo.git");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_https_with_port() {
		let result = GitHubRepo::parse_url("https://github.com:443/owner/repo.git");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_https_with_port_no_git_suffix() {
		let result = GitHubRepo::parse_url("https://github.com:8080/owner/repo");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	#[test]
	fn parse_https_colon_no_digits_returns_none() {
		assert!(GitHubRepo::parse_url("https://github.com:/owner/repo").is_none());
	}

	#[test]
	fn parse_ssh_url_non_github_returns_none() {
		assert!(GitHubRepo::parse_url("ssh://git@gitlab.com/owner/repo.git").is_none());
	}

	#[test]
	fn parse_trims_whitespace() {
		let result = GitHubRepo::parse_url("  https://github.com/owner/repo.git\n");
		assert_eq!(result, Some(GitHubRepo::new("owner", "repo").unwrap()));
	}

	// --- GitHubRepo::detect_in ---

	#[test]
	fn detect_returns_repo_for_https_remote() {
		let runner = Arc::new(
			RecordingCommandRunner::new(0)
				.with_stdout(b"https://github.com/acme/app.git\n".to_vec()),
		);
		let wd = workdir();
		let git = GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);
		let result = GitHubRepo::detect_in(&git).unwrap();
		assert_eq!(result, Some(GitHubRepo::new("acme", "app").unwrap()));
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, ["remote", "get-url", "origin"]);
	}

	#[test]
	fn detect_returns_repo_for_ssh_remote() {
		let runner = Arc::new(
			RecordingCommandRunner::new(0).with_stdout(b"git@github.com:acme/app.git\n".to_vec()),
		);
		let wd = workdir();
		let git = GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);
		let result = GitHubRepo::detect_in(&git).unwrap();
		assert_eq!(result, Some(GitHubRepo::new("acme", "app").unwrap()));
	}

	#[test]
	fn detect_returns_none_when_git_fails() {
		let runner = Arc::new(RecordingCommandRunner::new(1));
		let wd = workdir();
		let git = GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);
		let result = GitHubRepo::detect_in(&git).unwrap();
		assert_eq!(result, None);
	}

	#[test]
	fn detect_returns_none_for_non_github_url() {
		let runner = Arc::new(
			RecordingCommandRunner::new(0)
				.with_stdout(b"https://gitlab.com/owner/repo.git\n".to_vec()),
		);
		let wd = workdir();
		let git = GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);
		let result = GitHubRepo::detect_in(&git).unwrap();
		assert_eq!(result, None);
	}

	// --- GitHubRepo::resolve ---

	fn make_github_config(owner: Option<&str>, repo: Option<&str>) -> GitHubConfig {
		let mut config = GitHubConfig::enabled_config();
		if let Some(o) = owner {
			config = config.with_owner(o.to_string());
		}
		if let Some(r) = repo {
			config = config.with_repo(r.to_string());
		}
		config
	}

	#[test]
	fn resolve_github_repo_uses_config_when_set() {
		let config = make_github_config(Some("acme"), Some("app"));
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);

		let gh_repo = GitHubRepo::resolve(&config, &git).unwrap();
		assert_eq!(gh_repo.owner, "acme");
		assert_eq!(gh_repo.repo, "app");
		// Config values take priority — no git command should run
		assert!(runner.invocations().is_empty());
	}

	#[test]
	fn resolve_github_repo_falls_back_to_git_remote() {
		let config = make_github_config(None, None);
		let runner = Arc::new(
			RecordingCommandRunner::new(0)
				.with_stdout(b"https://github.com/myorg/myapp.git\n".to_vec()),
		);
		let wd = workdir();
		let git = GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);

		let gh_repo = GitHubRepo::resolve(&config, &git).unwrap();
		assert_eq!(gh_repo.owner, "myorg");
		assert_eq!(gh_repo.repo, "myapp");
	}

	#[test]
	fn resolve_github_repo_errors_when_neither_config_nor_remote() {
		let config = make_github_config(None, None);
		let runner = Arc::new(RecordingCommandRunner::new(1)); // no origin remote
		let wd = workdir();
		let git = GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);

		let result = GitHubRepo::resolve(&config, &git);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("Could not determine GitHub repository"),
			"Expected repo detection error, got: {msg}"
		);
	}

	#[test]
	fn resolve_github_repo_errors_when_only_owner_set() {
		let config = make_github_config(Some("acme"), None);
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);

		let result = GitHubRepo::resolve(&config, &git);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("must be set together"),
			"Expected partial config error, got: {msg}"
		);
	}

	#[test]
	fn resolve_github_repo_errors_when_only_repo_set() {
		let config = make_github_config(None, Some("app"));
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);

		let result = GitHubRepo::resolve(&config, &git);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("must be set together"),
			"Expected partial config error, got: {msg}"
		);
	}

	// --- GitHubRepo::new validation ---

	#[test]
	fn new_accepts_valid_names() {
		assert!(GitHubRepo::new("acme", "my-repo").is_ok());
		assert!(GitHubRepo::new("my-org", "my_repo.js").is_ok());
		assert!(GitHubRepo::new("Org123", "repo").is_ok());
	}

	#[test]
	fn new_rejects_invalid_owner() {
		assert!(GitHubRepo::new("", "repo").is_err());
		assert!(GitHubRepo::new("a/b", "repo").is_err());
		assert!(GitHubRepo::new("../evil", "repo").is_err());
		assert!(GitHubRepo::new("a b", "repo").is_err());
	}

	#[test]
	fn new_rejects_invalid_repo() {
		assert!(GitHubRepo::new("owner", "").is_err());
		assert!(GitHubRepo::new("owner", "a/b").is_err());
		assert!(GitHubRepo::new("owner", "../evil").is_err());
	}
}
