//! Production GitLab API client using the Kitware `gitlab` crate.
//!
//! Wraps a [`gitlab::AsyncGitlab`] to implement the
//! [`crate::forge::CodeForgeClient`] trait. Authentication strategy and
//! base-URL resolution are decided by the binary boundary; this module
//! receives a fully constructed `AsyncGitlab` plus a [`GitLabProject`]
//! identity.

use std::path::Path;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use gitlab::api::projects::merge_requests::{CreateMergeRequest, EditMergeRequest, MergeRequests};
use gitlab::api::projects::packages::generic::UploadPackageFile;
use gitlab::api::projects::releases::links::{CreateReleaseLink, LinkType};
use gitlab::api::projects::releases::{CreateRelease, ProjectReleaseByTag};
use gitlab::api::{ApiError, AsyncQuery};
use gitlab::{AsyncGitlab, GitlabBuilder};
use log::info;
use serde::Deserialize;

use super::remote::GitLabProject;
use crate::forge::{CodeForgeClient, ExistingRelease, PullRequest};
use crate::redact::redact_credentials;

/// The constant Generic Package Registry "package" used to host release
/// artifacts. All asset uploads for the same release share this key,
/// versioned by the release tag.
///
/// GitLab's Generic Package Registry requires the package_name to match
/// `[A-Za-z0-9._-]+`; this constant complies.
const RELEASE_ASSETS_PACKAGE: &str = "release-assets";

/// GitLab API client backed by the Kitware `gitlab` crate's `AsyncGitlab`.
///
/// Accepts a pre-configured [`AsyncGitlab`] instance and a resolved
/// [`GitLabProject`] at construction (ADR-042). The library does not handle
/// authentication — consumers provide a fully configured client (e.g. with a
/// personal access token or `CI_JOB_TOKEN`).
pub struct ReqwestGitLabClient {
	client: AsyncGitlab,
	project: GitLabProject,
}

impl std::fmt::Debug for ReqwestGitLabClient {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ReqwestGitLabClient")
			.field("project", &self.project)
			.finish_non_exhaustive()
	}
}

/// Source of the GitLab auth token, used to pick between PAT (full scope)
/// and CI job token (read-only on Merge Requests) builders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitLabTokenKind {
	/// A user-, group-, or project-access personal token with `api` scope.
	PersonalAccessToken,
	/// A GitLab CI `CI_JOB_TOKEN`. Cannot create or update merge requests.
	JobToken,
}

impl ReqwestGitLabClient {
	/// Creates a new client wrapping the given `AsyncGitlab` and project identity.
	pub fn new(client: AsyncGitlab, project: GitLabProject) -> Self {
		Self { client, project }
	}

	/// Constructs a [`ReqwestGitLabClient`] from raw connection parameters.
	///
	/// `host` is a bare hostname (no scheme). `token` is the GitLab auth
	/// token; `token_kind` selects between PAT and `CI_JOB_TOKEN` semantics.
	///
	/// # Errors
	///
	/// Returns an error if the underlying `gitlab::GitlabBuilder` cannot
	/// initialise the HTTP client (e.g. invalid host).
	pub async fn build(
		host: &str,
		token: &str,
		token_kind: GitLabTokenKind,
		project: GitLabProject,
	) -> anyhow::Result<Self> {
		let builder = match token_kind {
			GitLabTokenKind::PersonalAccessToken => GitlabBuilder::new(host, token),
			GitLabTokenKind::JobToken => GitlabBuilder::new_with_job_token(host, token),
		};
		let async_client = builder
			.build_async()
			.await
			.with_context(|| format!("Failed to initialise GitLab client for host '{host}'"))?;
		Ok(Self::new(async_client, project))
	}

	/// Returns the `group/project` path used as the GitLab project identifier.
	fn project_path(&self) -> String {
		format!("{}/{}", self.project.group, self.project.project)
	}

	/// Composes the public Generic Package Registry download URL for a single asset.
	///
	/// Delegates to the free [`compose_package_file_url`] so the URL template
	/// can be unit-tested directly without standing up a mock HTTP server.
	fn package_file_url(&self, version: &str, file_name: &str) -> String {
		compose_package_file_url(&self.project, version, file_name)
	}
}

/// Minimal merge-request response shape used to extract the user-facing URL
/// from create/update calls. Only the fields cursus needs are deserialised.
#[derive(Debug, Deserialize)]
struct MergeRequestResponse {
	iid: u64,
	web_url: String,
}

#[async_trait]
impl CodeForgeClient for ReqwestGitLabClient {
	fn forge_name(&self) -> &'static str {
		"GitLab"
	}

	async fn create_release(
		&self,
		tag_name: &str,
		name: &str,
		body: &str,
	) -> anyhow::Result<String> {
		log::trace!("create_release: tag={tag_name} name={name}");
		let endpoint = CreateRelease::builder()
			.project(self.project_path())
			.tag_name(tag_name)
			.name(name)
			.description(body)
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.with_context(|| {
				format!("Failed to build create-release request for tag '{tag_name}'")
			})?;
		gitlab::api::ignore(endpoint)
			.query_async(&self.client)
			.await
			.map_err(redact_api_error)
			.with_context(|| format!("Failed to create GitLab release for tag '{tag_name}'"))?;
		info!("Created GitLab release for {tag_name}");
		// Releases on GitLab are identified by tag; we use it as the opaque
		// release_id so subsequent upload_asset/publish_release calls have
		// the context they need without a second lookup.
		Ok(tag_name.to_string())
	}

	async fn upload_asset(
		&self,
		release_id: &str,
		file_name: &str,
		file_path: &Path,
	) -> anyhow::Result<()> {
		log::trace!("upload_asset: release_id={release_id} file={file_name}");
		let safe_version = sanitize_package_version(release_id);
		let safe_file_name = sanitize_file_name(file_name);
		let data = tokio::fs::read(file_path)
			.await
			.with_context(|| format!("Failed to read asset file '{}'", file_path.display()))?;

		let upload = UploadPackageFile::builder()
			.project(self.project_path())
			.package_name(RELEASE_ASSETS_PACKAGE)
			.package_version(safe_version.clone())
			.file_name(safe_file_name.clone())
			.contents(data.as_slice())
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.with_context(|| {
				format!("Failed to build generic-package upload request for '{file_name}'")
			})?;
		gitlab::api::ignore(upload)
			.query_async(&self.client)
			.await
			.map_err(redact_api_error)
			.with_context(|| {
				format!("Failed to upload '{file_name}' to the GitLab Generic Package Registry")
			})?;

		let asset_url = self.package_file_url(&safe_version, &safe_file_name);
		let link = CreateReleaseLink::builder()
			.project(self.project_path())
			.tag_name(release_id)
			.name(file_name)
			.url(asset_url)
			.link_type(LinkType::Package)
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.with_context(|| format!("Failed to build release-link request for '{file_name}'"))?;
		gitlab::api::ignore(link)
			.query_async(&self.client)
			.await
			.map_err(redact_api_error)
			.with_context(|| {
				format!("Failed to attach '{file_name}' to GitLab release '{release_id}'")
			})?;
		Ok(())
	}

	async fn create_pull_request(
		&self,
		title: &str,
		body: &str,
		head: &str,
		base: &str,
	) -> anyhow::Result<String> {
		log::trace!("create_pull_request: title={title} head={head} base={base}");
		let endpoint = CreateMergeRequest::builder()
			.project(self.project_path())
			.source_branch(head)
			.target_branch(base)
			.title(title)
			.description(body)
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.with_context(|| format!("Failed to build create-merge-request for '{title}'"))?;
		let mr: MergeRequestResponse = endpoint
			.query_async(&self.client)
			.await
			.map_err(redact_api_error)
			.with_context(|| format!("Failed to create merge request '{title}'"))?;
		info!("Created merge request: {}", mr.web_url);
		Ok(mr.web_url)
	}

	async fn find_open_pull_request(&self, head: &str) -> anyhow::Result<Option<PullRequest>> {
		log::trace!("find_open_pull_request: head={head}");
		let endpoint = MergeRequests::builder()
			.project(self.project_path())
			.source_branch(head)
			.state(gitlab::api::projects::merge_requests::MergeRequestState::Opened)
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.with_context(|| format!("Failed to build list-merge-requests for branch '{head}'"))?;
		let mrs: Vec<MergeRequestResponse> = endpoint
			.query_async(&self.client)
			.await
			.map_err(redact_api_error)
			.with_context(|| format!("Failed to list merge requests for branch '{head}'"))?;
		// GitLab's MR `iid` is per-project, matching what the trait expects;
		// `state=opened` is enforced server-side, so we just take the first hit.
		Ok(mrs.into_iter().next().map(|mr| PullRequest {
			number: mr.iid,
			html_url: mr.web_url,
		}))
	}

	async fn update_pull_request(
		&self,
		pull_number: u64,
		title: &str,
		body: &str,
	) -> anyhow::Result<String> {
		log::trace!("update_pull_request: !{pull_number} title={title}");
		let endpoint = EditMergeRequest::builder()
			.project(self.project_path())
			.merge_request(pull_number)
			.title(title)
			.description(body)
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.with_context(|| format!("Failed to build update-merge-request !{pull_number}"))?;
		let mr: MergeRequestResponse = endpoint
			.query_async(&self.client)
			.await
			.map_err(redact_api_error)
			.with_context(|| format!("Failed to update merge request !{pull_number}"))?;
		info!("Updated merge request: {}", mr.web_url);
		Ok(mr.web_url)
	}

	async fn find_release_by_tag(&self, tag: &str) -> anyhow::Result<Option<ExistingRelease>> {
		log::trace!("find_release_by_tag: tag={tag}");
		let endpoint = ProjectReleaseByTag::builder()
			.project(self.project_path())
			.tag(tag)
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.with_context(|| format!("Failed to build release lookup for tag '{tag}'"))?;
		match gitlab::api::ignore(endpoint)
			.query_async(&self.client)
			.await
		{
			Ok(()) => Ok(Some(ExistingRelease {
				id: tag.to_string(),
				// GitLab releases have no draft state — once created they are
				// immediately visible, per ADR-056.
				is_draft: false,
			})),
			Err(e) if is_not_found(&e) => Ok(None),
			Err(e) => Err(redact_api_error(e))
				.with_context(|| format!("Failed to look up GitLab release for tag '{tag}'")),
		}
	}

	async fn publish_release(&self, release_id: &str) -> anyhow::Result<()> {
		// GitLab has no draft → published transition; releases are visible
		// the moment `create_release` succeeds. This no-op exists so the
		// trait shape stays uniform across forges (per ADR-056).
		log::trace!("publish_release: noop for GitLab release {release_id}");
		Ok(())
	}
}

/// Converts a `gitlab::api::ApiError` into an [`anyhow::Error`] after running
/// its `Display` output through [`redact_credentials`]. Used at every
/// `query_async` call site so a future upstream change that surfaces a
/// credential-bearing URL (proxy URL, redirect target, etc.) inside an
/// `ApiError` cannot leak that URL into user-visible logs or wrapped error
/// chains.
pub(crate) fn redact_api_error<T>(err: ApiError<T>) -> anyhow::Error
where
	T: std::error::Error + Send + Sync + 'static,
{
	let raw = format!("{err}");
	anyhow!("{}", redact_credentials(&raw).into_owned())
}

/// Detects whether a `gitlab::api::ApiError` corresponds to an HTTP 404 response.
pub(crate) fn is_not_found<E>(err: &ApiError<E>) -> bool
where
	E: std::error::Error + Send + Sync + 'static,
{
	match err {
		ApiError::GitlabWithStatus { status, .. }
		| ApiError::GitlabObjectWithStatus { status, .. }
		| ApiError::GitlabUnrecognizedWithStatus { status, .. } => status.as_u16() == 404,
		_ => false,
	}
}

/// Composes the public Generic Package Registry download URL for a single asset.
///
/// GitLab serves uploaded files at the same path the upload used, so the URL
/// is fully determined by `(scheme, host, project, package_name, version,
/// file_name)`. The scheme is read from the project identity so HTTP-only
/// self-managed instances and HTTPS upstream gitlab.com both produce
/// reachable links.
pub(crate) fn compose_package_file_url(
	project: &GitLabProject,
	version: &str,
	file_name: &str,
) -> String {
	format!(
		"{scheme}://{host}/api/v4/projects/{path}/packages/generic/{pkg}/{ver}/{file}",
		scheme = project.scheme,
		host = project.host,
		path = percent_encode_path(&format!("{}/{}", project.group, project.project)),
		pkg = RELEASE_ASSETS_PACKAGE,
		ver = percent_encode_path(version),
		file = percent_encode_path(file_name),
	)
}

/// Replaces characters disallowed by GitLab's package_version regex
/// (`\A(\.?[\w\+-]+\.?)+\z`) with `-`. Allowed characters: word characters,
/// `+`, `-`, and `.`.
pub(crate) fn sanitize_package_version(s: &str) -> String {
	s.chars()
		.map(|c| {
			if c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '-' || c == '.' {
				c
			} else {
				'-'
			}
		})
		.collect()
}

/// Replaces characters disallowed by GitLab's generic package file_name rule
/// (alphanumerics plus `.`, `-`, `_`) with `-`.
pub(crate) fn sanitize_file_name(s: &str) -> String {
	s.chars()
		.map(|c| {
			if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
				c
			} else {
				'-'
			}
		})
		.collect()
}

/// Percent-encodes a value for safe interpolation into a single URL path segment.
///
/// The `/` separator inside `group/project` paths is preserved by encoding to `%2F`.
pub(crate) fn percent_encode_path(s: &str) -> String {
	percent_encoding::utf8_percent_encode(s, PATH_SET).to_string()
}

const PATH_SET: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
	.add(b' ')
	.add(b'"')
	.add(b'#')
	.add(b'<')
	.add(b'>')
	.add(b'?')
	.add(b'`')
	.add(b'{')
	.add(b'}')
	.add(b'/');
