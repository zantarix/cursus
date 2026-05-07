//! Production GitHub API client using octocrab.
//!
//! Wraps an [`octocrab::Octocrab`] instance to implement the [`CodeForgeClient`]
//! trait. The `Octocrab` is constructed and configured by the consumer (binary
//! or bot), so authentication strategy stays outside the library.

use std::path::Path;

use anyhow::Context;
use async_trait::async_trait;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use super::client::{CodeForgeClient, ExistingRelease, PullRequest};

// Encodes characters unsafe in a URL path segment used as a single token (e.g., a git tag in
// the GitHub Releases API path `/releases/tags/{tag}`). Unlike branch refs, `/` must be encoded
// here because the tag occupies a single path segment — an unencoded `/` would corrupt the route.
const TAG_PATH: &AsciiSet = &CONTROLS.add(b' ').add(b'?').add(b'#').add(b'%').add(b'/');
use super::remote::GitHubRepo;

/// GitHub API client backed by octocrab.
///
/// Accepts a pre-configured [`octocrab::Octocrab`] instance and a resolved
/// [`GitHubRepo`] at construction (ADR-042). The library does not handle
/// authentication — consumers provide a fully configured client (e.g. with a
/// personal access token or GitHub App token).
pub struct OctocrabGitHubClient {
	client: octocrab::Octocrab,
	gh_repo: GitHubRepo,
}

impl std::fmt::Debug for OctocrabGitHubClient {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("OctocrabGitHubClient")
			.field("gh_repo", &self.gh_repo)
			.finish_non_exhaustive()
	}
}

impl OctocrabGitHubClient {
	/// Creates a new client wrapping the given octocrab instance and repository identity.
	pub fn new(client: octocrab::Octocrab, gh_repo: GitHubRepo) -> Self {
		Self { client, gh_repo }
	}
}

#[async_trait]
impl CodeForgeClient for OctocrabGitHubClient {
	async fn create_release(
		&self,
		tag_name: &str,
		name: &str,
		body: &str,
	) -> anyhow::Result<String> {
		log::trace!("create_release: tag={tag_name} name={name}");
		let release = self
			.client
			.repos(&self.gh_repo.owner, &self.gh_repo.repo)
			.releases()
			.create(tag_name)
			.name(name)
			.body(body)
			.draft(true)
			.prerelease(false)
			.send()
			.await
			.with_context(|| format!("Failed to create GitHub Release for tag '{tag_name}'"))?;
		Ok(release.id.to_string())
	}

	async fn upload_asset(
		&self,
		release_id: &str,
		file_name: &str,
		file_path: &Path,
	) -> anyhow::Result<()> {
		let id: u64 = release_id
			.parse()
			.with_context(|| format!("Invalid GitHub release_id: {release_id:?}"))?;

		log::trace!("upload_asset: release_id={release_id} file={file_name}");
		let data = tokio::fs::read(file_path)
			.await
			.with_context(|| format!("Failed to read asset file '{}'", file_path.display()))?;

		self.client
			.repos(&self.gh_repo.owner, &self.gh_repo.repo)
			.releases()
			.upload_asset(id, file_name, data.into())
			.send()
			.await
			.with_context(|| format!("Failed to upload asset '{file_name}'"))?;
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
		let pr = self
			.client
			.pulls(&self.gh_repo.owner, &self.gh_repo.repo)
			.create(title, head, base)
			.body(body)
			.send()
			.await
			.with_context(|| format!("Failed to create pull request '{title}'"))?;
		Ok(pr.html_url.to_string())
	}

	async fn find_open_pull_request(&self, head: &str) -> anyhow::Result<Option<PullRequest>> {
		let head_filter = format!("{}:{}", self.gh_repo.owner, head);
		log::trace!("find_open_pull_request: head={head_filter}");
		let page = self
			.client
			.pulls(&self.gh_repo.owner, &self.gh_repo.repo)
			.list()
			.state(octocrab::params::State::Open)
			.head(&head_filter)
			.send()
			.await
			.with_context(|| format!("Failed to list pull requests for branch '{head}'"))?;
		Ok(page.items.into_iter().next().map(|pr| {
			let html_url = pr.html_url.to_string();
			PullRequest {
				number: pr.number,
				html_url,
			}
		}))
	}

	async fn update_pull_request(
		&self,
		pull_number: u64,
		title: &str,
		body: &str,
	) -> anyhow::Result<String> {
		log::trace!("update_pull_request: #{pull_number} title={title}");
		let pr = self
			.client
			.pulls(&self.gh_repo.owner, &self.gh_repo.repo)
			.update(pull_number)
			.title(title)
			.body(body)
			.send()
			.await
			.with_context(|| format!("Failed to update pull request #{pull_number}"))?;
		Ok(pr.html_url.to_string())
	}

	async fn find_release_by_tag(&self, tag: &str) -> anyhow::Result<Option<ExistingRelease>> {
		let encoded_tag = utf8_percent_encode(tag, TAG_PATH).to_string();
		log::trace!("find_release_by_tag: tag={tag} encoded={encoded_tag}");
		match self
			.client
			.repos(&self.gh_repo.owner, &self.gh_repo.repo)
			.releases()
			.get_by_tag(&encoded_tag)
			.await
		{
			Ok(release) => Ok(Some(ExistingRelease {
				id: release.id.to_string(),
				is_draft: release.draft,
			})),
			Err(octocrab::Error::GitHub { ref source, .. })
				if source.status_code.as_u16() == 404 =>
			{
				Ok(None)
			}
			Err(e) => {
				Err(e).with_context(|| format!("Failed to look up GitHub Release for tag '{tag}'"))
			}
		}
	}

	async fn publish_release(&self, release_id: &str) -> anyhow::Result<()> {
		let id: u64 = release_id
			.parse()
			.with_context(|| format!("Invalid GitHub release_id: {release_id:?}"))?;
		log::trace!("publish_release: release_id={release_id}");
		self.client
			.repos(&self.gh_repo.owner, &self.gh_repo.repo)
			.releases()
			.update(id)
			.draft(false)
			.send()
			.await
			.with_context(|| format!("Failed to publish GitHub Release {release_id}"))?;
		Ok(())
	}
}
