//! Integration tests for [`RestGitHubClient`] over a mock HTTP server.
//!
//! These tests exercise the HTTP layer — URL construction, headers,
//! request body serialisation, and response parsing — by pointing
//! [`RestGitHubClient`] at an [`httpmock`] server instead of the real
//! GitHub API.
//!
//! When the GitHub OpenAPI spec is available (cached by `build.rs` at
//! `.cache/github-openapi.json`), the request body is also validated
//! against the spec schema using [`jsonschema`].

use std::io::Write as _;
use std::sync::{Arc, Mutex};

use chronicle::github::RestGitHubClient;
use chronicle::github::client::GitHubClient as _;
use httpmock::prelude::*;
use serde_json::Value;
use tempfile::NamedTempFile;

/// Path to the cached GitHub OpenAPI spec, written by build.rs.
const OPENAPI_SPEC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.cache/github-openapi.json");

/// Loads the GitHub OpenAPI spec if it was cached by the build script.
///
/// Returns `None` if the file is missing or cannot be parsed.
fn try_load_spec() -> Option<Value> {
	let content = std::fs::read_to_string(OPENAPI_SPEC_PATH).ok()?;
	serde_json::from_str(&content).ok()
}

/// Resolves a same-document JSON Reference (`"$ref": "#/..."`) within a spec value.
fn resolve_ref<'a>(doc: &'a Value, reference: &str) -> Option<&'a Value> {
	let path = reference.strip_prefix("#/")?;
	// Convert the reference path to a JSON Pointer by prepending '/'
	doc.pointer(&format!("/{path}"))
}

/// Extracts the JSON Schema for the `POST /repos/{owner}/{repo}/releases` request body.
///
/// Follows one level of `$ref` if present.
fn create_release_schema(spec: &Value) -> Option<Value> {
	let schema = spec.pointer(
		"/paths/~1repos~1{owner}~1{repo}~1releases/post/requestBody/content/application~1json/schema",
	)?;
	if let Some(ref_str) = schema.get("$ref").and_then(Value::as_str) {
		resolve_ref(spec, ref_str).cloned()
	} else {
		Some(schema.clone())
	}
}

/// Validates `instance` against `schema` using [`jsonschema`].
///
/// Returns `Ok(())` if the instance is valid, or an error listing all
/// violations.
fn validate(instance: &Value, schema: &Value) -> Result<(), String> {
	let validator =
		jsonschema::validator_for(schema).map_err(|e| format!("Failed to build validator: {e}"))?;
	let errors: Vec<_> = validator.iter_errors(instance).collect();
	if errors.is_empty() {
		Ok(())
	} else {
		Err(errors
			.iter()
			.map(|e| e.to_string())
			.collect::<Vec<_>>()
			.join("\n"))
	}
}

// ── create_release tests ──────────────────────────────────────────────────────

#[test]
fn create_release_sends_correct_request() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/owner/repo/releases")
			.header("Authorization", "Bearer test-token")
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", "2022-11-28");
		then.status(201)
			.header("Content-Type", "application/json")
			.body(r#"{"id": 12345}"#);
	});

	let client = RestGitHubClient::new("test-token".to_string())
		.with_base_urls(server.base_url(), server.base_url());

	let result = client.create_release(
		"owner",
		"repo",
		"v1.0.0",
		"Release v1.0.0",
		"Changelog body",
	);
	assert!(result.is_ok(), "create_release failed: {:?}", result.err());
	assert_eq!(result.unwrap(), "12345");
	mock.assert();
}

#[test]
fn create_release_sends_spec_compliant_request() {
	let spec = match try_load_spec() {
		Some(s) => s,
		None => {
			eprintln!(
				"Skipping create_release_sends_spec_compliant_request: \
                 OpenAPI spec not available at {OPENAPI_SPEC_PATH}"
			);
			return;
		}
	};

	// Capture the raw request body sent by the client via the mock's `matches` hook.
	let captured_body: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
	let captured_body_clone = Arc::clone(&captured_body);

	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/owner/repo/releases")
			.is_true(move |req| {
				*captured_body_clone.lock().unwrap() = req.body_string();
				true
			});
		then.status(201)
			.header("Content-Type", "application/json")
			.body(r#"{"id": 99999}"#);
	});

	let client = RestGitHubClient::new("test-token".to_string())
		.with_base_urls(server.base_url(), server.base_url());

	client
		.create_release("owner", "repo", "v1.0.0", "Release v1.0.0", "Changelog")
		.expect("create_release should succeed against mock server");

	let body_str = captured_body.lock().unwrap().clone();
	let body: Value = serde_json::from_str(&body_str).expect("client sent non-JSON request body");

	if let Some(schema) = create_release_schema(&spec) {
		validate(&body, &schema)
			.unwrap_or_else(|e| panic!("create_release request body is not spec-compliant:\n{e}"));
	} else {
		eprintln!(
			"Warning: could not extract create_release schema from spec \
             (spec structure may have changed)"
		);
	}
}

#[test]
fn create_release_handles_api_error() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST).path("/repos/owner/repo/releases");
		then.status(422)
			.header("Content-Type", "application/json")
			.body(r#"{"message": "Validation Failed", "errors": []}"#);
	});

	let client = RestGitHubClient::new("test-token".to_string())
		.with_base_urls(server.base_url(), server.base_url());

	let result = client.create_release("owner", "repo", "v1.0.0", "Release", "Body");
	assert!(result.is_err(), "Expected error on 422 response");
	let err = format!("{:#}", result.unwrap_err());
	assert!(
		err.contains("v1.0.0"),
		"Error message should mention the tag name, got: {err}"
	);
}

#[test]
fn response_with_extra_fields_still_deserializes() {
	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST).path("/repos/owner/repo/releases");
		// A realistic GitHub API response with many extra fields
		then.status(201)
			.header("Content-Type", "application/json")
			.body(
				r#"{
                "id": 54321,
                "url": "https://api.github.com/repos/owner/repo/releases/54321",
                "html_url": "https://github.com/owner/repo/releases/tag/v1.0.0",
                "assets_url": "https://api.github.com/repos/owner/repo/releases/54321/assets",
                "upload_url": "https://uploads.github.com/repos/owner/repo/releases/54321/assets{?name,label}",
                "tarball_url": "https://api.github.com/repos/owner/repo/tarball/v1.0.0",
                "zipball_url": "https://api.github.com/repos/owner/repo/zipball/v1.0.0",
                "tag_name": "v1.0.0",
                "name": "Release v1.0.0",
                "body": "Changelog body",
                "draft": false,
                "prerelease": false,
                "created_at": "2024-01-01T00:00:00Z",
                "published_at": "2024-01-01T00:00:00Z",
                "author": {"login": "user", "id": 1},
                "assets": []
            }"#,
			);
	});

	let client = RestGitHubClient::new("test-token".to_string())
		.with_base_urls(server.base_url(), server.base_url());

	let result = client.create_release(
		"owner",
		"repo",
		"v1.0.0",
		"Release v1.0.0",
		"Changelog body",
	);
	assert!(
		result.is_ok(),
		"Should handle extra fields gracefully, got: {:?}",
		result.err()
	);
	assert_eq!(result.unwrap(), "54321");
}

// ── upload_asset tests ────────────────────────────────────────────────────────

#[test]
fn upload_asset_percent_encodes_filename_in_url() {
	let mut file = NamedTempFile::new().unwrap();
	file.write_all(b"data").unwrap();
	let file_path = file.path().to_path_buf();

	let captured_uri: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
	let captured_uri_clone = Arc::clone(&captured_uri);

	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/owner/repo/releases/12345/assets")
			.is_true(move |req| {
				*captured_uri_clone.lock().unwrap() = req.uri_str().to_string();
				true
			});
		then.status(201)
			.header("Content-Type", "application/json")
			.body(r#"{"id": 888, "name": "my app (1).tar.gz"}"#);
	});

	let client = RestGitHubClient::new("test-token".to_string())
		.with_base_urls(server.base_url(), server.base_url());

	let result = client.upload_asset("owner", "repo", "12345", "my app (1).tar.gz", &file_path);
	assert!(result.is_ok(), "upload_asset failed: {:?}", result.err());
	mock.assert();

	let uri = captured_uri.lock().unwrap().clone();
	assert!(
		uri.contains("name=my%20app%20%281%29.tar.gz"),
		"URL should contain percent-encoded filename in name= param, got: {uri}"
	);
}

#[test]
fn upload_asset_sends_correct_request() {
	let mut file = NamedTempFile::new().unwrap();
	file.write_all(b"binary content").unwrap();
	let file_path = file.path().to_path_buf();

	let captured_uri: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
	let captured_uri_clone = Arc::clone(&captured_uri);

	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/owner/repo/releases/12345/assets")
			.header("Authorization", "Bearer test-token")
			.header("Content-Type", "application/octet-stream")
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", "2022-11-28")
			.is_true(move |req| {
				*captured_uri_clone.lock().unwrap() = req.uri_str().to_string();
				true
			});
		then.status(201)
			.header("Content-Type", "application/json")
			.body(r#"{"id": 777, "name": "app.tar.gz"}"#);
	});

	let client = RestGitHubClient::new("test-token".to_string())
		.with_base_urls(server.base_url(), server.base_url());

	let result = client.upload_asset("owner", "repo", "12345", "app.tar.gz", &file_path);
	assert!(result.is_ok(), "upload_asset failed: {:?}", result.err());
	mock.assert();

	let uri = captured_uri.lock().unwrap().clone();
	assert!(
		uri.contains("name=app.tar.gz"),
		"URL should contain name= query param, got: {uri}"
	);
}

#[test]
fn upload_asset_handles_api_error() {
	let mut file = NamedTempFile::new().unwrap();
	file.write_all(b"data").unwrap();
	let file_path = file.path().to_path_buf();

	let server = MockServer::start();
	let _mock = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/owner/repo/releases/12345/assets");
		then.status(500).body("Internal Server Error");
	});

	let client = RestGitHubClient::new("test-token".to_string())
		.with_base_urls(server.base_url(), server.base_url());

	let result = client.upload_asset("owner", "repo", "12345", "file.tar.gz", &file_path);
	assert!(result.is_err(), "Expected error on 500 response");
	let err = format!("{:#}", result.unwrap_err());
	assert!(
		err.contains("file.tar.gz"),
		"Error message should mention the filename, got: {err}"
	);
}
