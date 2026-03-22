//! GitHub Release creation, artifact upload, and build command orchestration.

use anyhow::Context;
use log::{error, info, warn};

use crate::git;
use crate::github::GitHubRepo;
use crate::github::client::GitHubClient;
use crate::model::changelog::extract_version_body;
use crate::model::config::Config;

use super::PublishedPackage;

/// Logs what GitHub Releases and artifacts would be created in a dry run.
pub(super) fn log_dry_run_github_releases(
	published_packages: &[PublishedPackage],
	config: &crate::model::config::Config,
	is_multi_package: bool,
) {
	for pkg in published_packages {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);
		info!("Would create GitHub Release for {tag}");
		for display_name in config.github.artifacts.keys() {
			info!("  Would attach: {display_name}");
		}
		info!("  Would publish release after artifact upload");
	}
}

/// Runs the configured GitHub pre-release build command, if any.
///
/// Returns `true` if the build command failed, `false` if it succeeded or was not configured.
pub(super) fn run_github_build_command(
	env: &crate::Env,
	config: &Config,
	git: &git::GitWorkdir,
) -> anyhow::Result<bool> {
	if config.github.build_command.is_empty() {
		return Ok(false);
	}
	info!("Running build command: {}", config.github.build_command);
	let output = env
		.run_shell_mut(&config.github.build_command, git.path())
		.with_context(|| {
			format!(
				"Failed to execute build command: {}",
				config.github.build_command
			)
		})?;
	if !output.status.success() {
		error!("Build command failed with status {}", output.status);
		return Ok(true);
	}
	Ok(false)
}

/// Reads the changelog body for a published package, returning an empty string on any error.
pub(super) fn read_changelog_body(pkg: &PublishedPackage) -> String {
	let changelog_path = pkg.project_path.join("CHANGELOG.md");
	if !changelog_path.exists() {
		return String::new();
	}
	match extract_version_body(&changelog_path, &pkg.version) {
		Ok(text) => text,
		Err(e) => {
			warn!("could not read changelog for {}: {e:#}", pkg.name);
			String::new()
		}
	}
}

/// Uploads artifacts then publishes a draft release.
///
/// Returns `true` if any step failed (the release is left as a draft on upload failure).
pub(super) fn publish_draft_release(
	github_client: &dyn GitHubClient,
	gh_repo: &GitHubRepo,
	tag: &str,
	release_id: &str,
	artifacts: &std::collections::BTreeMap<String, String>,
	git_root: &crate::path::AbsolutePath,
) -> bool {
	if upload_release_artifacts(github_client, gh_repo, release_id, artifacts, git_root) {
		warn!("Artifact uploads failed for {tag}; leaving release as a draft");
		return true;
	}
	match github_client.publish_release(gh_repo, release_id) {
		Ok(()) => {
			info!("Created GitHub Release for {tag}");
			false
		}
		Err(e) => {
			error!("Failed to publish GitHub Release for {tag}: {e:#}");
			true
		}
	}
}

/// Orchestrates GitHub Release creation for all successfully published packages.
///
/// The caller must ensure that a GitHub token is available and that `github_client`
/// is `Some` before calling this function (enforced by the early check in `cmd_publish`).
///
/// Returns `(releases_created, any_failed)`.
pub(super) fn orchestrate_github_releases(
	git: &git::GitWorkdir,
	config: &Config,
	github_client: &dyn GitHubClient,
	published_packages: &[PublishedPackage],
	is_multi_package: bool,
) -> anyhow::Result<(usize, bool)> {
	if published_packages.is_empty() {
		return Ok((0, false));
	}
	let gh_repo = GitHubRepo::resolve(&config.github, git)?;
	let mut github_failed = false;
	let mut created_count = 0;
	for pkg in published_packages {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);
		let body = read_changelog_body(pkg);
		match github_client.create_release(&gh_repo, &tag, &tag, &body) {
			Ok(release_id) => {
				if publish_draft_release(
					github_client,
					&gh_repo,
					&tag,
					&release_id,
					&config.github.artifacts,
					git.path(),
				) {
					github_failed = true;
				} else {
					created_count += 1;
				}
			}
			Err(e) => {
				error!("Failed to create GitHub Release for {tag}: {e:#}");
				github_failed = true;
			}
		}
	}
	Ok((created_count, github_failed))
}

/// Uploads all configured artifacts to a GitHub release.
///
/// Returns `true` if any upload failed, `false` if all succeeded.
pub(super) fn upload_release_artifacts(
	github_client: &dyn GitHubClient,
	gh_repo: &GitHubRepo,
	release_id: &str,
	artifacts: &std::collections::BTreeMap<String, String>,
	git_root: &crate::path::AbsolutePath,
) -> bool {
	let mut any_failed = false;
	for (display_name, artifact_path) in artifacts {
		let full_path = match git_root.subpath(artifact_path) {
			Ok(p) => p,
			Err(e) => {
				warn!("  Skipping '{display_name}': invalid artifact path: {e:#}");
				any_failed = true;
				continue;
			}
		};
		match github_client.upload_asset(gh_repo, release_id, display_name, &full_path) {
			Ok(()) => info!("  Attached: {display_name}"),
			Err(e) => {
				warn!("  Failed to attach '{display_name}': {e:#}");
				any_failed = true;
			}
		}
	}
	any_failed
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::sync::Arc;

	use super::*;
	use crate::cli::publish::tests_common::{make_github_config, workdir};
	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::filesystem::LocalFilesystem;
	use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
	use crate::model::config::{Config, GitHubConfig};
	use crate::path::AbsolutePath;

	// --- Tests for orchestrate_github_releases ---

	#[test]
	fn github_release_skipped_when_no_published_packages() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git = git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);

		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &[], false).unwrap();

		assert_eq!(created, 0);
		assert!(!failed);
		assert!(client.invocations().is_empty());
	}

	#[test]
	fn github_releases_created_for_published_packages() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git = git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);
		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, false).unwrap();

		assert_eq!(created, 1);
		assert!(!failed);
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 2);
		assert!(matches!(
			&invocations[0],
			GitHubInvocation::CreateRelease { tag_name, gh_repo, .. }
				if tag_name == "v1.2.0" && gh_repo.owner == "acme" && gh_repo.repo == "app"
		));
		assert!(matches!(
			&invocations[1],
			GitHubInvocation::PublishRelease { release_id, .. } if release_id == "release-1"
		));
	}

	#[test]
	fn github_releases_uses_prefixed_tag_for_monorepo() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git = git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);
		let (created, failed) = orchestrate_github_releases(
			&git, &config, &client, &packages, true, // is_multi_package
		)
		.unwrap();

		assert_eq!(created, 1);
		assert!(!failed);
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 2);
		assert!(matches!(
			&invocations[0],
			GitHubInvocation::CreateRelease { tag_name, .. } if tag_name == "my-app@1.2.0"
		));
		assert!(matches!(
			&invocations[1],
			GitHubInvocation::PublishRelease { .. }
		));
	}

	#[test]
	fn github_release_create_failure_continues_other_packages() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new().with_create_failure();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![
			PublishedPackage {
				name: "pkg-a".to_string(),
				version: "1.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
			PublishedPackage {
				name: "pkg-b".to_string(),
				version: "2.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
		];

		let wd = workdir();
		let git = git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);
		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, true).unwrap();

		assert_eq!(created, 0);
		assert!(failed);
		// Both packages should have been attempted
		assert_eq!(client.invocations().len(), 2);
	}

	#[test]
	fn github_release_upload_failure_continues_other_artifacts() {
		// Create the artifact files
		let dir = tempfile::tempdir().unwrap();
		let linux_path = dir.path().join("linux.tar.gz");
		let macos_path = dir.path().join("macos.tar.gz");
		std::fs::write(&linux_path, b"linux binary").unwrap();
		std::fs::write(&macos_path, b"macos binary").unwrap();

		let mut artifacts_with_paths = BTreeMap::new();
		artifacts_with_paths.insert(
			"linux".to_string(),
			linux_path.to_string_lossy().into_owned(),
		);
		artifacts_with_paths.insert(
			"macos".to_string(),
			macos_path.to_string_lossy().into_owned(),
		);

		let github_cfg = {
			let mut c = GitHubConfig::enabled_config();
			c.artifacts = artifacts_with_paths;
			c.with_owner("acme".to_string())
				.with_repo("app".to_string())
		};

		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_github(github_cfg);
		let client = RecordingGitHubClient::new().with_upload_failure();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			dir_abs.clone(),
		);

		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, false).unwrap();

		// Draft was created but upload failed — not counted as created (left as draft)
		assert_eq!(created, 0);
		assert!(failed);

		let invocations = client.invocations();
		// Both artifacts were attempted despite first failure
		let uploads: Vec<_> = invocations
			.iter()
			.filter(|i| matches!(i, GitHubInvocation::UploadAsset { .. }))
			.collect();
		assert_eq!(uploads.len(), 2);
		// Release must NOT be published when uploads failed (left as draft)
		assert!(
			!invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::PublishRelease { .. })),
			"PublishRelease should not be called when uploads fail"
		);
	}

	#[test]
	fn github_release_artifacts_attached_to_every_release() {
		let dir = tempfile::tempdir().unwrap();
		let artifact_path = dir.path().join("app.tar.gz");
		std::fs::write(&artifact_path, b"binary content").unwrap();

		let mut artifacts = BTreeMap::new();
		artifacts.insert(
			"app".to_string(),
			artifact_path.to_string_lossy().into_owned(),
		);

		let github_cfg = {
			let mut c = GitHubConfig::enabled_config();
			c.artifacts = artifacts;
			c.with_owner("acme".to_string())
				.with_repo("app".to_string())
		};
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_github(github_cfg);
		let client = RecordingGitHubClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![
			PublishedPackage {
				name: "pkg-a".to_string(),
				version: "1.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
			PublishedPackage {
				name: "pkg-b".to_string(),
				version: "2.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
		];
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			dir_abs.clone(),
		);

		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, true).unwrap();

		assert_eq!(created, 2);
		assert!(!failed);

		let invocations = client.invocations();
		let upload_count = invocations
			.iter()
			.filter(|i| matches!(i, GitHubInvocation::UploadAsset { .. }))
			.count();
		// Each of 2 packages should have 1 artifact each
		assert_eq!(upload_count, 2);
	}

	#[test]
	fn github_release_publish_failure_sets_github_failed() {
		let config = Config::new(&workdir()).with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingGitHubClient::new().with_publish_release_failure();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git = git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			wd.clone(),
		);
		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, false).unwrap();

		// Draft was created but publish failed — not counted as created
		assert_eq!(created, 0);
		assert!(failed);

		let invocations = client.invocations();
		assert!(matches!(
			&invocations[0],
			GitHubInvocation::CreateRelease { .. }
		));
		assert!(matches!(
			&invocations[1],
			GitHubInvocation::PublishRelease { .. }
		));
	}

	#[test]
	fn github_artifacts_each_release_includes_publish() {
		let dir = tempfile::tempdir().unwrap();
		let artifact_path = dir.path().join("app.tar.gz");
		std::fs::write(&artifact_path, b"binary content").unwrap();

		let mut artifacts = BTreeMap::new();
		artifacts.insert(
			"app".to_string(),
			artifact_path.to_string_lossy().into_owned(),
		);

		let github_cfg = {
			let mut c = GitHubConfig::enabled_config();
			c.artifacts = artifacts;
			c.with_owner("acme".to_string())
				.with_repo("app".to_string())
		};
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
			.with_github(github_cfg);
		let client = RecordingGitHubClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				Arc::new(LocalFilesystem),
			),
			dir_abs.clone(),
		);

		let (created, failed) =
			orchestrate_github_releases(&git, &config, &client, &packages, false).unwrap();

		assert_eq!(created, 1);
		assert!(!failed);

		// Sequence: CreateRelease, UploadAsset, PublishRelease
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 3);
		assert!(matches!(
			&invocations[0],
			GitHubInvocation::CreateRelease { .. }
		));
		assert!(matches!(
			&invocations[1],
			GitHubInvocation::UploadAsset { .. }
		));
		assert!(matches!(
			&invocations[2],
			GitHubInvocation::PublishRelease { .. }
		));
	}

	// --- Tests for publish_draft_release ---

	#[test]
	fn publish_draft_release_success_returns_false() {
		let client = RecordingGitHubClient::new();
		let gh_repo = GitHubRepo::new("acme", "app").unwrap();
		let failed = publish_draft_release(
			&client,
			&gh_repo,
			"v1.0.0",
			"release-1",
			&BTreeMap::new(),
			&workdir(),
		);
		assert!(!failed);
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(matches!(
			&invocations[0],
			GitHubInvocation::PublishRelease { release_id, .. } if release_id == "release-1"
		));
	}

	#[test]
	fn publish_draft_release_upload_failure_returns_true_no_publish() {
		let dir = tempfile::tempdir().unwrap();
		let artifact_path = dir.path().join("app.tar.gz");
		std::fs::write(&artifact_path, b"data").unwrap();

		let mut artifacts = BTreeMap::new();
		artifacts.insert(
			"app".to_string(),
			artifact_path.to_string_lossy().into_owned(),
		);

		let client = RecordingGitHubClient::new().with_upload_failure();
		let gh_repo = GitHubRepo::new("acme", "app").unwrap();
		let dir_abs = AbsolutePath::new(dir.path()).unwrap();
		let failed = publish_draft_release(
			&client,
			&gh_repo,
			"v1.0.0",
			"release-1",
			&artifacts,
			&dir_abs,
		);
		assert!(failed);
		// Upload was attempted; publish must NOT be called
		let invocations = client.invocations();
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::UploadAsset { .. }))
		);
		assert!(
			!invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::PublishRelease { .. }))
		);
	}

	#[test]
	fn publish_draft_release_publish_failure_returns_true() {
		let client = RecordingGitHubClient::new().with_publish_release_failure();
		let gh_repo = GitHubRepo::new("acme", "app").unwrap();
		let failed = publish_draft_release(
			&client,
			&gh_repo,
			"v1.0.0",
			"release-1",
			&BTreeMap::new(),
			&workdir(),
		);
		assert!(failed);
		assert!(matches!(
			&client.invocations()[0],
			GitHubInvocation::PublishRelease { .. }
		));
	}

	// --- Tests for read_changelog_body ---

	#[test]
	fn read_changelog_body_returns_empty_when_no_changelog() {
		let pkg = PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		};
		assert_eq!(read_changelog_body(&pkg), "");
	}

	#[test]
	fn read_changelog_body_returns_version_section() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("CHANGELOG.md"),
			"## 1.0.0\n\nFix a bug\n\n## 0.9.0\n\nOld release\n",
		)
		.unwrap();
		let pkg = PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new(dir.path()).unwrap(),
		};
		let body = read_changelog_body(&pkg);
		assert!(
			body.contains("Fix a bug"),
			"Expected changelog body, got: {body}"
		);
	}

	#[test]
	fn read_changelog_body_returns_empty_when_version_missing() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("CHANGELOG.md"), "## 0.9.0\n\nOld release\n").unwrap();
		let pkg = PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new(dir.path()).unwrap(),
		};
		// Version not found returns empty string
		assert_eq!(read_changelog_body(&pkg), "");
	}

	#[test]
	fn upload_release_artifacts_rejects_path_traversal() {
		let outer = tempfile::tempdir().unwrap();
		let inner = outer.path().join("repo");
		std::fs::create_dir(&inner).unwrap();
		let secret = outer.path().join("secret.txt");
		std::fs::write(&secret, b"sensitive").unwrap();

		let mut artifacts = BTreeMap::new();
		artifacts.insert("secret".to_string(), secret.to_string_lossy().into_owned());

		let client = RecordingGitHubClient::new();
		let gh_repo = GitHubRepo::new("acme", "app").unwrap();
		let git_root = AbsolutePath::new(&inner).unwrap();

		let failed =
			upload_release_artifacts(&client, &gh_repo, "release-1", &artifacts, &git_root);

		assert!(failed, "Expected failure for path traversal");
		// No upload should have been attempted
		assert!(
			client.invocations().is_empty(),
			"Upload should not be called for invalid path"
		);
	}

	// --- Tests for log_dry_run_github_releases ---

	#[test]
	fn log_dry_run_github_releases_emits_would_publish_always() {
		use crate::test_logging::{init_test_logger, take_logs};
		init_test_logger();
		let _ = take_logs();

		let wd = workdir();
		let config = Config::new(&wd).with_github(make_github_config("", BTreeMap::new()));
		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		log_dry_run_github_releases(&packages, &config, false);

		let logs = take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("Would publish release after artifact upload")),
			"Expected 'Would publish release after artifact upload' even without artifacts, got: {logs:?}"
		);
	}

	#[test]
	fn log_dry_run_github_releases_emits_would_attach_for_artifacts() {
		use crate::test_logging::{init_test_logger, take_logs};
		init_test_logger();
		let _ = take_logs();

		let wd = workdir();
		let mut artifacts = BTreeMap::new();
		artifacts.insert("linux-amd64".to_string(), "target/app".to_string());
		let config = Config::new(&wd).with_github(make_github_config("", artifacts));
		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		log_dry_run_github_releases(&packages, &config, false);

		let logs = take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("Would attach: linux-amd64")),
			"Expected artifact attachment log, got: {logs:?}"
		);
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("Would publish release after artifact upload")),
			"Expected publish log even with artifacts, got: {logs:?}"
		);
	}
}
