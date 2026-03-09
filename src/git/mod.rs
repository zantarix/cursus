//! Git lifecycle automation for Chronicle releases.
//!
//! This module provides optional post-release git automation: creating a commit,
//! tagging each released package, and optionally pushing to origin.

mod config;
mod operations;

pub use config::{GitConfig, GitStep, TagFormat};

use std::path::{Path, PathBuf};

use anyhow::Context;
use semver::Version;

use crate::command::CommandRunner;

/// Information about a single package that was released.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
	/// The name of the released package.
	pub package_name: String,
	/// The new version after the release.
	pub new_version: Version,
}

/// Normalises a path by resolving `.` and `..` components without requiring
/// the path to exist on disk.
fn normalize_path(path: &Path) -> PathBuf {
	use std::path::Component;
	let mut result = PathBuf::new();
	for component in path.components() {
		match component {
			Component::ParentDir => {
				result.pop();
			}
			Component::CurDir => {}
			c => result.push(c),
		}
	}
	result
}

/// Formats the git commit message for a release.
///
/// Produces `chore(release): pkg1@1.0.0, pkg2@2.0.0` for the given releases.
///
/// # Note
///
/// `releases` must be non-empty; passing an empty slice produces the malformed
/// string `"chore(release): "`. This is guaranteed by the early return in
/// [`run_git_lifecycle`] which skips all git operations when there are no releases.
pub(crate) fn format_commit_message(releases: &[ReleaseInfo]) -> String {
	let parts: Vec<String> = releases
		.iter()
		.map(|r| format!("{}@{}", r.package_name, r.new_version))
		.collect();
	format!("chore(release): {}", parts.join(", "))
}

/// Runs the git lifecycle after a release: commit, optionally tag, optionally push.
///
/// If `dry_run` is `true`, prints what would be done without executing any git commands.
///
/// # Arguments
///
/// * `git_workdir` - Root of the git repository.
/// * `config` - Git lifecycle configuration.
/// * `releases` - The packages that were released.
/// * `modified_files` - Files to stage before committing.
/// * `total_project_count` - Total number of projects in the workspace (used for auto tag format).
/// * `dry_run` - If `true`, only print a summary; do not modify git state.
/// * `runner` - Command runner for executing git commands.
///
/// # Errors
///
/// Returns an error if any git command fails.
pub fn run_git_lifecycle(
	git_workdir: &Path,
	config: &GitConfig,
	releases: &[ReleaseInfo],
	modified_files: &[PathBuf],
	total_project_count: usize,
	dry_run: bool,
	runner: &dyn CommandRunner,
) -> anyhow::Result<()> {
	if releases.is_empty() {
		return Ok(());
	}

	let commit_message = format_commit_message(releases);
	let is_multi_package = total_project_count > 1;

	let tags: Vec<String> = releases
		.iter()
		.map(|r| {
			config
				.tag_format
				.tag(&r.package_name, &r.new_version, is_multi_package)
		})
		.collect();

	// Build the full staging list, validating that extra_files resolve inside the repo root.
	let mut all_files = modified_files.to_vec();
	for f in &config.extra_files {
		let resolved = normalize_path(&git_workdir.join(f));
		if !resolved.starts_with(git_workdir) {
			anyhow::bail!(
				"extra_files entry {:?} resolves outside the repository root",
				f
			);
		}
		all_files.push(resolved);
	}

	if dry_run {
		println!("Would create commit: {commit_message}");
		println!("Would stage files:");
		for file in &all_files {
			println!("  {}", file.display());
		}
		if config.run_until.should_tag() {
			for tag in &tags {
				println!("Would create tag: {tag}");
			}
		}
		if config.run_until.should_push() {
			println!("Would push: git push origin HEAD --follow-tags");
		}
		return Ok(());
	}

	// Stage and commit
	if config.run_until.should_commit() {
		operations::git_add(runner, git_workdir, &all_files)
			.context("Failed to stage files for git commit")?;
		operations::git_commit(runner, git_workdir, &commit_message)
			.context("Failed to create git commit")?;
	}

	// Tag each release
	if config.run_until.should_tag() {
		for (release, tag) in releases.iter().zip(tags.iter()) {
			let tag_message = format!(
				"Release {} version {}",
				release.package_name, release.new_version
			);
			operations::git_tag(runner, git_workdir, tag, &tag_message)
				.with_context(|| format!("Failed to create git tag: {tag}"))?;
		}
	}

	// Push
	if config.run_until.should_push() {
		operations::git_push(runner, git_workdir).context("Failed to push to remote")?;
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use crate::command::test_support::RecordingCommandRunner;

	use super::*;

	#[test]
	fn format_commit_message_single_package() {
		let releases = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
		}];
		assert_eq!(
			format_commit_message(&releases),
			"chore(release): my-pkg@1.0.0"
		);
	}

	#[test]
	fn format_commit_message_multiple_packages() {
		let releases = vec![
			ReleaseInfo {
				package_name: "pkg-a".to_string(),
				new_version: "1.0.0".parse().unwrap(),
			},
			ReleaseInfo {
				package_name: "pkg-b".to_string(),
				new_version: "2.1.0".parse().unwrap(),
			},
		];
		assert_eq!(
			format_commit_message(&releases),
			"chore(release): pkg-a@1.0.0, pkg-b@2.1.0"
		);
	}

	#[test]
	fn format_commit_message_empty() {
		assert_eq!(format_commit_message(&[]), "chore(release): ");
	}

	#[test]
	fn tag_format_auto_single_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(TagFormat::Auto.tag("my-pkg", &version, false), "v1.2.3");
	}

	#[test]
	fn tag_format_auto_multi_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(
			TagFormat::Auto.tag("my-pkg", &version, true),
			"my-pkg@1.2.3"
		);
	}

	#[test]
	fn tag_format_prefixed_single_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(
			TagFormat::Prefixed.tag("my-pkg", &version, false),
			"my-pkg@1.2.3"
		);
	}

	#[test]
	fn tag_format_prefixed_multi_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(
			TagFormat::Prefixed.tag("my-pkg", &version, true),
			"my-pkg@1.2.3"
		);
	}

	#[test]
	fn tag_format_simple_single_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(TagFormat::Simple.tag("my-pkg", &version, false), "v1.2.3");
	}

	#[test]
	fn tag_format_simple_multi_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(TagFormat::Simple.tag("my-pkg", &version, true), "v1.2.3");
	}

	#[test]
	fn normalize_path_collapses_parent_dir() {
		let p = std::path::Path::new("/repo/foo/../bar");
		assert_eq!(normalize_path(p), std::path::Path::new("/repo/bar"));
	}

	#[test]
	fn normalize_path_collapses_current_dir() {
		let p = std::path::Path::new("/repo/./foo");
		assert_eq!(normalize_path(p), std::path::Path::new("/repo/foo"));
	}

	#[test]
	fn extra_files_outside_repo_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		let config = GitConfig {
			enabled: Some(true),
			extra_files: vec!["../../etc/passwd".to_string()],
			..Default::default()
		};
		let releases = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
		}];
		let runner = RecordingCommandRunner::new(0);
		let result = run_git_lifecycle(dir.path(), &config, &releases, &[], 1, true, &runner);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[test]
	fn extra_files_absolute_path_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		let config = GitConfig {
			enabled: Some(true),
			extra_files: vec!["/etc/passwd".to_string()],
			..Default::default()
		};
		let releases = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
		}];
		let runner = RecordingCommandRunner::new(0);
		let result = run_git_lifecycle(dir.path(), &config, &releases, &[], 1, true, &runner);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[test]
	fn run_git_lifecycle_empty_releases_is_noop() {
		let dir = tempfile::tempdir().unwrap();
		let config = GitConfig {
			enabled: Some(true),
			..Default::default()
		};
		// Empty releases → returns Ok immediately without touching git
		let runner = RecordingCommandRunner::new(0);
		let result = run_git_lifecycle(dir.path(), &config, &[], &[], 1, false, &runner);
		assert!(result.is_ok());
	}

	#[test]
	fn run_git_lifecycle_dry_run_prints_summary() {
		let dir = tempfile::tempdir().unwrap();
		let config = GitConfig {
			enabled: Some(true),
			..Default::default()
		};
		let releases = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
		}];
		// Dry run should not execute any git commands
		let runner = RecordingCommandRunner::new(0);
		let result = run_git_lifecycle(dir.path(), &config, &releases, &[], 1, true, &runner);
		assert!(result.is_ok());
	}

	#[test]
	fn run_git_lifecycle_dry_run_with_push_enabled_prints_summary() {
		let dir = tempfile::tempdir().unwrap();
		let config = GitConfig {
			enabled: Some(true),
			run_until: GitStep::Push,
			..Default::default()
		};
		let releases = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
		}];
		// Dry run with push enabled should print "Would push" without running git
		let runner = RecordingCommandRunner::new(0);
		let result = run_git_lifecycle(dir.path(), &config, &releases, &[], 1, true, &runner);
		assert!(result.is_ok());
	}
}
