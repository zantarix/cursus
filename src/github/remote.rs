//! GitHub remote URL detection and parsing.

use std::path::Path;

use anyhow::Context;

use crate::command::CommandRunner;

/// A parsed GitHub repository owner and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepo {
	/// GitHub organisation or user name.
	pub owner: String,
	/// GitHub repository name.
	pub repo: String,
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

/// Parses a git remote URL into a [`GitHubRepo`] if it points to GitHub.
///
/// Supported formats:
/// - HTTPS: `https://github.com[:<port>]/owner/repo[.git]`
/// - SCP-syntax SSH: `git@github.com:owner/repo[.git]`
/// - SSH URL: `ssh://[user@]github.com[:<port>]/owner/repo[.git]`
///
/// Returns `None` for non-GitHub URLs, URLs with extra path segments, or
/// empty/malformed input.
pub fn parse_github_remote(url: &str) -> Option<GitHubRepo> {
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
pub fn detect_github_repo(
	runner: &dyn CommandRunner,
	git_workdir: &Path,
) -> anyhow::Result<Option<GitHubRepo>> {
	let output = runner
		.run("git", &["remote", "get-url", "origin"], git_workdir)
		.context("Failed to query git remote URL")?;

	if !output.status.success() {
		return Ok(None);
	}

	let url = String::from_utf8_lossy(&output.stdout);
	Ok(parse_github_remote(url.trim()))
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use super::*;
	use crate::command::test_support::RecordingCommandRunner;

	fn workdir() -> PathBuf {
		PathBuf::from("/tmp")
	}

	// --- parse_github_remote ---

	#[test]
	fn parse_https_with_git_suffix() {
		let result = parse_github_remote("https://github.com/owner/repo.git");
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
		let result = parse_github_remote("https://github.com/owner/repo");
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
		let result = parse_github_remote("git@github.com:owner/repo.git");
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
		let result = parse_github_remote("git@github.com:owner/repo");
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
		assert!(parse_github_remote("https://gitlab.com/owner/repo.git").is_none());
	}

	#[test]
	fn parse_non_github_ssh_returns_none() {
		assert!(parse_github_remote("git@gitlab.com:owner/repo.git").is_none());
	}

	#[test]
	fn parse_empty_returns_none() {
		assert!(parse_github_remote("").is_none());
	}

	#[test]
	fn parse_malformed_returns_none() {
		assert!(parse_github_remote("not-a-url").is_none());
	}

	#[test]
	fn parse_extra_path_segments_returns_none() {
		assert!(parse_github_remote("https://github.com/owner/repo/extra").is_none());
	}

	#[test]
	fn parse_ssh_extra_path_segments_returns_none() {
		assert!(parse_github_remote("git@github.com:owner/repo/extra").is_none());
	}

	#[test]
	fn parse_trailing_slash_returns_none() {
		// Trailing slash is not a standard git remote format; reject it.
		assert!(parse_github_remote("https://github.com/owner/repo/").is_none());
	}

	#[test]
	fn parse_ssh_url_with_git_suffix() {
		let result = parse_github_remote("ssh://git@github.com/owner/repo.git");
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
		let result = parse_github_remote("ssh://git@github.com/owner/repo");
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
		let result = parse_github_remote("ssh://github.com/owner/repo.git");
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
		let result = parse_github_remote("ssh://git@github.com:22/owner/repo.git");
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
		let result = parse_github_remote("https://github.com:443/owner/repo.git");
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
		let result = parse_github_remote("https://github.com:8080/owner/repo");
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
		assert!(parse_github_remote("https://github.com:/owner/repo").is_none());
	}

	#[test]
	fn parse_ssh_url_non_github_returns_none() {
		assert!(parse_github_remote("ssh://git@gitlab.com/owner/repo.git").is_none());
	}

	#[test]
	fn parse_trims_whitespace() {
		let result = parse_github_remote("  https://github.com/owner/repo.git\n");
		assert_eq!(
			result,
			Some(GitHubRepo {
				owner: "owner".to_string(),
				repo: "repo".to_string(),
			})
		);
	}

	// --- detect_github_repo ---

	#[test]
	fn detect_returns_repo_for_https_remote() {
		let runner = RecordingCommandRunner::new(0)
			.with_stdout(b"https://github.com/acme/app.git\n".to_vec());
		let result = detect_github_repo(&runner, &workdir()).unwrap();
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
		let result = detect_github_repo(&runner, &workdir()).unwrap();
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
		let result = detect_github_repo(&runner, &workdir()).unwrap();
		assert_eq!(result, None);
	}

	#[test]
	fn detect_returns_none_for_non_github_url() {
		let runner = RecordingCommandRunner::new(0)
			.with_stdout(b"https://gitlab.com/owner/repo.git\n".to_vec());
		let result = detect_github_repo(&runner, &workdir()).unwrap();
		assert_eq!(result, None);
	}
}
