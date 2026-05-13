//! GitHub Release creation, artifact upload, and build command orchestration.

use anyhow::Context;
use log::{error, info, warn};

use crate::forge::CodeForgeClient;
use crate::git::Git;
use crate::model::changelog::extract_version_body;
use crate::model::config::Config;

use super::PublishedPackage;

/// Logs what releases and artifacts would be created on the active forge in a dry run.
///
/// `forge_name` is the active forge's user-facing label (e.g. `"GitHub"`,
/// `"GitLab"`); the caller obtains it from the configured
/// [`crate::forge::CodeForgeClient`] or falls back to a configured-forge label
/// when no client is available.
pub(super) fn log_dry_run_forge_releases(
	published_packages: &[PublishedPackage],
	config: &crate::model::config::Config,
	is_multi_package: bool,
	forge_name: &str,
) {
	for pkg in published_packages {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);
		info!("Would create {forge_name} Release for {tag}");
		if let Some(artifacts) = config.forge_artifacts().get(&pkg.name) {
			for display_name in artifacts.keys() {
				info!("  Would attach: {display_name}");
			}
		}
		info!("  Would publish release after artifact upload");
	}
}

/// Runs the active forge's pre-release build command, if any.
///
/// Reads `[github].build_command` or `[gitlab].build_command` depending on
/// which forge is enabled (GitHub-first when both are enabled).
///
/// Returns `true` if the build command failed, `false` if it succeeded or was not configured.
pub(super) async fn run_forge_build_command(
	env: &crate::Env,
	config: &Config,
	git: &dyn Git,
) -> anyhow::Result<bool> {
	let build_command = config.build_command();
	if build_command.is_empty() {
		return Ok(false);
	}
	let status = env
		.run_streaming(build_command, git.path())
		.await
		.with_context(|| format!("Failed to execute build command: {build_command}"))?;
	if !status.success() {
		error!("Build command failed with status {status}");
		return Ok(true);
	}
	Ok(false)
}

/// Reads the changelog body for a published package, returning an empty string on any error.
pub(super) async fn read_changelog_body(
	pkg: &PublishedPackage,
	fs: &dyn crate::filesystem::Filesystem,
) -> String {
	let changelog_path = pkg.project_path.child("CHANGELOG.md");
	if !fs.exists(&changelog_path).await.unwrap_or(false) {
		return String::new();
	}
	match extract_version_body(&changelog_path, &pkg.version, fs).await {
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
pub(super) async fn publish_draft_release(
	code_forge_client: &dyn CodeForgeClient,
	tag: &str,
	release_id: &str,
	artifacts: &std::collections::BTreeMap<String, String>,
	git_root: &crate::path::AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> bool {
	if upload_release_artifacts(code_forge_client, release_id, artifacts, git_root, fs).await {
		warn!("Artifact uploads failed for {tag}; leaving release as a draft");
		return true;
	}
	let forge = code_forge_client.forge_name();
	match code_forge_client.publish_release(release_id).await {
		Ok(()) => {
			info!("Created {forge} Release for {tag}");
			false
		}
		Err(e) => {
			error!("Failed to publish {forge} Release for {tag}: {e:#}");
			true
		}
	}
}

enum ReleaseAction {
	Created,
	AlreadyPresent,
	Failed,
}

/// Processes a single package's GitHub Release.
async fn process_one_release(
	code_forge_client: &dyn CodeForgeClient,
	git: &dyn Git,
	config: &Config,
	pkg: &PublishedPackage,
	tag: &str,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<ReleaseAction> {
	let forge = code_forge_client.forge_name();
	match code_forge_client.find_release_by_tag(tag).await {
		Err(e) => {
			error!("Failed to look up {forge} Release for {tag}: {e:#}");
			return Ok(ReleaseAction::Failed);
		}
		Ok(Some(r)) if !r.is_draft => {
			info!("Skipped {forge} Release for {tag} (already exists)");
			return Ok(ReleaseAction::AlreadyPresent);
		}
		Ok(Some(_)) => {
			error!(
				"Found a partial draft {forge} Release for {tag}; cursus will not modify it. \
				 Finalise or delete the draft manually (e.g. via the {forge} UI or \
				 `gh release delete '{tag}'`) and re-run cursus publish."
			);
			return Ok(ReleaseAction::Failed);
		}
		Ok(None) => {}
	}
	let body = read_changelog_body(pkg, fs).await;
	let empty = std::collections::BTreeMap::new();
	let artifacts = config.forge_artifacts().get(&pkg.name).unwrap_or(&empty);
	match code_forge_client.create_release(tag, tag, &body).await {
		Ok(release_id) => {
			let failed = publish_draft_release(
				code_forge_client,
				tag,
				&release_id,
				artifacts,
				git.path(),
				fs,
			)
			.await;
			Ok(if failed {
				ReleaseAction::Failed
			} else {
				ReleaseAction::Created
			})
		}
		Err(e) => {
			error!("Failed to create {forge} Release for {tag}: {e:#}");
			Ok(ReleaseAction::Failed)
		}
	}
}

/// Orchestrates GitHub Release creation for all successfully published packages.
///
/// The caller must ensure that the code forge client is available (i.e. `code_forge_client`
/// is `Ok`) before calling this function (enforced by the early check in `cmd_publish`).
///
/// Returns `(releases_created, releases_already_present, any_failed)`.
pub(super) async fn orchestrate_forge_releases(
	git: &dyn Git,
	config: &Config,
	code_forge_client: &dyn CodeForgeClient,
	published_packages: &[PublishedPackage],
	is_multi_package: bool,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<(usize, usize, bool)> {
	if published_packages.is_empty() {
		return Ok((0, 0, false));
	}
	let mut forge_failed = false;
	let mut created_count = 0;
	let mut already_present_count = 0;
	for pkg in published_packages {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);
		match process_one_release(code_forge_client, git, config, pkg, &tag, fs).await? {
			ReleaseAction::Created => created_count += 1,
			ReleaseAction::AlreadyPresent => already_present_count += 1,
			ReleaseAction::Failed => forge_failed = true,
		}
	}
	Ok((created_count, already_present_count, forge_failed))
}

/// Uploads all configured artifacts to a GitHub release.
///
/// Returns `true` if any upload failed, `false` if all succeeded.
pub(super) async fn upload_release_artifacts(
	code_forge_client: &dyn CodeForgeClient,
	release_id: &str,
	artifacts: &std::collections::BTreeMap<String, String>,
	git_root: &crate::path::AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> bool {
	let mut any_failed = false;
	for (display_name, artifact_path) in artifacts {
		let full_path = match git_root.subpath(artifact_path, fs).await {
			Ok(p) => p,
			Err(e) => {
				warn!("  Skipping '{display_name}': invalid artifact path: {e:#}");
				any_failed = true;
				continue;
			}
		};
		match code_forge_client
			.upload_asset(release_id, display_name, &full_path)
			.await
		{
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
	use crate::cli::publish::tests_common::{
		artifact_map, git_from_dir, make_github_config, recording_runner, workdir,
	};
	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::forge::test_support::{CodeForgeInvocation, RecordingCodeForgeClient};
	use crate::model::config::Config;
	use crate::path::AbsolutePath;

	// --- Tests for orchestrate_forge_releases ---

	#[tokio::test]
	async fn github_release_skipped_when_no_published_packages() {
		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingCodeForgeClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let wd = workdir();
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());

		let (created, _already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&[],
			false,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 0);
		assert!(!failed);
		assert!(client.invocations().is_empty());
	}

	#[tokio::test]
	async fn forge_releases_created_for_published_packages() {
		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingCodeForgeClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let (created, _already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			false,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 1);
		assert!(!failed);
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 3);
		assert!(matches!(
			&invocations[0],
			CodeForgeInvocation::FindReleaseByTag { tag } if tag == "v1.2.0"
		));
		assert!(matches!(
			&invocations[1],
			CodeForgeInvocation::CreateRelease { tag_name, .. }
				if tag_name == "v1.2.0"
		));
		assert!(matches!(
			&invocations[2],
			CodeForgeInvocation::PublishRelease { release_id, .. } if release_id == "release-1"
		));
	}

	#[tokio::test]
	async fn forge_releases_uses_prefixed_tag_for_monorepo() {
		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingCodeForgeClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let (created, _already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			true,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 1);
		assert!(!failed);
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 3);
		assert!(matches!(
			&invocations[0],
			CodeForgeInvocation::FindReleaseByTag { tag } if tag == "my-app@1.2.0"
		));
		assert!(matches!(
			&invocations[1],
			CodeForgeInvocation::CreateRelease { tag_name, .. } if tag_name == "my-app@1.2.0"
		));
		assert!(matches!(
			&invocations[2],
			CodeForgeInvocation::PublishRelease { .. }
		));
	}

	#[tokio::test]
	async fn github_release_create_failure_continues_other_packages() {
		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingCodeForgeClient::new().with_create_failure();
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
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let (created, _already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			true,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 0);
		assert!(failed);
		// Both packages should have been attempted: FindReleaseByTag + CreateRelease each
		assert_eq!(client.invocations().len(), 4);
	}

	#[tokio::test]
	async fn github_release_upload_failure_continues_other_artifacts() {
		let dir = tempfile::tempdir().unwrap();
		let linux_path = dir.path().join("linux.tar.gz");
		let macos_path = dir.path().join("macos.tar.gz");
		std::fs::write(&linux_path, b"linux binary").unwrap();
		std::fs::write(&macos_path, b"macos binary").unwrap();

		let artifacts = artifact_map("my-app", &[("linux", &linux_path), ("macos", &macos_path)]);
		let config = Config::new().with_github(make_github_config("", artifacts));
		let client = RecordingCodeForgeClient::new().with_upload_failure();
		let runner = recording_runner(0);
		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];
		let git = git_from_dir(&runner, dir.path());

		let (created, _already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			false,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		// Draft was created but upload failed — not counted as created (left as draft)
		assert_eq!(created, 0);
		assert!(failed);

		let invocations = client.invocations();
		// Both artifacts were attempted despite first failure
		let uploads: Vec<_> = invocations
			.iter()
			.filter(|i| matches!(i, CodeForgeInvocation::UploadAsset { .. }))
			.collect();
		assert_eq!(uploads.len(), 2);
		// Release must NOT be published when uploads failed (left as draft)
		assert!(
			!invocations
				.iter()
				.any(|i| matches!(i, CodeForgeInvocation::PublishRelease { .. })),
			"PublishRelease should not be called when uploads fail"
		);
	}

	#[tokio::test]
	async fn github_release_artifacts_scoped_to_package() {
		// Artifact configured only for pkg-a; pkg-b has no entry and should get no uploads.
		let dir = tempfile::tempdir().unwrap();
		let artifact_path = dir.path().join("app.tar.gz");
		std::fs::write(&artifact_path, b"binary content").unwrap();

		let config = Config::new().with_github(make_github_config(
			"",
			artifact_map("pkg-a", &[("app", &artifact_path)]),
		));
		let client = RecordingCodeForgeClient::new();
		let runner = recording_runner(0);
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
		let git = git_from_dir(&runner, dir.path());

		let (created, _already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			true,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 2);
		assert!(!failed);

		let invocations = client.invocations();
		let upload_count = invocations
			.iter()
			.filter(|i| matches!(i, CodeForgeInvocation::UploadAsset { .. }))
			.count();
		// Only pkg-a has a configured artifact; pkg-b gets no uploads
		assert_eq!(upload_count, 1);
	}

	#[tokio::test]
	async fn github_release_publish_failure_sets_github_failed() {
		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingCodeForgeClient::new().with_publish_release_failure();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let (created, _already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			false,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		// Draft was created but publish failed — not counted as created
		assert_eq!(created, 0);
		assert!(failed);

		let invocations = client.invocations();
		assert!(matches!(
			&invocations[0],
			CodeForgeInvocation::FindReleaseByTag { .. }
		));
		assert!(matches!(
			&invocations[1],
			CodeForgeInvocation::CreateRelease { .. }
		));
		assert!(matches!(
			&invocations[2],
			CodeForgeInvocation::PublishRelease { .. }
		));
	}

	#[tokio::test]
	async fn github_artifacts_each_release_includes_publish() {
		let dir = tempfile::tempdir().unwrap();
		let artifact_path = dir.path().join("app.tar.gz");
		std::fs::write(&artifact_path, b"binary content").unwrap();

		let config = Config::new().with_github(make_github_config(
			"",
			artifact_map("my-app", &[("app", &artifact_path)]),
		));
		let client = RecordingCodeForgeClient::new();
		let runner = recording_runner(0);
		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];
		let git = git_from_dir(&runner, dir.path());

		let (created, _already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			false,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 1);
		assert!(!failed);

		// Sequence: FindReleaseByTag, CreateRelease, UploadAsset, PublishRelease
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 4);
		assert!(matches!(
			&invocations[0],
			CodeForgeInvocation::FindReleaseByTag { .. }
		));
		assert!(matches!(
			&invocations[1],
			CodeForgeInvocation::CreateRelease { .. }
		));
		assert!(matches!(
			&invocations[2],
			CodeForgeInvocation::UploadAsset { .. }
		));
		assert!(matches!(
			&invocations[3],
			CodeForgeInvocation::PublishRelease { .. }
		));
	}

	// --- Tests for publish_draft_release ---

	#[tokio::test]
	async fn publish_draft_release_success_returns_false() {
		let client = RecordingCodeForgeClient::new();
		let failed = publish_draft_release(
			&client,
			"v1.0.0",
			"release-1",
			&BTreeMap::new(),
			&workdir(),
			&crate::filesystem::LocalFilesystem,
		)
		.await;
		assert!(!failed);
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(matches!(
			&invocations[0],
			CodeForgeInvocation::PublishRelease { release_id, .. } if release_id == "release-1"
		));
	}

	#[tokio::test]
	async fn publish_draft_release_upload_failure_returns_true_no_publish() {
		let dir = tempfile::tempdir().unwrap();
		let artifact_path = dir.path().join("app.tar.gz");
		std::fs::write(&artifact_path, b"data").unwrap();

		let mut artifacts = BTreeMap::new();
		artifacts.insert(
			"app".to_string(),
			artifact_path.to_string_lossy().into_owned(),
		);

		let client = RecordingCodeForgeClient::new().with_upload_failure();
		let dir_abs = AbsolutePath::new(dir.path()).unwrap();
		let failed = publish_draft_release(
			&client,
			"v1.0.0",
			"release-1",
			&artifacts,
			&dir_abs,
			&crate::filesystem::LocalFilesystem,
		)
		.await;
		assert!(failed);
		// Upload was attempted; publish must NOT be called
		let invocations = client.invocations();
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, CodeForgeInvocation::UploadAsset { .. }))
		);
		assert!(
			!invocations
				.iter()
				.any(|i| matches!(i, CodeForgeInvocation::PublishRelease { .. }))
		);
	}

	#[tokio::test]
	async fn publish_draft_release_publish_failure_returns_true() {
		let client = RecordingCodeForgeClient::new().with_publish_release_failure();
		let failed = publish_draft_release(
			&client,
			"v1.0.0",
			"release-1",
			&BTreeMap::new(),
			&workdir(),
			&crate::filesystem::LocalFilesystem,
		)
		.await;
		assert!(failed);
		assert!(matches!(
			&client.invocations()[0],
			CodeForgeInvocation::PublishRelease { .. }
		));
	}

	// --- Tests for read_changelog_body ---

	#[tokio::test]
	async fn read_changelog_body_returns_empty_when_no_changelog() {
		let pkg = PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		};
		assert_eq!(
			read_changelog_body(&pkg, &crate::filesystem::LocalFilesystem).await,
			""
		);
	}

	#[tokio::test]
	async fn read_changelog_body_returns_version_section() {
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
		let body = read_changelog_body(&pkg, &crate::filesystem::LocalFilesystem).await;
		assert!(
			body.contains("Fix a bug"),
			"Expected changelog body, got: {body}"
		);
	}

	#[tokio::test]
	async fn read_changelog_body_returns_empty_when_version_missing() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("CHANGELOG.md"), "## 0.9.0\n\nOld release\n").unwrap();
		let pkg = PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new(dir.path()).unwrap(),
		};
		// Version not found returns empty string
		assert_eq!(
			read_changelog_body(&pkg, &crate::filesystem::LocalFilesystem).await,
			""
		);
	}

	#[tokio::test]
	async fn upload_release_artifacts_rejects_path_traversal() {
		let outer = tempfile::tempdir().unwrap();
		let inner = outer.path().join("repo");
		std::fs::create_dir(&inner).unwrap();
		let secret = outer.path().join("secret.txt");
		std::fs::write(&secret, b"sensitive").unwrap();

		let mut artifacts = BTreeMap::new();
		artifacts.insert("secret".to_string(), secret.to_string_lossy().into_owned());

		let client = RecordingCodeForgeClient::new();
		let git_root = AbsolutePath::new(&inner).unwrap();

		let failed = upload_release_artifacts(
			&client,
			"release-1",
			&artifacts,
			&git_root,
			&crate::filesystem::LocalFilesystem,
		)
		.await;

		assert!(failed, "Expected failure for path traversal");
		// No upload should have been attempted
		assert!(
			client.invocations().is_empty(),
			"Upload should not be called for invalid path"
		);
	}

	// --- Tests for log_dry_run_forge_releases ---

	#[tokio::test]
	async fn log_dry_run_forge_releases_emits_would_publish_always() {
		use crate::test_logging::{init_test_logger, take_logs};
		init_test_logger();
		let _ = take_logs();

		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		log_dry_run_forge_releases(&packages, &config, false, "GitHub");

		let logs = take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("Would publish release after artifact upload")),
			"Expected 'Would publish release after artifact upload' even without artifacts, got: {logs:?}"
		);
	}

	#[tokio::test]
	async fn log_dry_run_forge_releases_emits_would_attach_for_artifacts() {
		use crate::test_logging::{init_test_logger, take_logs};
		init_test_logger();
		let _ = take_logs();

		let mut pkg_artifacts = BTreeMap::new();
		pkg_artifacts.insert("linux-amd64".to_string(), "target/app".to_string());
		let mut artifacts = BTreeMap::new();
		artifacts.insert("my-app".to_string(), pkg_artifacts);
		let config = Config::new().with_github(make_github_config("", artifacts));
		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		log_dry_run_forge_releases(&packages, &config, false, "GitHub");

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

	// --- Tests for GitHub Release idempotency ---

	#[tokio::test]
	async fn orchestrate_forge_releases_skips_when_release_already_published() {
		use crate::forge::ExistingRelease;

		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let existing = ExistingRelease {
			id: "existing-1".to_string(),
			is_draft: false,
		};
		let client = RecordingCodeForgeClient::new().with_existing_release("v1.0.0", existing);
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let (created, already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			false,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 0);
		assert_eq!(already_present, 1);
		assert!(!failed);

		let invocations = client.invocations();
		assert!(
			invocations.iter().any(
				|i| matches!(i, CodeForgeInvocation::FindReleaseByTag { tag } if tag == "v1.0.0")
			),
			"Expected FindReleaseByTag invocation"
		);
		assert!(
			!invocations
				.iter()
				.any(|i| matches!(i, CodeForgeInvocation::CreateRelease { .. })),
			"CreateRelease must NOT be called when release already exists"
		);
		assert!(
			!invocations
				.iter()
				.any(|i| matches!(i, CodeForgeInvocation::PublishRelease { .. })),
			"PublishRelease must NOT be called when release already exists"
		);
	}

	#[tokio::test]
	async fn orchestrate_forge_releases_aborts_on_existing_draft() {
		use crate::forge::ExistingRelease;
		use crate::test_logging::{init_test_logger, take_logs};
		init_test_logger();
		let _ = take_logs();

		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingCodeForgeClient::new().with_existing_release(
			"v1.0.0",
			ExistingRelease {
				id: "draft-1".to_string(),
				is_draft: true,
			},
		);
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, workdir());
		let (created, already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			false,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 0);
		assert_eq!(already_present, 0);
		assert!(failed);
		// Only FindReleaseByTag must have been called — no mutating API calls.
		let invocations = client.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(matches!(
			&invocations[0],
			CodeForgeInvocation::FindReleaseByTag { .. }
		));
		let logs = take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("v1.0.0") && m.contains("draft")),
			"Expected error mentioning the tag and 'draft': {logs:?}"
		);
	}

	#[tokio::test]
	async fn orchestrate_forge_releases_finds_release_before_creating() {
		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingCodeForgeClient::new();
		let runner = Arc::new(RecordingCommandRunner::new(0));

		let packages = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "2.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let wd = workdir();
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, wd.clone());
		let (created, already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			false,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 1);
		assert_eq!(already_present, 0);
		assert!(!failed);

		let invocations = client.invocations();
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, CodeForgeInvocation::FindReleaseByTag { .. })),
			"FindReleaseByTag must be called before CreateRelease"
		);
		// FindReleaseByTag must come before CreateRelease in the invocation list
		let find_pos = invocations
			.iter()
			.position(|i| matches!(i, CodeForgeInvocation::FindReleaseByTag { .. }))
			.unwrap();
		let create_pos = invocations
			.iter()
			.position(|i| matches!(i, CodeForgeInvocation::CreateRelease { .. }))
			.unwrap();
		assert!(
			find_pos < create_pos,
			"FindReleaseByTag must precede CreateRelease"
		);
	}

	#[tokio::test]
	async fn orchestrate_forge_releases_find_failure_marks_failed_continues_next_package() {
		// A find_release_by_tag error on one package must not abort the loop.
		let config = Config::new().with_github(make_github_config("", BTreeMap::new()));
		let client = RecordingCodeForgeClient::new().with_find_release_failure();
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
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, workdir());
		let (created, already_present, failed) = orchestrate_forge_releases(
			&git,
			&config,
			&client,
			&packages,
			true,
			&crate::filesystem::LocalFilesystem,
		)
		.await
		.unwrap();

		assert_eq!(created, 0);
		assert_eq!(already_present, 0);
		assert!(failed);
		// Both packages should have had FindReleaseByTag attempted.
		let find_count = client
			.invocations()
			.iter()
			.filter(|i| matches!(i, CodeForgeInvocation::FindReleaseByTag { .. }))
			.count();
		assert_eq!(
			find_count, 2,
			"Both packages should be attempted despite first failure"
		);
	}
}
