//! Forge release creation, artifact upload, and build command orchestration.
//!
//! Forge-neutral: dispatches to whichever [`crate::forge::CodeForgeClient`]
//! the binary boundary provided (GitHub or GitLab). User-facing log lines
//! pick up the active forge's vocabulary via [`CodeForgeClient::forge_name`].

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

/// Processes a single package's forge release on the active forge.
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

/// Orchestrates forge release creation for all successfully published packages.
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

/// Uploads all configured artifacts to a forge release on the active forge.
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
