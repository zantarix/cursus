//! Production REST-based GitHub API client.
//!
//! Uses the GitHub REST API v3 over ureq to create releases and upload assets.

use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::client::GitHubClient;

/// GitHub REST API version header value, pinned to prevent breakage on future API changes.
const GITHUB_API_VERSION: &str = "2022-11-28";

/// Request body for creating a GitHub Release.
#[derive(Debug, Serialize)]
struct CreateReleaseRequest<'a> {
	tag_name: &'a str,
	name: &'a str,
	body: &'a str,
	draft: bool,
	prerelease: bool,
}

/// Response body from a GitHub Release creation.
#[derive(Debug, Deserialize)]
struct CreateReleaseResponse {
	id: u64,
}

/// Response body from a GitHub Release asset upload.
///
/// Fields are read by serde during JSON deserialization; the struct exists to
/// validate that the API returned a well-formed response.
#[derive(Debug, Deserialize)]
struct UploadAssetResponse {
	#[allow(dead_code)]
	id: u64,
	#[allow(dead_code)]
	name: String,
}

/// GitHub API client using the REST API over ureq.
///
/// Authentication is via a personal access token or GitHub App token,
/// passed as a `Bearer` token in the `Authorization` header.
#[derive(Debug)]
pub struct RestGitHubClient {
	token: String,
	api_base_url: String,
	upload_base_url: String,
}

impl RestGitHubClient {
	/// Creates a new REST client authenticated with the given token.
	///
	/// Uses the production GitHub API and uploads base URLs by default.
	pub fn new(token: String) -> Self {
		Self {
			token,
			api_base_url: "https://api.github.com".to_string(),
			upload_base_url: "https://uploads.github.com".to_string(),
		}
	}

	/// Overrides the base URLs used for API and upload requests.
	///
	/// Used in tests to point the client at a mock HTTP server.
	#[cfg(any(test, feature = "test-support"))]
	pub fn with_base_urls(mut self, api: impl Into<String>, upload: impl Into<String>) -> Self {
		self.api_base_url = api.into();
		self.upload_base_url = upload.into();
		self
	}

	/// Returns the standard headers used for all GitHub API requests.
	fn auth_header(&self) -> String {
		format!("Bearer {}", self.token)
	}
}

/// Validates that a GitHub owner or repository name contains only safe characters.
///
/// GitHub allows alphanumeric characters, hyphens, underscores, and dots. Rejecting
/// anything else prevents path-traversal attacks when values are interpolated into URLs.
fn validate_github_identifier(value: &str, field: &str) -> anyhow::Result<()> {
	if value.is_empty()
		|| !value
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
	{
		anyhow::bail!("Invalid GitHub {field}: {value:?}");
	}
	Ok(())
}

/// Percent-encodes a string for use in a URL query parameter value.
///
/// Encodes all bytes that are not in the unreserved character set (A-Z, a-z, 0-9, `-`, `_`, `.`, `~`).
fn percent_encode(s: &str) -> String {
	let mut encoded = String::with_capacity(s.len());
	for b in s.bytes() {
		match b {
			b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
				encoded.push(b as char);
			}
			b => {
				encoded.push_str(&format!("%{b:02X}"));
			}
		}
	}
	encoded
}

impl GitHubClient for RestGitHubClient {
	fn create_release(
		&self,
		owner: &str,
		repo: &str,
		tag_name: &str,
		name: &str,
		body: &str,
	) -> anyhow::Result<String> {
		validate_github_identifier(owner, "owner")?;
		validate_github_identifier(repo, "repo")?;
		let url = format!("{}/repos/{owner}/{repo}/releases", self.api_base_url);
		let request_body = CreateReleaseRequest {
			tag_name,
			name,
			body,
			draft: false,
			prerelease: false,
		};
		let response: CreateReleaseResponse = ureq::post(&url)
			.header("Authorization", &self.auth_header())
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", GITHUB_API_VERSION)
			.header(
				"User-Agent",
				&format!("chronicle/{}", env!("CARGO_PKG_VERSION")),
			)
			.send_json(&request_body)
			.with_context(|| format!("Failed to create GitHub Release for tag '{tag_name}'"))?
			.body_mut()
			.read_json()
			.context("Failed to parse GitHub Release creation response")?;
		Ok(response.id.to_string())
	}

	fn upload_asset(
		&self,
		owner: &str,
		repo: &str,
		release_id: &str,
		file_name: &str,
		file_path: &Path,
	) -> anyhow::Result<()> {
		validate_github_identifier(owner, "owner")?;
		validate_github_identifier(repo, "repo")?;
		if release_id.is_empty() || !release_id.chars().all(|c| c.is_ascii_digit()) {
			anyhow::bail!("Invalid GitHub release_id: {release_id:?}");
		}
		let file = std::fs::File::open(file_path)
			.with_context(|| format!("Failed to read asset file '{}'", file_path.display()))?;

		let encoded_name = percent_encode(file_name);
		let url = format!(
			"{}/repos/{owner}/{repo}/releases/{release_id}/assets?name={encoded_name}",
			self.upload_base_url
		);
		let _response: UploadAssetResponse = ureq::post(&url)
			.header("Authorization", &self.auth_header())
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", GITHUB_API_VERSION)
			.header(
				"User-Agent",
				&format!("chronicle/{}", env!("CARGO_PKG_VERSION")),
			)
			.header("Content-Type", "application/octet-stream")
			.send(file)
			.with_context(|| format!("Failed to upload asset '{file_name}'"))?
			.body_mut()
			.read_json()
			.context("Failed to parse GitHub asset upload response")?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn create_release_request_serializes_correctly() {
		let req = CreateReleaseRequest {
			tag_name: "v1.2.3",
			name: "Release 1.2.3",
			body: "## Changes\n- Fixed bug",
			draft: false,
			prerelease: false,
		};
		let json = serde_json::to_value(&req).unwrap();
		assert_eq!(json["tag_name"], "v1.2.3");
		assert_eq!(json["name"], "Release 1.2.3");
		assert_eq!(json["body"], "## Changes\n- Fixed bug");
		assert_eq!(json["draft"], false);
		assert_eq!(json["prerelease"], false);
	}

	#[test]
	fn create_release_response_deserializes_correctly() {
		let json =
			r#"{"id": 12345678, "url": "https://api.github.com/repos/o/r/releases/12345678"}"#;
		let response: CreateReleaseResponse = serde_json::from_str(json).unwrap();
		assert_eq!(response.id, 12345678);
	}

	#[test]
	fn upload_asset_response_deserializes_correctly() {
		let json =
			r#"{"id": 987, "name": "app.tar.gz", "size": 1024, "url": "https://example.com"}"#;
		let response: UploadAssetResponse = serde_json::from_str(json).unwrap();
		assert_eq!(response.id, 987);
		assert_eq!(response.name, "app.tar.gz");
	}

	#[test]
	fn upload_asset_returns_error_when_file_does_not_exist() {
		let client = RestGitHubClient::new("token".to_string());
		let result = client.upload_asset(
			"owner",
			"repo",
			"12345678",
			"missing.tar.gz",
			Path::new("/nonexistent/path/to/missing.tar.gz"),
		);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("missing.tar.gz"),
			"Error should mention file path, got: {msg}"
		);
	}

	#[test]
	fn upload_asset_rejects_invalid_release_id() {
		let client = RestGitHubClient::new("token".to_string());
		for bad_id in &["", "abc", "12-34", "../evil", "12 34"] {
			let result = client.upload_asset(
				"owner",
				"repo",
				bad_id,
				"file.tar.gz",
				Path::new("/tmp/file.tar.gz"),
			);
			assert!(
				result.is_err(),
				"Expected error for release_id={bad_id:?}, but got Ok"
			);
		}
	}

	#[test]
	fn upload_asset_accepts_numeric_release_id() {
		// Validation should pass for a numeric release ID; the error is the missing file.
		let client = RestGitHubClient::new("token".to_string());
		let result = client.upload_asset(
			"owner",
			"repo",
			"987654321",
			"file.tar.gz",
			Path::new("/nonexistent/file.tar.gz"),
		);
		assert!(result.is_err());
		// Error should be about the file, not the release_id.
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("file.tar.gz"),
			"Error should be about the missing file, got: {msg}"
		);
	}

	#[test]
	fn percent_encode_unreserved_chars_unchanged() {
		let input = "linux-amd64.tar.gz";
		assert_eq!(percent_encode(input), "linux-amd64.tar.gz");
	}

	#[test]
	fn percent_encode_special_chars_encoded() {
		assert_eq!(percent_encode("a b"), "a%20b");
		assert_eq!(percent_encode("a&b"), "a%26b");
		assert_eq!(percent_encode("a?b"), "a%3Fb");
		assert_eq!(percent_encode("a#b"), "a%23b");
		assert_eq!(percent_encode("a%b"), "a%25b");
	}

	#[test]
	fn percent_encode_empty_string() {
		assert_eq!(percent_encode(""), "");
	}

	#[test]
	fn validate_github_identifier_accepts_valid_names() {
		assert!(validate_github_identifier("acme", "owner").is_ok());
		assert!(validate_github_identifier("my-org", "owner").is_ok());
		assert!(validate_github_identifier("my_repo.js", "repo").is_ok());
		assert!(validate_github_identifier("Org123", "owner").is_ok());
	}

	#[test]
	fn validate_github_identifier_rejects_invalid_names() {
		assert!(validate_github_identifier("", "owner").is_err());
		assert!(validate_github_identifier("a/b", "owner").is_err());
		assert!(validate_github_identifier("../evil", "repo").is_err());
		assert!(validate_github_identifier("a b", "owner").is_err());
	}
}
