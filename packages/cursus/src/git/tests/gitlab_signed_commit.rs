use std::path::Path;
use std::sync::Arc;

use gitlab::{AsyncGitlab, GitlabBuilder};
use httpmock::MockServer;
use serde_json::json;

use crate::command::test_support::{DispatchingCommandRunner, RecordingCommandRunner};
use crate::command::{CommandRunner, DryRunCommandRunner};
use crate::filesystem::LocalFilesystem;
use crate::forge::gitlab::GitLabProject;
use crate::git::gitlab_signed_commit::{GitLabSignedCommit, PendingCommit};
use crate::git::{Git, GitWorkdir};
use crate::path::AbsolutePath;

fn make_git(runner: Arc<dyn CommandRunner>, root: &AbsolutePath) -> Arc<dyn Git> {
	Arc::new(GitWorkdir::new(runner, root.clone()))
}

async fn make_async_gitlab(server: &MockServer) -> Arc<AsyncGitlab> {
	// `build_async()` hits `/api/v4/user` to verify the connection.
	server.mock(|when, then| {
		when.method(httpmock::Method::GET).path("/api/v4/user");
		then.status(200)
			.header("Content-Type", "application/json")
			.body(json!({"id": 1, "username": "test"}).to_string());
	});
	let host = server
		.base_url()
		.trim_start_matches("http://")
		.trim_start_matches("https://")
		.to_string();
	let mut builder = GitlabBuilder::new(&host, "test-token");
	builder.insecure();
	Arc::new(builder.build_async().await.unwrap())
}

fn make_project() -> GitLabProject {
	GitLabProject::new("gitlab.example.com", "acme", "app").unwrap()
}

async fn make_decorator(
	git: Arc<dyn Git>,
	client: Arc<AsyncGitlab>,
	runner: Arc<dyn CommandRunner>,
) -> GitLabSignedCommit {
	GitLabSignedCommit::new(
		git,
		Arc::new(LocalFilesystem),
		client,
		runner,
		make_project(),
		false,
	)
}

async fn make_decorator_dry_run(
	git: Arc<dyn Git>,
	client: Arc<AsyncGitlab>,
	runner: Arc<dyn CommandRunner>,
) -> GitLabSignedCommit {
	GitLabSignedCommit::new(
		git,
		Arc::new(LocalFilesystem),
		client,
		runner,
		make_project(),
		true,
	)
}

/// Returns a [`DispatchingCommandRunner`] pre-configured so that
/// `git rev-parse HEAD` returns `head_sha`,
/// `git rev-parse --abbrev-ref HEAD` returns `branch`, and any other
/// invocation succeeds with the default exit code.
fn git_runner(head_sha: &str, branch: &str) -> Arc<DispatchingCommandRunner> {
	Arc::new(
		DispatchingCommandRunner::new(0)
			.on_with_args_stdout(
				"git",
				vec![
					"rev-parse".to_string(),
					"--abbrev-ref".to_string(),
					"HEAD".to_string(),
				],
				0,
				format!("{branch}\n").into_bytes(),
			)
			.on_with_args_stdout(
				"git",
				vec!["rev-parse".to_string(), "HEAD".to_string()],
				0,
				format!("{head_sha}\n").into_bytes(),
			),
	)
}

/// Reuses [`git_runner`] but adds a `cat-file -e HEAD:<file>` rule so the
/// caller can control whether `path_exists_at_head` returns true or false.
fn git_runner_with_head_paths(
	head_sha: &str,
	branch: &str,
	existing_at_head: &[&str],
) -> Arc<DispatchingCommandRunner> {
	let mut runner = DispatchingCommandRunner::new(0)
		.on_with_args_stdout(
			"git",
			vec![
				"rev-parse".to_string(),
				"--abbrev-ref".to_string(),
				"HEAD".to_string(),
			],
			0,
			format!("{branch}\n").into_bytes(),
		)
		.on_with_args_stdout(
			"git",
			vec!["rev-parse".to_string(), "HEAD".to_string()],
			0,
			format!("{head_sha}\n").into_bytes(),
		);
	for file in existing_at_head {
		// `cat-file -e HEAD:<file>` returns 0 → present at HEAD.
		runner = runner.on_with_args(
			"git",
			vec![
				"cat-file".to_string(),
				"-e".to_string(),
				format!("HEAD:{file}"),
			],
			0,
		);
	}
	Arc::new(runner)
}

fn commit_endpoint_path() -> &'static str {
	"/api/v4/projects/acme%2Fapp/repository/commits"
}

#[tokio::test]
async fn add_delegates_to_inner_and_records_paths() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;

	let file = dir.path().join("foo.txt");
	std::fs::write(&file, b"hello").unwrap();
	dec.add(std::slice::from_ref(&file)).await.unwrap();

	let calls = runner.invocations();
	assert!(calls.iter().any(|c| c.args.contains(&"add".to_string())));
	let state = dec.state.lock().await;
	assert_eq!(state.staged, vec![file]);
}

#[tokio::test]
async fn commit_dry_run_makes_no_api_calls() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let catch_all = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path());
		then.status(200).body("{}");
	});

	let dec = make_decorator_dry_run(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;

	let file = root.child("foo.txt");
	std::fs::write(file.as_path(), b"content").unwrap();
	dec.add(&[file.into_path_buf()]).await.unwrap();
	dec.commit("ci(release): version packages").await.unwrap();

	catch_all.assert_calls(0);
	assert!(dec.state.lock().await.pending.is_none());
}

#[tokio::test]
async fn push_dry_run_makes_no_api_calls() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let catch_all = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path());
		then.status(200).body("{}");
	});

	let dec = make_decorator_dry_run(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;

	// Pre-seed pending state so we know dry_run guards the push path.
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			parent_sha: "abc".to_string(),
			message: "msg".to_string(),
			paths: vec![],
		});
	}

	dec.push().await.unwrap();
	dec.force_push_branch("main").await.unwrap();

	catch_all.assert_calls(0);
	assert!(
		runner.invocations().is_empty(),
		"no inner git work in dry-run"
	);
}

#[tokio::test]
async fn commit_with_no_staged_files_is_noop() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let catch_all = server.mock(|when, then| {
		when.any_request();
		then.status(200).body("{}");
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;

	dec.commit("ci(release): version packages").await.unwrap();
	// The /api/v4/user probe fires once during build; nothing else.
	assert!(catch_all.calls() <= 1);
	assert!(dec.state.lock().await.pending.is_none());
}

#[tokio::test]
async fn commit_records_pending_with_branch_parent_message_and_paths() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = git_runner("abc123", "main");
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	let file = root.child("Cargo.toml");
	std::fs::write(file.as_path(), b"[package]").unwrap();

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	dec.add(&[file.as_path().to_path_buf()]).await.unwrap();
	dec.commit("ci(release): version packages").await.unwrap();

	let state = dec.state.lock().await;
	assert!(state.staged.is_empty(), "staged cleared after commit");
	let pending = state.pending.as_ref().expect("pending should be set");
	assert_eq!(pending.branch, "main");
	assert_eq!(pending.parent_sha, "abc123");
	assert_eq!(pending.message, "ci(release): version packages");
	assert_eq!(pending.paths, vec![file.into_path_buf()]);
}

#[tokio::test]
async fn commit_deduplicates_staged_paths() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = git_runner("abc123", "main");
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	let file = root.child("Cargo.toml");
	std::fs::write(file.as_path(), b"[package]").unwrap();

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	dec.add(&[file.as_path().to_path_buf()]).await.unwrap();
	dec.add(&[file.as_path().to_path_buf()]).await.unwrap();
	dec.commit("ci(release): version packages").await.unwrap();

	let state = dec.state.lock().await;
	let pending = state.pending.as_ref().unwrap();
	assert_eq!(
		pending.paths.len(),
		1,
		"duplicate paths must be deduped at commit() time"
	);
}

#[tokio::test]
async fn commit_fails_on_detached_head() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(
		DispatchingCommandRunner::new(0)
			.on_with_args_stdout(
				"git",
				vec!["rev-parse".to_string(), "HEAD".to_string()],
				0,
				b"abc123\n".to_vec(),
			)
			.on_with_args_stdout(
				"git",
				vec![
					"rev-parse".to_string(),
					"--abbrev-ref".to_string(),
					"HEAD".to_string(),
				],
				0,
				b"HEAD\n".to_vec(),
			),
	);
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	let file = root.child("foo.txt");
	std::fs::write(file.as_path(), b"content").unwrap();

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	dec.add(&[file.into_path_buf()]).await.unwrap();
	let result = dec.commit("ci(release): version packages").await;

	assert!(result.is_err());
	assert!(
		result.unwrap_err().to_string().contains("detached HEAD"),
		"expected detached HEAD error"
	);
}

#[tokio::test]
async fn push_without_pending_delegates_to_inner() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let catch_all = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path());
		then.status(200).body("{}");
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;

	dec.push().await.unwrap();

	catch_all.assert_calls(0);
	let calls = runner.invocations();
	assert!(
		calls
			.iter()
			.any(|c| c.program == "git" && c.args.contains(&"push".to_string())),
		"expected inner git push"
	);
}

#[tokio::test]
async fn force_push_branch_without_pending_delegates_to_inner() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let catch_all = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path());
		then.status(200).body("{}");
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;

	dec.force_push_branch("release-branch").await.unwrap();

	catch_all.assert_calls(0);
	let calls = runner.invocations();
	assert!(
		calls.iter().any(|c| c.program == "git"
			&& c.args.contains(&"push".to_string())
			&& c.args.contains(&"release-branch".to_string())),
		"expected inner git force-push branch"
	);
}

#[tokio::test]
async fn push_with_pending_posts_commit_and_syncs_local() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let file = root.child("Cargo.toml");
	std::fs::write(file.as_path(), b"[package]").unwrap();

	// File exists at HEAD → should be classified `update`.
	let runner = git_runner_with_head_paths("abc123", "main", &["Cargo.toml"]);

	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let post_mock = expect_commit_post(&server, |w| {
		w.body_includes("branch=main")
			.body_includes("start_sha=abc123")
			.body_includes("force=false")
			.body_includes("commit_message=ci%28release%29%3A+version+packages")
			.body_includes("actions%5B%5D%5Baction%5D=update")
			.body_includes("actions%5B%5D%5Bfile_path%5D=Cargo.toml")
			.body_includes("actions%5B%5D%5Bencoding%5D=base64")
			.body_excludes("author_email")
			.body_excludes("author_name")
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	seed_pending(&dec, "main", "abc123", &[file.into_path_buf()]).await;
	dec.push().await.unwrap();

	post_mock.assert();
	assert_fetch_and_reset(&runner.invocations(), "main");
	assert!(dec.state.lock().await.pending.is_none());
}

/// Builds a mock for the GitLab create-commit endpoint and lets the caller
/// layer extra `when` matchers (body assertions) on top.
fn expect_commit_post<'s>(
	server: &'s MockServer,
	customise: impl FnOnce(httpmock::When) -> httpmock::When,
) -> httpmock::Mock<'s> {
	server.mock(|when, then| {
		let base = when
			.method(httpmock::Method::POST)
			.path(commit_endpoint_path());
		customise(base);
		then.status(201).json_body(json!({ "id": "newsha" }));
	})
}

async fn seed_pending(
	dec: &GitLabSignedCommit,
	branch: &str,
	parent_sha: &str,
	paths: &[std::path::PathBuf],
) {
	let mut state = dec.state.lock().await;
	state.pending = Some(PendingCommit {
		branch: branch.to_string(),
		parent_sha: parent_sha.to_string(),
		message: "ci(release): version packages".to_string(),
		paths: paths.to_vec(),
	});
}

fn assert_fetch_and_reset(calls: &[crate::command::test_support::Invocation], branch: &str) {
	assert!(
		calls
			.iter()
			.any(|c| c.program == "git" && c.args == ["fetch", "origin", branch]),
		"expected `git fetch origin {branch}`"
	);
	assert!(
		calls
			.iter()
			.any(|c| c.program == "git" && c.args == ["reset", "--hard", "FETCH_HEAD"]),
		"expected `git reset --hard FETCH_HEAD`"
	);
}

#[tokio::test]
async fn push_classifies_create_when_path_absent_at_head() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let file = root.child("CHANGELOG.md");
	std::fs::write(file.as_path(), b"# Changelog").unwrap();

	// Default exit 0 so the post-API `git fetch` / `git reset --hard` succeed;
	// override `cat-file -e HEAD:CHANGELOG.md` to exit 128 to signal absent.
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args(
		"git",
		vec![
			"cat-file".to_string(),
			"-e".to_string(),
			"HEAD:CHANGELOG.md".to_string(),
		],
		128,
	));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let post_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path())
			.body_includes("actions%5B%5D%5Baction%5D=create")
			.body_includes("actions%5B%5D%5Bfile_path%5D=CHANGELOG.md");
		then.status(201).json_body(json!({ "id": "newsha" }));
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			parent_sha: "abc123".to_string(),
			message: "ci".to_string(),
			paths: vec![file.into_path_buf()],
		});
	}
	dec.push().await.unwrap();
	post_mock.assert();
}

#[tokio::test]
async fn push_classifies_delete_when_file_absent_from_disk() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let deleted = root.child(".cursus/some-changeset.md");
	std::fs::create_dir_all(deleted.as_path().parent().unwrap()).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let post_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path())
			.body_includes("actions%5B%5D%5Baction%5D=delete")
			// delete payload carries no content/encoding fields.
			.body_excludes("actions%5B%5D%5Bencoding%5D");
		then.status(201).json_body(json!({ "id": "newsha" }));
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			parent_sha: "abc123".to_string(),
			message: "ci".to_string(),
			paths: vec![deleted.into_path_buf()],
		});
	}
	dec.push().await.unwrap();
	post_mock.assert();
}

#[tokio::test]
async fn force_push_branch_sends_force_true_and_uses_arg_branch() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let post_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path())
			.body_includes("branch=release-branch")
			.body_includes("force=true")
			.body_includes("start_sha=abc123");
		then.status(201).json_body(json!({ "id": "newsha" }));
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			parent_sha: "abc123".to_string(),
			message: "ci".to_string(),
			paths: vec![],
		});
	}

	dec.force_push_branch("release-branch").await.unwrap();
	post_mock.assert();
}

#[tokio::test]
async fn push_with_pending_clears_pending_even_on_api_error() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path());
		then.status(422).body("unprocessable entity");
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			parent_sha: "abc".to_string(),
			message: "ci".to_string(),
			paths: vec![],
		});
	}

	let result = dec.push().await;
	assert!(result.is_err(), "expected error on 422");
	assert!(dec.state.lock().await.pending.is_none());
}

#[tokio::test]
async fn fetch_stderr_credentials_are_redacted() {
	// After a successful API commit, the post-push `git fetch origin <branch>`
	// can fail with a stderr that echoes the credentialed remote URL. The
	// decorator must pass that stderr through `redact_credentials` before
	// embedding it in the resulting `anyhow::Error`.
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stderr(
		"git",
		vec![
			"fetch".to_string(),
			"origin".to_string(),
			"main".to_string(),
		],
		1,
		b"fatal: unable to access 'https://x-access-token:glpat_secret@gitlab.example.com/acme/app.git/': boom".to_vec(),
	));

	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path());
		then.status(201).json_body(json!({ "id": "newsha" }));
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			parent_sha: "abc".to_string(),
			message: "ci".to_string(),
			paths: vec![],
		});
	}

	let err = dec.push().await.expect_err("push should fail on fetch");
	let msg = format!("{err:#}");
	assert!(
		!msg.contains("glpat_secret"),
		"raw credential leaked: {msg}"
	);
	assert!(
		msg.contains("[REDACTED]"),
		"credentials not redacted: {msg}"
	);
}

#[tokio::test]
async fn reset_stderr_credentials_are_redacted() {
	// After a successful API commit and fetch, the `git reset --hard FETCH_HEAD`
	// can also fail with a stderr that echoes a credentialed remote URL. The
	// decorator must pass that stderr through `redact_credentials` too.
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stderr(
		"git",
		vec![
			"reset".to_string(),
			"--hard".to_string(),
			"FETCH_HEAD".to_string(),
		],
		1,
		b"fatal: working tree refers to https://x-access-token:glpat_secret@gitlab.example.com/acme/app.git/: boom".to_vec(),
	));

	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path());
		then.status(201).json_body(json!({ "id": "newsha" }));
	});

	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			parent_sha: "abc".to_string(),
			message: "ci".to_string(),
			paths: vec![],
		});
	}

	let err = dec.push().await.expect_err("push should fail on reset");
	let msg = format!("{err:#}");
	assert!(
		!msg.contains("glpat_secret"),
		"raw credential leaked: {msg}"
	);
	assert!(
		msg.contains("[REDACTED]"),
		"credentials not redacted: {msg}"
	);
}

#[tokio::test]
async fn dry_run_runner_suppresses_fetch_and_reset() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let inner_runner = Arc::new(RecordingCommandRunner::new(0));
	let dry_runner = Arc::new(DryRunCommandRunner::new(
		Arc::clone(&inner_runner) as Arc<dyn CommandRunner>
	));

	let git = make_git(Arc::clone(&dry_runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let post_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path(commit_endpoint_path());
		then.status(201).json_body(json!({ "id": "newsha" }));
	});

	// dry_run=false so the API call is allowed, but DryRunCommandRunner
	// must suppress the post-API fetch/reset shell calls.
	let dec = GitLabSignedCommit::new(
		git,
		Arc::new(LocalFilesystem),
		make_async_gitlab(&server).await,
		Arc::clone(&dry_runner) as Arc<dyn CommandRunner>,
		make_project(),
		false,
	);
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			parent_sha: "abc".to_string(),
			message: "ci".to_string(),
			paths: vec![],
		});
	}

	dec.push().await.unwrap();
	post_mock.assert();
	assert!(
		inner_runner.invocations().is_empty(),
		"DryRunCommandRunner must suppress fetch and reset"
	);
}

// ── Delegation tests ─────────────────────────────────────────────────────────

async fn make_delegating_decorator(
	runner: Arc<dyn CommandRunner>,
) -> (tempfile::TempDir, GitLabSignedCommit) {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let git = make_git(Arc::clone(&runner), &root);
	let server = MockServer::start();
	let dec = make_decorator(git, make_async_gitlab(&server).await, runner).await;
	(dir, dec)
}

#[tokio::test]
async fn path_delegates_to_inner() {
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let (dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert_eq!(dec.path().as_path(), dir.path());
}

#[tokio::test]
async fn head_sha_delegates_to_inner() {
	let runner = git_runner("deadbeef", "main");
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert_eq!(dec.head_sha().await.unwrap(), "deadbeef");
}

#[tokio::test]
async fn path_exists_at_head_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args(
		"git",
		vec![
			"cat-file".to_string(),
			"-e".to_string(),
			"HEAD:foo.txt".to_string(),
		],
		0,
	));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert!(dec.path_exists_at_head(Path::new("foo.txt")).await.unwrap());
}

#[tokio::test]
async fn is_dirty_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec!["status".to_string(), "--porcelain".to_string()],
		0,
		b" M foo\n".to_vec(),
	));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert!(dec.is_dirty().await.unwrap());
}

#[tokio::test]
async fn current_branch_delegates_to_inner() {
	let runner = git_runner("abc", "feature/foo");
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert_eq!(
		dec.current_branch().await.unwrap().as_deref(),
		Some("feature/foo")
	);
}

#[tokio::test]
async fn tag_exists_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec![
			"rev-parse".to_string(),
			"--verify".to_string(),
			"refs/tags/v1.0.0".to_string(),
		],
		0,
		b"sha\n".to_vec(),
	));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert!(dec.tag_exists("v1.0.0").await.unwrap());
}

#[tokio::test]
async fn remote_origin_url_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec![
			"remote".to_string(),
			"get-url".to_string(),
			"origin".to_string(),
		],
		0,
		b"git@gitlab.com:acme/app.git\n".to_vec(),
	));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert_eq!(
		dec.remote_origin_url().await.unwrap().as_deref(),
		Some("git@gitlab.com:acme/app.git"),
	);
}

#[tokio::test]
async fn rev_list_count_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec![
			"rev-list".to_string(),
			"--count".to_string(),
			"origin/main..HEAD".to_string(),
		],
		0,
		b"5\n".to_vec(),
	));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert_eq!(dec.rev_list_count("origin/main..HEAD").await.unwrap(), 5);
}

#[tokio::test]
async fn log_message_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec![
			"log".to_string(),
			"-1".to_string(),
			"--format=%B".to_string(),
			"HEAD".to_string(),
		],
		0,
		b"hello\n".to_vec(),
	));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert_eq!(dec.log_message("HEAD").await.unwrap().trim(), "hello");
}

#[tokio::test]
async fn log_subject_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec![
			"log".to_string(),
			"-1".to_string(),
			"--format=%s".to_string(),
			"HEAD".to_string(),
		],
		0,
		b"subject line\n".to_vec(),
	));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert_eq!(
		dec.log_subject("HEAD").await.unwrap().trim(),
		"subject line"
	);
}

#[tokio::test]
async fn diff_tree_names_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec![
			"diff-tree".to_string(),
			"--no-commit-id".to_string(),
			"-r".to_string(),
			"--name-only".to_string(),
			"HEAD".to_string(),
		],
		0,
		b"a.txt\nb.txt\n".to_vec(),
	));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	assert_eq!(
		dec.diff_tree_names("HEAD").await.unwrap(),
		vec!["a.txt".to_string(), "b.txt".to_string()],
	);
}

#[tokio::test]
async fn diff_names_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_stdout("git", 0, b"x.md\n".to_vec()));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	let out = dec.diff_names(&["--name-only"]).await.unwrap();
	assert_eq!(out, vec!["x.md".to_string()]);
}

#[tokio::test]
async fn log_added_commit_delegates_to_inner() {
	let runner =
		Arc::new(DispatchingCommandRunner::new(0).on_stdout("git", 0, b"shaaaa\n".to_vec()));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	let result: Option<String> = dec.log_added_commit(Path::new("README.md")).await.unwrap();
	assert_eq!(result.as_deref(), Some("shaaaa"));
}

#[tokio::test]
async fn tag_delegates_to_inner() {
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	dec.tag("v1.0.0", "rel").await.unwrap();
	let calls = runner.invocations();
	assert!(
		calls
			.iter()
			.any(|c| c.program == "git" && c.args.contains(&"tag".to_string())),
		"expected inner git tag invocation",
	);
}

#[tokio::test]
async fn checkout_delegates_to_inner() {
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	dec.checkout("main").await.unwrap();
	let calls = runner.invocations();
	assert!(
		calls
			.iter()
			.any(|c| c.program == "git" && c.args.contains(&"checkout".to_string())),
		"expected inner git checkout invocation",
	);
}

#[tokio::test]
async fn checkout_or_reset_branch_delegates_to_inner() {
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	dec.checkout_or_reset_branch("release").await.unwrap();
	let calls = runner.invocations();
	assert!(
		calls.iter().any(|c| c.program == "git"),
		"expected inner git invocation",
	);
}

#[tokio::test]
async fn delete_tag_delegates_to_inner() {
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	dec.delete_tag("v1.0.0").await.unwrap();
	let calls = runner.invocations();
	assert!(
		calls.iter().any(|c| c.program == "git"
			&& c.args.contains(&"tag".to_string())
			&& c.args.contains(&"-d".to_string())),
		"expected inner git tag -d invocation",
	);
}

#[tokio::test]
async fn push_tag_delegates_to_inner() {
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let (_dir, dec) =
		make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>).await;
	dec.push_tag("v1.0.0").await.unwrap();
	let calls = runner.invocations();
	assert!(
		calls
			.iter()
			.any(|c| c.program == "git" && c.args.contains(&"push".to_string())),
		"expected inner git push invocation",
	);
}

#[tokio::test]
async fn debug_impl_shows_project_and_dry_run() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let dec = make_decorator(
		git,
		make_async_gitlab(&server).await,
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	)
	.await;
	let formatted = format!("{dec:?}");
	assert!(formatted.contains("acme"));
	assert!(formatted.contains("app"));
	assert!(formatted.contains("dry_run"));
}
