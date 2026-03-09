//! Abstract GitHub API client trait and test support.

use std::path::Path;

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
		owner: &str,
		repo: &str,
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
		owner: &str,
		repo: &str,
		release_id: &str,
		file_name: &str,
		file_path: &Path,
	) -> anyhow::Result<()>;
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

	use super::GitHubClient;

	/// A recorded GitHub API invocation.
	#[derive(Debug, Clone)]
	pub enum GitHubInvocation {
		/// A `create_release` call.
		CreateRelease {
			/// Repository owner.
			owner: String,
			/// Repository name.
			repo: String,
			/// Git tag name for the release.
			tag_name: String,
			/// Release title.
			name: String,
			/// Release body (markdown).
			body: String,
		},
		/// An `upload_asset` call.
		UploadAsset {
			/// Repository owner.
			owner: String,
			/// Repository name.
			repo: String,
			/// ID of the release to attach the asset to.
			release_id: String,
			/// Asset file name as it will appear in the release.
			file_name: String,
			/// Local path of the file to upload.
			file_path: std::path::PathBuf,
		},
	}

	/// A [`GitHubClient`] that records all invocations and returns configured responses.
	#[derive(Debug)]
	pub struct RecordingGitHubClient {
		invocations: Mutex<Vec<GitHubInvocation>>,
		release_id: String,
		fail_create: bool,
		fail_upload: bool,
	}

	impl RecordingGitHubClient {
		/// Creates a new recording client that succeeds with `release_id = "release-1"`.
		pub fn new() -> Self {
			Self {
				invocations: Mutex::new(Vec::new()),
				release_id: "release-1".to_string(),
				fail_create: false,
				fail_upload: false,
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
			owner: &str,
			repo: &str,
			tag_name: &str,
			name: &str,
			body: &str,
		) -> anyhow::Result<String> {
			self.invocations.lock().expect("mutex poisoned").push(
				GitHubInvocation::CreateRelease {
					owner: owner.to_string(),
					repo: repo.to_string(),
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
			owner: &str,
			repo: &str,
			release_id: &str,
			file_name: &str,
			file_path: &Path,
		) -> anyhow::Result<()> {
			self.invocations
				.lock()
				.expect("mutex poisoned")
				.push(GitHubInvocation::UploadAsset {
					owner: owner.to_string(),
					repo: repo.to_string(),
					release_id: release_id.to_string(),
					file_name: file_name.to_string(),
					file_path: file_path.to_path_buf(),
				});
			if self.fail_upload {
				bail!("simulated upload_asset failure");
			}
			Ok(())
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
				.create_release("owner", "repo", "v1.0.0", "Release 1.0.0", "body text")
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
				.upload_asset("owner", "repo", "r-1", "app.tar.gz", &path)
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
			let result = client.create_release("owner", "repo", "v1.0.0", "Release", "body");
			assert!(result.is_err());
			// Invocation is still recorded even on failure
			assert_eq!(client.invocations().len(), 1);
		}

		#[test]
		fn recording_client_upload_failure_returns_error() {
			let client = RecordingGitHubClient::new().with_upload_failure();
			let result = client.upload_asset(
				"owner",
				"repo",
				"r-1",
				"file.tar.gz",
				Path::new("/tmp/file.tar.gz"),
			);
			assert!(result.is_err());
			assert_eq!(client.invocations().len(), 1);
		}
	}
}
