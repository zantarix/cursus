//! GitHub remote URL detection and parsing.

use std::path::Path;

use anyhow::{Context, bail};

use crate::command::CommandRunner;
use crate::github::GitHubConfig;

/// A parsed GitHub repository owner and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepo {
	/// GitHub organisation or user name.
	pub owner: String,
	/// GitHub repository name.
	pub repo: String,
}

impl GitHubRepo {
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
		if repo.contains('/') || owner.is_empty() || repo.is_empty() {
			return None;
		}
		Some(GitHubRepo {
			owner: owner.to_string(),
			repo: repo.to_string(),
		})
	}

	/// Detects the GitHub repository for a git working directory.
	///
	/// Runs `git remote get-url origin` and parses the output. Returns `Ok(None)`
	/// if there is no `origin` remote or the URL does not point to GitHub.
	///
	/// # Errors
	///
	/// Returns an error if the git command cannot be executed.
	pub fn detect_in(
		git_workdir: &Path,
		runner: &dyn CommandRunner,
	) -> anyhow::Result<Option<Self>> {
		let output = runner
			.run("git", &["remote", "get-url", "origin"], git_workdir)
			.context("Failed to query git remote URL")?;

		if !output.status.success() {
			return Ok(None);
		}

		let url = String::from_utf8_lossy(&output.stdout);
		Ok(Self::parse_url(url.trim()))
	}

	/// Resolves the GitHub repository from config or by detecting from the git remote.
	///
	/// Checks `owner` and `repo` config fields first, then falls back to
	/// detecting from the git remote URL.
	///
	/// # Errors
	///
	/// Returns an error if both config fields are partially set (one set, one not),
	/// or if neither config nor remote detection can determine the repository.
	pub fn resolve(
		github_config: &GitHubConfig,
		git_workdir: &Path,
		runner: &dyn CommandRunner,
	) -> anyhow::Result<Self> {
		match (&github_config.owner, &github_config.repo) {
			(Some(owner), Some(repo)) => {
				return Ok(GitHubRepo {
					owner: owner.clone(),
					repo: repo.clone(),
				});
			}
			(Some(_), None) | (None, Some(_)) => bail!(
				"[github].owner and [github].repo must be set together; \
				 set both or omit both for auto-detection."
			),
			(None, None) => {}
		}

		match Self::detect_in(git_workdir, runner)? {
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
	use std::path::PathBuf;

	use super::*;
	use crate::command::test_support::RecordingCommandRunner;

	fn workdir() -> PathBuf {
		PathBuf::from("/tmp")
	}

	// --- GitHubRepo::parse_url ---

	#[test]
	fn parse_https_with_git_suffix() {
		let result = GitHubRepo::parse_url("https://github.com/owner/repo.git");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	#[test]
	fn parse_https_without_git_suffix() {
		let result = GitHubRepo::parse_url("https://github.com/owner/repo");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	#[test]
	fn parse_ssh_with_git_suffix() {
		let result = GitHubRepo::parse_url("git@github.com:owner/repo.git");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	#[test]
	fn parse_ssh_without_git_suffix() {
		let result = GitHubRepo::parse_url("git@github.com:owner/repo");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
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
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	#[test]
	fn parse_ssh_url_without_git_suffix() {
		let result = GitHubRepo::parse_url("ssh://git@github.com/owner/repo");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	#[test]
	fn parse_ssh_url_without_user() {
		let result = GitHubRepo::parse_url("ssh://github.com/owner/repo.git");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	#[test]
	fn parse_ssh_url_with_port() {
		let result = GitHubRepo::parse_url("ssh://git@github.com:22/owner/repo.git");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	#[test]
	fn parse_https_with_port() {
		let result = GitHubRepo::parse_url("https://github.com:443/owner/repo.git");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	#[test]
	fn parse_https_with_port_no_git_suffix() {
		let result = GitHubRepo::parse_url("https://github.com:8080/owner/repo");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
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
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	// --- GitHubRepo::detect_in ---

	#[test]
	fn detect_returns_repo_for_https_remote() {
		let runner = RecordingCommandRunner::new(0)
			.with_stdout(b"https://github.com/acme/app.git\n".to_vec());
		let result = GitHubRepo::detect_in(&workdir(), &runner).unwrap();
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "acme".to_string(),
				repo: "app".to_string(),
			})
		);
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, ["remote", "get-url", "origin"]);
	}

	#[test]
	fn detect_returns_repo_for_ssh_remote() {
		let runner =
			RecordingCommandRunner::new(0).with_stdout(b"git@github.com:acme/app.git\n".to_vec());
		let result = GitHubRepo::detect_in(&workdir(), &runner).unwrap();
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "acme".to_string(),
				repo: "app".to_string(),
			})
		);
	}

	#[test]
	fn detect_returns_none_when_git_fails() {
		let runner = RecordingCommandRunner::new(1);
		let result = GitHubRepo::detect_in(&workdir(), &runner).unwrap();
		assert_eq!(result, None);
	}

	#[test]
	fn detect_returns_none_for_non_github_url() {
		let runner = RecordingCommandRunner::new(0)
			.with_stdout(b"https://gitlab.com/owner/repo.git\n".to_vec());
		let result = GitHubRepo::detect_in(&workdir(), &runner).unwrap();
		assert_eq!(result, None);
	}

	// --- GitHubRepo::resolve ---

	fn make_github_config(owner: Option<&str>, repo: Option<&str>) -> GitHubConfig {
		GitHubConfig {
			enabled: true,
			owner: owner.map(str::to_string),
			repo: repo.map(str::to_string),
			..Default::default()
		}
	}

	#[test]
	fn resolve_github_repo_uses_config_when_set() {
		let config = make_github_config(Some("acme"), Some("app"));
		let runner = RecordingCommandRunner::new(0);

		let gh_repo = GitHubRepo::resolve(&config, &workdir(), &runner).unwrap();
		assert_eq!(gh_repo.owner, "acme");
		assert_eq!(gh_repo.repo, "app");
		// Config values take priority — no git command should run
		assert!(runner.invocations().is_empty());
	}

	#[test]
	fn resolve_github_repo_falls_back_to_git_remote() {
		let config = make_github_config(None, None);
		let runner = RecordingCommandRunner::new(0)
			.with_stdout(b"https://github.com/myorg/myapp.git\n".to_vec());

		let gh_repo = GitHubRepo::resolve(&config, &workdir(), &runner).unwrap();
		assert_eq!(gh_repo.owner, "myorg");
		assert_eq!(gh_repo.repo, "myapp");
	}

	#[test]
	fn resolve_github_repo_errors_when_neither_config_nor_remote() {
		let config = make_github_config(None, None);
		let runner = RecordingCommandRunner::new(1); // no origin remote

		let result = GitHubRepo::resolve(&config, &workdir(), &runner);
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
		let runner = RecordingCommandRunner::new(0);

		let result = GitHubRepo::resolve(&config, &workdir(), &runner);
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
		let runner = RecordingCommandRunner::new(0);

		let result = GitHubRepo::resolve(&config, &workdir(), &runner);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("must be set together"),
			"Expected partial config error, got: {msg}"
		);
	}
}
