//! Abstract code forge client trait and test support.

use std::path::Path;

use async_trait::async_trait;

/// A resolved pull request.
#[derive(Debug, Clone)]
pub struct PullRequest {
	/// Pull request number.
	pub number: u64,
	/// Full URL of the pull request.
	pub html_url: String,
}

/// An existing release found on the active forge by tag lookup.
#[derive(Debug, Clone)]
pub struct ExistingRelease {
	/// Numeric release ID as a string.
	pub id: String,
	/// Whether the release is currently a draft (unpublished).
	pub is_draft: bool,
}

/// Abstract interface for code forge API operations.
///
/// Each implementation stores its repository identity at construction time
/// (ADR-042), so methods do not accept a repo parameter. All methods are
/// async. The production implementation uses octocrab. The `test_support`
/// module provides a recording fake for unit tests.
#[async_trait]
pub trait CodeForgeClient: Send + Sync + std::fmt::Debug {
	/// Returns the human-readable forge name for use in user-visible messages.
	///
	/// Implementations return the active forge's native label
	/// (e.g. `"GitHub"`, `"GitLab"`). The orchestration layer composes log
	/// strings and error messages using this label so it does not need to
	/// know which concrete client is active.
	fn forge_name(&self) -> &'static str;

	/// Creates a release for the given tag, returning the release ID.
	///
	/// The release is created as a draft. Call [`publish_release`](Self::publish_release)
	/// after uploading all artifacts to make it public.
	///
	/// # Errors
	///
	/// Returns an error if the API call fails or authentication is missing.
	async fn create_release(
		&self,
		tag_name: &str,
		name: &str,
		body: &str,
	) -> anyhow::Result<String>;

	/// Uploads a file as an asset to an existing release.
	///
	/// # Errors
	///
	/// Returns an error if the upload fails.
	async fn upload_asset(
		&self,
		release_id: &str,
		file_name: &str,
		file_path: &Path,
	) -> anyhow::Result<()>;

	/// Creates a pull request and returns the PR URL.
	///
	/// # Arguments
	///
	/// * `title` - Pull request title.
	/// * `body` - Pull request description (markdown).
	/// * `head` - Source branch (the branch to merge from).
	/// * `base` - Target branch (the branch to merge into).
	///
	/// # Errors
	///
	/// Returns an error if the API call fails.
	async fn create_pull_request(
		&self,
		title: &str,
		body: &str,
		head: &str,
		base: &str,
	) -> anyhow::Result<String>;

	/// Finds an open pull request whose head branch matches `head`.
	///
	/// Returns `None` if no open PR exists for that branch.
	///
	/// # Errors
	///
	/// Returns an error if the API call fails.
	async fn find_open_pull_request(&self, head: &str) -> anyhow::Result<Option<PullRequest>>;

	/// Updates the title and body of an existing pull request, returning the PR URL.
	///
	/// # Errors
	///
	/// Returns an error if the API call fails.
	async fn update_pull_request(
		&self,
		pull_number: u64,
		title: &str,
		body: &str,
	) -> anyhow::Result<String>;

	/// Looks up an existing release by its git tag.
	///
	/// Returns `Ok(None)` if no release exists for the given tag (HTTP 404).
	///
	/// # Errors
	///
	/// Returns an error if the API call fails for reasons other than "not found".
	async fn find_release_by_tag(&self, tag: &str) -> anyhow::Result<Option<ExistingRelease>>;

	/// Transitions a draft release to published.
	///
	/// # Errors
	///
	/// Returns an error if the API call fails or `release_id` is not numeric.
	async fn publish_release(&self, release_id: &str) -> anyhow::Result<()>;
}

/// Test support types for code-forge client operations.
///
/// Provides a fake client implementation for use in unit and integration tests.
/// Available when compiled with `#[cfg(test)]` (unit tests within this crate)
/// or with the `test-support` feature (external consumers such as integration
/// test crates).
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
	use std::path::Path;
	use std::sync::Mutex;

	use anyhow::bail;
	use async_trait::async_trait;

	use super::{CodeForgeClient, ExistingRelease, PullRequest};

	/// A recorded code forge API invocation.
	#[derive(Debug, Clone)]
	pub enum CodeForgeInvocation {
		/// A `create_release` call.
		CreateRelease {
			/// Git tag name for the release.
			tag_name: String,
			/// Release title.
			name: String,
			/// Release body (markdown).
			body: String,
		},
		/// An `upload_asset` call.
		UploadAsset {
			/// ID of the release to attach the asset to.
			release_id: String,
			/// Asset file name as it will appear in the release.
			file_name: String,
			/// Local path of the file to upload.
			file_path: std::path::PathBuf,
		},
		/// A `create_pull_request` call.
		CreatePullRequest {
			/// Pull request title.
			title: String,
			/// Pull request body (markdown).
			body: String,
			/// Source branch (head).
			head: String,
			/// Target branch (base).
			base: String,
		},
		/// A `find_open_pull_request` call.
		FindOpenPullRequest {
			/// Head branch to search for.
			head: String,
		},
		/// An `update_pull_request` call.
		UpdatePullRequest {
			/// Pull request number.
			pull_number: u64,
			/// New pull request title.
			title: String,
			/// New pull request body (markdown).
			body: String,
		},
		/// A `publish_release` call.
		PublishRelease {
			/// ID of the draft release to publish.
			release_id: String,
		},
		/// A `find_release_by_tag` call.
		FindReleaseByTag {
			/// Git tag to look up.
			tag: String,
		},
	}

	/// A [`CodeForgeClient`] that records all invocations and returns configured responses.
	#[derive(Debug)]
	pub struct RecordingCodeForgeClient {
		invocations: Mutex<Vec<CodeForgeInvocation>>,
		release_id: String,
		fail_create: bool,
		fail_upload: bool,
		fail_create_pr: bool,
		existing_pr: Option<PullRequest>,
		fail_find_pr: bool,
		fail_update_pr: bool,
		fail_publish_release: bool,
		existing_releases: std::collections::HashMap<String, ExistingRelease>,
		fail_find_release: bool,
		forge_name: &'static str,
	}

	impl RecordingCodeForgeClient {
		/// Creates a new recording client that succeeds with `release_id = "release-1"`.
		///
		/// Defaults `forge_name` to `"GitHub"`. Use
		/// [`with_forge_name`](Self::with_forge_name) when a test needs to assert
		/// on GitLab-specific orchestrator output.
		pub fn new() -> Self {
			Self {
				invocations: Mutex::new(Vec::new()),
				release_id: "release-1".to_string(),
				fail_create: false,
				fail_upload: false,
				fail_create_pr: false,
				existing_pr: None,
				fail_find_pr: false,
				fail_update_pr: false,
				fail_publish_release: false,
				existing_releases: std::collections::HashMap::new(),
				fail_find_release: false,
				forge_name: "GitHub",
			}
		}

		/// Overrides the [`CodeForgeClient::forge_name`] return value (defaults to `"GitHub"`).
		pub fn with_forge_name(mut self, name: &'static str) -> Self {
			self.forge_name = name;
			self
		}

		/// Configures the release ID returned by [`create_release`](CodeForgeClient::create_release).
		pub fn with_release_id(mut self, id: impl Into<String>) -> Self {
			self.release_id = id.into();
			self
		}

		/// Causes [`create_release`](CodeForgeClient::create_release) to return an error.
		pub fn with_create_failure(mut self) -> Self {
			self.fail_create = true;
			self
		}

		/// Causes [`upload_asset`](CodeForgeClient::upload_asset) to return an error.
		pub fn with_upload_failure(mut self) -> Self {
			self.fail_upload = true;
			self
		}

		/// Causes [`create_pull_request`](CodeForgeClient::create_pull_request) to return an error.
		pub fn with_create_pr_failure(mut self) -> Self {
			self.fail_create_pr = true;
			self
		}

		/// Configures an existing PR to be returned by
		/// [`find_open_pull_request`](CodeForgeClient::find_open_pull_request).
		pub fn with_existing_pr(mut self, pr: PullRequest) -> Self {
			self.existing_pr = Some(pr);
			self
		}

		/// Causes [`find_open_pull_request`](CodeForgeClient::find_open_pull_request) to return an
		/// error.
		pub fn with_find_pr_failure(mut self) -> Self {
			self.fail_find_pr = true;
			self
		}

		/// Causes [`update_pull_request`](CodeForgeClient::update_pull_request) to return an error.
		pub fn with_update_pr_failure(mut self) -> Self {
			self.fail_update_pr = true;
			self
		}

		/// Causes [`publish_release`](CodeForgeClient::publish_release) to return an error.
		pub fn with_publish_release_failure(mut self) -> Self {
			self.fail_publish_release = true;
			self
		}

		/// Configures a release returned by
		/// [`find_release_by_tag`](CodeForgeClient::find_release_by_tag) for the given tag.
		pub fn with_existing_release(
			mut self,
			tag: impl Into<String>,
			release: ExistingRelease,
		) -> Self {
			self.existing_releases.insert(tag.into(), release);
			self
		}

		/// Causes [`find_release_by_tag`](CodeForgeClient::find_release_by_tag) to return an error.
		pub fn with_find_release_failure(mut self) -> Self {
			self.fail_find_release = true;
			self
		}

		/// Returns all invocations recorded so far.
		pub fn invocations(&self) -> Vec<CodeForgeInvocation> {
			self.invocations.lock().expect("mutex poisoned").clone()
		}
	}

	impl Default for RecordingCodeForgeClient {
		fn default() -> Self {
			Self::new()
		}
	}

	#[async_trait]
	impl CodeForgeClient for RecordingCodeForgeClient {
		fn forge_name(&self) -> &'static str {
			self.forge_name
		}

		async fn create_release(
			&self,
			tag_name: &str,
			name: &str,
			body: &str,
		) -> anyhow::Result<String> {
			self.invocations.lock().expect("mutex poisoned").push(
				CodeForgeInvocation::CreateRelease {
					tag_name: tag_name.to_string(),
					name: name.to_string(),
					body: body.to_string(),
				},
			);
			if self.fail_create {
				bail!("simulated create_release failure");
			}
			Ok(self.release_id.clone())
		}

		async fn upload_asset(
			&self,
			release_id: &str,
			file_name: &str,
			file_path: &Path,
		) -> anyhow::Result<()> {
			self.invocations.lock().expect("mutex poisoned").push(
				CodeForgeInvocation::UploadAsset {
					release_id: release_id.to_string(),
					file_name: file_name.to_string(),
					file_path: file_path.to_path_buf(),
				},
			);
			if self.fail_upload {
				bail!("simulated upload_asset failure");
			}
			Ok(())
		}

		async fn create_pull_request(
			&self,
			title: &str,
			body: &str,
			head: &str,
			base: &str,
		) -> anyhow::Result<String> {
			self.invocations.lock().expect("mutex poisoned").push(
				CodeForgeInvocation::CreatePullRequest {
					title: title.to_string(),
					body: body.to_string(),
					head: head.to_string(),
					base: base.to_string(),
				},
			);
			if self.fail_create_pr {
				bail!("simulated create_pull_request failure");
			}
			Ok("https://example.com/pull/1".to_string())
		}

		async fn find_open_pull_request(&self, head: &str) -> anyhow::Result<Option<PullRequest>> {
			self.invocations.lock().expect("mutex poisoned").push(
				CodeForgeInvocation::FindOpenPullRequest {
					head: head.to_string(),
				},
			);
			if self.fail_find_pr {
				bail!("simulated find_open_pull_request failure");
			}
			Ok(self.existing_pr.clone())
		}

		async fn update_pull_request(
			&self,
			pull_number: u64,
			title: &str,
			body: &str,
		) -> anyhow::Result<String> {
			self.invocations.lock().expect("mutex poisoned").push(
				CodeForgeInvocation::UpdatePullRequest {
					pull_number,
					title: title.to_string(),
					body: body.to_string(),
				},
			);
			if self.fail_update_pr {
				bail!("simulated update_pull_request failure");
			}
			Ok(format!("https://example.com/pull/{pull_number}"))
		}

		async fn find_release_by_tag(&self, tag: &str) -> anyhow::Result<Option<ExistingRelease>> {
			self.invocations.lock().expect("mutex poisoned").push(
				CodeForgeInvocation::FindReleaseByTag {
					tag: tag.to_string(),
				},
			);
			if self.fail_find_release {
				bail!("simulated find_release_by_tag failure");
			}
			Ok(self.existing_releases.get(tag).cloned())
		}

		async fn publish_release(&self, release_id: &str) -> anyhow::Result<()> {
			self.invocations.lock().expect("mutex poisoned").push(
				CodeForgeInvocation::PublishRelease {
					release_id: release_id.to_string(),
				},
			);
			if self.fail_publish_release {
				bail!("simulated publish_release failure");
			}
			Ok(())
		}
	}
}
