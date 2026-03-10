//! Production REST-based GitHub API client.
//!
//! Uses the GitHub REST API v3 over ureq to create releases and upload assets.

use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::client::GitHubClient;
use super::remote::GitHubRepo;

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

/// Request body for creating a GitHub pull request.
#[derive(Debug, Serialize)]
struct CreatePullRequestRequest<'a> {
	title: &'a str,
	body: &'a str,
	head: &'a str,
	base: &'a str,
}

/// Response body from a GitHub pull request creation.
#[derive(Debug, Deserialize)]
struct CreatePullRequestResponse {
	html_url: String,
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
///
/// The underlying [`ureq::Agent`] is configured with `http_status_as_error(false)`
/// so that non-2xx responses are returned normally and their bodies can be
/// logged at trace level before the error is propagated.
#[derive(Debug)]
pub struct RestGitHubClient {
	token: String,
	api_base_url: String,
	upload_base_url: String,
	agent: ureq::Agent,
}

impl RestGitHubClient {
	/// Creates a new REST client authenticated with the given token.
	///
	/// Uses the production GitHub API and uploads base URLs by default.
	pub fn new(token: String) -> Self {
		let config = ureq::Agent::config_builder()
			.http_status_as_error(false)
			.build();
		let agent = ureq::Agent::new_with_config(config);
		Self {
			token,
			api_base_url: "https://api.github.com".to_string(),
			upload_base_url: "https://uploads.github.com".to_string(),
			agent,
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

	/// Returns the `Authorization` header value for the current token.
	fn auth_header(&self) -> String {
		format!("Bearer {}", self.token)
	}

	/// Returns a POST request builder for `url` with all standard GitHub API
	/// headers pre-applied (Authorization, Accept, API version, User-Agent).
	fn post_request(&self, url: &str) -> ureq::RequestBuilder<ureq::typestate::WithBody> {
		self.agent
			.post(url)
			.header("Authorization", &self.auth_header())
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", GITHUB_API_VERSION)
			.header(
				"User-Agent",
				&format!("chronicle/{}", env!("CARGO_PKG_VERSION")),
			)
	}

	/// Checks that `response` has a 2xx status. On failure, logs the status
	/// and response body then returns `Err` wrapped with `context`.
	fn require_success(
		method: &str,
		url: &str,
		response: &mut ureq::http::Response<ureq::Body>,
		context: impl Fn() -> String,
	) -> anyhow::Result<()> {
		if response.status().is_success() {
			return Ok(());
		}
		let code = response.status().as_u16();
		let reason = response
			.status()
			.canonical_reason()
			.unwrap_or("Unknown")
			.to_string();
		let resp_body = response.body_mut().read_to_string().unwrap_or_default();
		Self::log_http_failure(method, url, code, &reason, &resp_body);
		Err(anyhow::anyhow!("HTTP {code} {reason}")).with_context(context)
	}

	/// Logs an HTTP failure at debug level and, when trace is enabled, also logs the response body.
	fn log_http_failure(method: &str, url: &str, code: u16, reason: &str, resp_body: &str) {
		log::debug!("{method} {url} -> {code} {reason}");
		log::trace!("  response body: {resp_body}");
	}
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
		gh_repo: &GitHubRepo,
		tag_name: &str,
		name: &str,
		body: &str,
	) -> anyhow::Result<String> {
		let url = format!(
			"{}/repos/{}/{}/releases",
			self.api_base_url, gh_repo.owner, gh_repo.repo
		);
		let request_body = CreateReleaseRequest {
			tag_name,
			name,
			body,
			draft: false,
			prerelease: false,
		};
		log::trace!(
			"  request body: {}",
			serde_json::to_string(&request_body).unwrap_or_default()
		);
		let mut response = self
			.post_request(&url)
			.send_json(&request_body)
			.with_context(|| format!("Failed to create GitHub Release for tag '{tag_name}'"))?;
		Self::require_success("POST", &url, &mut response, || {
			format!("Failed to create GitHub Release for tag '{tag_name}'")
		})?;
		let release: CreateReleaseResponse = response
			.body_mut()
			.read_json()
			.context("Failed to parse GitHub Release creation response")?;
		Ok(release.id.to_string())
	}

	fn upload_asset(
		&self,
		gh_repo: &GitHubRepo,
		release_id: &str,
		file_name: &str,
		file_path: &Path,
	) -> anyhow::Result<()> {
		if release_id.is_empty() || !release_id.chars().all(|c| c.is_ascii_digit()) {
			anyhow::bail!("Invalid GitHub release_id: {release_id:?}");
		}
		let file = std::fs::File::open(file_path)
			.with_context(|| format!("Failed to read asset file '{}'", file_path.display()))?;

		let encoded_name = percent_encode(file_name);
		let url = format!(
			"{}/repos/{}/{}/releases/{release_id}/assets?name={encoded_name}",
			self.upload_base_url, gh_repo.owner, gh_repo.repo
		);
		let mut response = self
			.post_request(&url)
			.header("Content-Type", "application/octet-stream")
			.send(file)
			.with_context(|| format!("Failed to upload asset '{file_name}'"))?;
		Self::require_success("POST", &url, &mut response, || {
			format!("Failed to upload asset '{file_name}'")
		})?;
		let _: UploadAssetResponse = response
			.body_mut()
			.read_json()
			.context("Failed to parse GitHub asset upload response")?;
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
		let url = format!(
			"{}/repos/{}/{}/pulls",
			self.api_base_url, gh_repo.owner, gh_repo.repo
		);
		let request_body = CreatePullRequestRequest {
			title,
			body,
			head,
			base,
		};
		log::trace!(
			"  request body: {}",
			serde_json::to_string(&request_body).unwrap_or_default()
		);
		let mut response = self
			.post_request(&url)
			.send_json(&request_body)
			.with_context(|| format!("Failed to create pull request '{title}'"))?;
		Self::require_success("POST", &url, &mut response, || {
			format!("Failed to create pull request '{title}'")
		})?;
		let pr: CreatePullRequestResponse = response
			.body_mut()
			.read_json()
			.context("Failed to parse pull request creation response")?;
		Ok(pr.html_url)
	}
}

#[cfg(test)]
mod tests {
	use std::io::Write as _;

	use httpmock::prelude::*;
	use tempfile::NamedTempFile;

	use super::*;
	use crate::test_logging::{init_test_logger, take_logs};

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
			&GitHubRepo::new("owner", "repo").unwrap(),
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
				&GitHubRepo::new("owner", "repo").unwrap(),
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
			&GitHubRepo::new("owner", "repo").unwrap(),
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
	fn create_release_error_logs_debug_message_on_422() {
		init_test_logger();
		let _ = take_logs();

		let server = MockServer::start();
		let _mock = server.mock(|when, then| {
			when.method(POST).path("/repos/owner/repo/releases");
			then.status(422)
				.header("Content-Type", "application/json")
				.body(r#"{"message": "Validation Failed", "errors": []}"#);
		});

		let client = RestGitHubClient::new("test-token".to_string())
			.with_base_urls(server.base_url(), server.base_url());

		let result = client.create_release(
			&GitHubRepo::new("owner", "repo").unwrap(),
			"v1.0.0",
			"Release",
			"Body",
		);
		assert!(result.is_err());

		let logs = take_logs();
		// Our log format is: "POST {url} -> 422 {reason}"
		let debug_msg = logs
			.iter()
			.find(|(level, msg)| {
				*level == log::Level::Debug && msg.contains("POST") && msg.contains("422")
			})
			.map(|(_, msg)| msg.as_str())
			.expect("expected a debug log message containing POST and 422");
		assert!(
			debug_msg.contains("/repos/owner/repo/releases"),
			"debug log should contain URL path: {debug_msg}"
		);
	}

	#[test]
	fn create_release_error_logs_trace_with_request_and_response_body() {
		init_test_logger();
		let _ = take_logs();

		let server = MockServer::start();
		let _mock = server.mock(|when, then| {
			when.method(POST).path("/repos/owner/repo/releases");
			then.status(422)
				.header("Content-Type", "application/json")
				.body(r#"{"message": "already exists"}"#);
		});

		let client = RestGitHubClient::new("test-token".to_string())
			.with_base_urls(server.base_url(), server.base_url());

		let _ = client.create_release(
			&GitHubRepo::new("owner", "repo").unwrap(),
			"v1.0.0",
			"Release",
			"Body",
		);

		let logs = take_logs();
		let trace_msgs: Vec<&str> = logs
			.iter()
			.filter(|(level, _)| *level == log::Level::Trace)
			.map(|(_, msg)| msg.as_str())
			.collect();

		let has_resp_body = trace_msgs.iter().any(|m| m.contains("already exists"));
		assert!(
			has_resp_body,
			"trace log should include response body content, got: {trace_msgs:?}"
		);

		let has_req_body = trace_msgs.iter().any(|m| m.contains("tag_name"));
		assert!(
			has_req_body,
			"trace log should include request body content, got: {trace_msgs:?}"
		);
	}

	#[test]
	fn upload_asset_error_logs_debug_message_on_500() {
		init_test_logger();
		let _ = take_logs();

		let mut file = NamedTempFile::new().unwrap();
		file.write_all(b"data").unwrap();

		let server = MockServer::start();
		let _mock = server.mock(|when, then| {
			when.method(POST)
				.path("/repos/owner/repo/releases/12345/assets");
			then.status(500).body("Internal Server Error");
		});

		let client = RestGitHubClient::new("test-token".to_string())
			.with_base_urls(server.base_url(), server.base_url());

		let result = client.upload_asset(
			&GitHubRepo::new("owner", "repo").unwrap(),
			"12345",
			"file.tar.gz",
			file.path(),
		);
		assert!(result.is_err());

		let logs = take_logs();
		// Our log format is: "POST {url} -> 500 {reason}"
		let debug_msg = logs
			.iter()
			.find(|(level, msg)| {
				*level == log::Level::Debug && msg.contains("POST") && msg.contains("500")
			})
			.map(|(_, msg)| msg.as_str())
			.expect("expected a debug log message containing POST and 500");
		assert!(
			debug_msg.contains("/repos/owner/repo/releases"),
			"debug log should contain URL path: {debug_msg}"
		);
	}

	#[test]
	fn log_http_failure_helper_formats_correctly() {
		init_test_logger();
		let _ = take_logs();
		RestGitHubClient::log_http_failure(
			"POST",
			"https://api.example.com/releases",
			422,
			"Unprocessable Entity",
			"error body",
		);
		let logs = take_logs();
		let debug = logs
			.iter()
			.find(|(l, _)| *l == log::Level::Debug)
			.map(|(_, m)| m.as_str())
			.expect("expected debug log");
		assert!(debug.contains("POST"));
		assert!(debug.contains("422"));
		assert!(debug.contains("Unprocessable Entity"));
		let trace = logs
			.iter()
			.find(|(l, _)| *l == log::Level::Trace)
			.map(|(_, m)| m.as_str())
			.expect("expected trace log");
		assert!(trace.contains("error body"));
	}

	#[test]
	fn create_pull_request_request_serializes_correctly() {
		let req = CreatePullRequestRequest {
			title: "Release updates",
			body: "Release:\n\n- my-pkg@1.0.0",
			head: "chronicle-release/main",
			base: "main",
		};
		let json = serde_json::to_value(&req).unwrap();
		assert_eq!(json["title"], "Release updates");
		assert_eq!(json["head"], "chronicle-release/main");
		assert_eq!(json["base"], "main");
	}

	#[test]
	fn create_pull_request_response_deserializes_correctly() {
		let json = r#"{"id": 42, "html_url": "https://github.com/acme/app/pull/1", "number": 1}"#;
		let response: CreatePullRequestResponse = serde_json::from_str(json).unwrap();
		assert_eq!(response.html_url, "https://github.com/acme/app/pull/1");
	}

	#[test]
	fn create_pull_request_sends_correct_request() {
		let server = MockServer::start();
		let _mock = server.mock(|when, then| {
			when.method(POST).path("/repos/acme/app/pulls");
			then.status(201)
				.header("Content-Type", "application/json")
				.body(
					r#"{"id": 1, "number": 1, "html_url": "https://github.com/acme/app/pull/1"}"#,
				);
		});

		let client = RestGitHubClient::new("test-token".to_string())
			.with_base_urls(server.base_url(), server.base_url());

		let url = client
			.create_pull_request(
				&GitHubRepo::new("acme", "app").unwrap(),
				"Release updates",
				"body",
				"release-branch",
				"main",
			)
			.unwrap();
		assert_eq!(url, "https://github.com/acme/app/pull/1");
	}

	#[test]
	fn create_pull_request_returns_error_on_failure() {
		let server = MockServer::start();
		let _mock = server.mock(|when, then| {
			when.method(POST).path("/repos/acme/app/pulls");
			then.status(422)
				.header("Content-Type", "application/json")
				.body(r#"{"message": "Validation Failed"}"#);
		});

		let client = RestGitHubClient::new("test-token".to_string())
			.with_base_urls(server.base_url(), server.base_url());

		let result = client.create_pull_request(
			&GitHubRepo::new("acme", "app").unwrap(),
			"Release updates",
			"body",
			"release-branch",
			"main",
		);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("422"),
			"Error should contain status code: {msg}"
		);
	}
}
