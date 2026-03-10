//! Abstract GitHub API client trait and test support.

use std::path::Path;

use super::remote::GitHubRepo;

/// A resolved GitHub pull request.
#[derive(Debug, Clone)]
pub struct PullRequest {
	/// Pull request number.
	pub number: u64,
	/// Full URL of the pull request on GitHub.
	pub html_url: String,
}

/// Abstract interface for GitHub API operations.
///
/// All methods are synchronous. The production implementation uses
/// REST API calls over ureq. The `test_support` module provides a
/// recording fake for unit tests.
pub trait GitHubClient: Send + Sync + std::fmt::Debug {
	/// Creates a GitHub Release for the given tag, returning the release ID.
	///
	/// # Errors
	///
	/// Returns an error if the API call fails or authentication is missing.
	fn create_release(
		&self,
		gh_repo: &GitHubRepo,
		tag_name: &str,
		name: &str,
		body: &str,
	) -> anyhow::Result<String>;

	/// Uploads a file as an asset to an existing GitHub Release.
	///
	/// # Errors
	///
	/// Returns an error if the upload fails.
	fn upload_asset(
		&self,
		gh_repo: &GitHubRepo,
		release_id: &str,
		file_name: &str,
		file_path: &Path,
	) -> anyhow::Result<()>;

	/// Creates a pull request and returns the PR URL.
	///
	/// # Arguments
	///
	/// * `gh_repo` - GitHub repository (owner and name).
	/// * `title` - Pull request title.
	/// * `body` - Pull request description (markdown).
	/// * `head` - Source branch (the branch to merge from).
	/// * `base` - Target branch (the branch to merge into).
	///
	/// # Errors
	///
	/// Returns an error if the API call fails.
	fn create_pull_request(
		&self,
		gh_repo: &GitHubRepo,
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
	fn find_open_pull_request(
		&self,
		gh_repo: &GitHubRepo,
		head: &str,
	) -> anyhow::Result<Option<PullRequest>>;

	/// Updates the title and body of an existing pull request, returning the PR URL.
	///
	/// # Errors
	///
	/// Returns an error if the API call fails.
	fn update_pull_request(
		&self,
		gh_repo: &GitHubRepo,
		pull_number: u64,
		title: &str,
		body: &str,
	) -> anyhow::Result<String>;
}

/// Test support types for GitHub client operations.
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

	use super::{GitHubClient, PullRequest};
	use crate::github::remote::GitHubRepo;

	/// A recorded GitHub API invocation.
	#[derive(Debug, Clone)]
	pub enum GitHubInvocation {
		/// A `create_release` call.
		CreateRelease {
			/// GitHub repository (owner and name).
			gh_repo: GitHubRepo,
			/// Git tag name for the release.
			tag_name: String,
			/// Release title.
			name: String,
			/// Release body (markdown).
			body: String,
		},
		/// An `upload_asset` call.
		UploadAsset {
			/// GitHub repository (owner and name).
			gh_repo: GitHubRepo,
			/// ID of the release to attach the asset to.
			release_id: String,
			/// Asset file name as it will appear in the release.
			file_name: String,
			/// Local path of the file to upload.
			file_path: std::path::PathBuf,
		},
		/// A `create_pull_request` call.
		CreatePullRequest {
			/// GitHub repository (owner and name).
			gh_repo: GitHubRepo,
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
			/// GitHub repository (owner and name).
			gh_repo: GitHubRepo,
			/// Head branch to search for.
			head: String,
		},
		/// An `update_pull_request` call.
		UpdatePullRequest {
			/// GitHub repository (owner and name).
			gh_repo: GitHubRepo,
			/// Pull request number.
			pull_number: u64,
			/// New pull request title.
			title: String,
			/// New pull request body (markdown).
			body: String,
		},
	}

	/// A [`GitHubClient`] that records all invocations and returns configured responses.
	#[derive(Debug)]
	pub struct RecordingGitHubClient {
		invocations: Mutex<Vec<GitHubInvocation>>,
		release_id: String,
		fail_create: bool,
		fail_upload: bool,
		fail_create_pr: bool,
		existing_pr: Option<PullRequest>,
		fail_find_pr: bool,
		fail_update_pr: bool,
	}

	impl RecordingGitHubClient {
		/// Creates a new recording client that succeeds with `release_id = "release-1"`.
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
			}
		}

		/// Configures the release ID returned by [`create_release`](GitHubClient::create_release).
		pub fn with_release_id(mut self, id: impl Into<String>) -> Self {
			self.release_id = id.into();
			self
		}

		/// Causes [`create_release`](GitHubClient::create_release) to return an error.
		pub fn with_create_failure(mut self) -> Self {
			self.fail_create = true;
			self
		}

		/// Causes [`upload_asset`](GitHubClient::upload_asset) to return an error.
		pub fn with_upload_failure(mut self) -> Self {
			self.fail_upload = true;
			self
		}

		/// Causes [`create_pull_request`](GitHubClient::create_pull_request) to return an error.
		pub fn with_create_pr_failure(mut self) -> Self {
			self.fail_create_pr = true;
			self
		}

		/// Configures an existing PR to be returned by
		/// [`find_open_pull_request`](GitHubClient::find_open_pull_request).
		pub fn with_existing_pr(mut self, pr: PullRequest) -> Self {
			self.existing_pr = Some(pr);
			self
		}

		/// Causes [`find_open_pull_request`](GitHubClient::find_open_pull_request) to return an
		/// error.
		pub fn with_find_pr_failure(mut self) -> Self {
			self.fail_find_pr = true;
			self
		}

		/// Causes [`update_pull_request`](GitHubClient::update_pull_request) to return an error.
		pub fn with_update_pr_failure(mut self) -> Self {
			self.fail_update_pr = true;
			self
		}

		/// Returns all invocations recorded so far.
		pub fn invocations(&self) -> Vec<GitHubInvocation> {
			self.invocations.lock().expect("mutex poisoned").clone()
		}
	}

	impl Default for RecordingGitHubClient {
		fn default() -> Self {
			Self::new()
		}
	}

	impl GitHubClient for RecordingGitHubClient {
		fn create_release(
			&self,
			gh_repo: &GitHubRepo,
			tag_name: &str,
			name: &str,
			body: &str,
		) -> anyhow::Result<String> {
			self.invocations.lock().expect("mutex poisoned").push(
				GitHubInvocation::CreateRelease {
					gh_repo: gh_repo.clone(),
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

		fn upload_asset(
			&self,
			gh_repo: &GitHubRepo,
			release_id: &str,
			file_name: &str,
			file_path: &Path,
		) -> anyhow::Result<()> {
			self.invocations
				.lock()
				.expect("mutex poisoned")
				.push(GitHubInvocation::UploadAsset {
					gh_repo: gh_repo.clone(),
					release_id: release_id.to_string(),
					file_name: file_name.to_string(),
					file_path: file_path.to_path_buf(),
				});
			if self.fail_upload {
				bail!("simulated upload_asset failure");
			}
			Ok(())
		}

		fn create_pull_request(
			&self,
			gh_repo: &GitHubRepo,
			title: &str,
			body: &str,
			head: &str,
			base: &str,
		) -> anyhow::Result<String> {
			self.invocations.lock().expect("mutex poisoned").push(
				GitHubInvocation::CreatePullRequest {
					gh_repo: gh_repo.clone(),
					title: title.to_string(),
					body: body.to_string(),
					head: head.to_string(),
					base: base.to_string(),
				},
			);
			if self.fail_create_pr {
				bail!("simulated create_pull_request failure");
			}
			let owner = &gh_repo.owner;
			let repo = &gh_repo.repo;
			Ok(format!("https://github.com/{owner}/{repo}/pull/1"))
		}

		fn find_open_pull_request(
			&self,
			gh_repo: &GitHubRepo,
			head: &str,
		) -> anyhow::Result<Option<PullRequest>> {
			self.invocations.lock().expect("mutex poisoned").push(
				GitHubInvocation::FindOpenPullRequest {
					gh_repo: gh_repo.clone(),
					head: head.to_string(),
				},
			);
			if self.fail_find_pr {
				bail!("simulated find_open_pull_request failure");
			}
			Ok(self.existing_pr.clone())
		}

		fn update_pull_request(
			&self,
			gh_repo: &GitHubRepo,
			pull_number: u64,
			title: &str,
			body: &str,
		) -> anyhow::Result<String> {
			self.invocations.lock().expect("mutex poisoned").push(
				GitHubInvocation::UpdatePullRequest {
					gh_repo: gh_repo.clone(),
					pull_number,
					title: title.to_string(),
					body: body.to_string(),
				},
			);
			if self.fail_update_pr {
				bail!("simulated update_pull_request failure");
			}
			let owner = &gh_repo.owner;
			let repo = &gh_repo.repo;
			Ok(format!(
				"https://github.com/{owner}/{repo}/pull/{pull_number}"
			))
		}
	}

	#[cfg(test)]
	mod tests {
		use std::path::PathBuf;

		use super::*;

		#[test]
		fn recording_client_records_create_release() {
			let client = RecordingGitHubClient::new().with_release_id("r-42");
			let id = client
				.create_release(
					&GitHubRepo::new("owner", "repo").unwrap(),
					"v1.0.0",
					"Release 1.0.0",
					"body text",
				)
				.unwrap();
			assert_eq!(id, "r-42");
			let invocations = client.invocations();
			assert_eq!(invocations.len(), 1);
			assert!(matches!(
				&invocations[0],
				GitHubInvocation::CreateRelease { tag_name, .. } if tag_name == "v1.0.0"
			));
		}

		#[test]
		fn recording_client_records_upload_asset() {
			let client = RecordingGitHubClient::new();
			let path = PathBuf::from("/tmp/app.tar.gz");
			client
				.upload_asset(
					&GitHubRepo::new("owner", "repo").unwrap(),
					"r-1",
					"app.tar.gz",
					&path,
				)
				.unwrap();
			let invocations = client.invocations();
			assert_eq!(invocations.len(), 1);
			assert!(matches!(
				&invocations[0],
				GitHubInvocation::UploadAsset { file_name, .. } if file_name == "app.tar.gz"
			));
		}

		#[test]
		fn recording_client_create_failure_returns_error() {
			let client = RecordingGitHubClient::new().with_create_failure();
			let result = client.create_release(
				&GitHubRepo::new("owner", "repo").unwrap(),
				"v1.0.0",
				"Release",
				"body",
			);
			assert!(result.is_err());
			// Invocation is still recorded even on failure
			assert_eq!(client.invocations().len(), 1);
		}

		#[test]
		fn recording_client_upload_failure_returns_error() {
			let client = RecordingGitHubClient::new().with_upload_failure();
			let result = client.upload_asset(
				&GitHubRepo::new("owner", "repo").unwrap(),
				"r-1",
				"file.tar.gz",
				Path::new("/tmp/file.tar.gz"),
			);
			assert!(result.is_err());
			assert_eq!(client.invocations().len(), 1);
		}

		#[test]
		fn recording_client_records_create_pull_request() {
			let client = RecordingGitHubClient::new();
			let url = client
				.create_pull_request(
					&GitHubRepo::new("acme", "app").unwrap(),
					"Release updates",
					"Release:\n\n- my-pkg@1.0.0",
					"chronicle-release/main",
					"main",
				)
				.unwrap();
			assert!(
				url.contains("acme/app"),
				"URL should contain owner/repo: {url}"
			);
			let invocations = client.invocations();
			assert_eq!(invocations.len(), 1);
			assert!(matches!(
				&invocations[0],
				GitHubInvocation::CreatePullRequest { title, head, base, .. }
					if title == "Release updates" && head == "chronicle-release/main" && base == "main"
			));
		}

		#[test]
		fn recording_client_create_pr_failure_returns_error() {
			let client = RecordingGitHubClient::new().with_create_pr_failure();
			let result = client.create_pull_request(
				&GitHubRepo::new("acme", "app").unwrap(),
				"Release",
				"body",
				"release-branch",
				"main",
			);
			assert!(result.is_err());
			// Invocation is still recorded even on failure
			assert_eq!(client.invocations().len(), 1);
		}

		#[test]
		fn recording_client_find_open_pr_returns_none_when_not_configured() {
			let client = RecordingGitHubClient::new();
			let result = client
				.find_open_pull_request(
					&GitHubRepo::new("acme", "app").unwrap(),
					"chronicle-release/main",
				)
				.unwrap();
			assert!(result.is_none());
			let invocations = client.invocations();
			assert_eq!(invocations.len(), 1);
			assert!(matches!(
				&invocations[0],
				GitHubInvocation::FindOpenPullRequest { head, .. }
					if head == "chronicle-release/main"
			));
		}

		#[test]
		fn recording_client_find_open_pr_returns_configured_pr() {
			let pr = PullRequest {
				number: 42,
				html_url: "https://github.com/acme/app/pull/42".to_string(),
			};
			let client = RecordingGitHubClient::new().with_existing_pr(pr);
			let result = client
				.find_open_pull_request(
					&GitHubRepo::new("acme", "app").unwrap(),
					"chronicle-release/main",
				)
				.unwrap();
			assert!(result.is_some());
			let found = result.unwrap();
			assert_eq!(found.number, 42);
			assert!(found.html_url.contains("pull/42"));
		}

		#[test]
		fn recording_client_find_pr_failure_returns_error() {
			let client = RecordingGitHubClient::new().with_find_pr_failure();
			let result = client
				.find_open_pull_request(&GitHubRepo::new("acme", "app").unwrap(), "release-branch");
			assert!(result.is_err());
			assert_eq!(client.invocations().len(), 1);
		}

		#[test]
		fn recording_client_update_pull_request_records_invocation() {
			let client = RecordingGitHubClient::new();
			let url = client
				.update_pull_request(
					&GitHubRepo::new("acme", "app").unwrap(),
					42,
					"Updated Title",
					"Updated body",
				)
				.unwrap();
			assert!(url.contains("pull/42"), "URL should contain pull/42: {url}");
			let invocations = client.invocations();
			assert_eq!(invocations.len(), 1);
			assert!(matches!(
				&invocations[0],
				GitHubInvocation::UpdatePullRequest { pull_number, title, .. }
					if *pull_number == 42 && title == "Updated Title"
			));
		}

		#[test]
		fn recording_client_update_pr_failure_returns_error() {
			let client = RecordingGitHubClient::new().with_update_pr_failure();
			let result = client.update_pull_request(
				&GitHubRepo::new("acme", "app").unwrap(),
				1,
				"Title",
				"body",
			);
			assert!(result.is_err());
			assert_eq!(client.invocations().len(), 1);
		}
	}
}
