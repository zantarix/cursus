use std::sync::Arc;

use httpmock::MockServer;
use octocrab::Octocrab;
use serde_json::json;

use crate::command::test_support::{DispatchingCommandRunner, RecordingCommandRunner};
use crate::command::{CommandRunner, DryRunCommandRunner};
use crate::filesystem::LocalFilesystem;
use crate::git::signed_commit::{PendingCommit, SignedCommitGit};
use crate::git::{Git, GitWorkdir};
use crate::path::AbsolutePath;

fn make_git(runner: Arc<dyn CommandRunner>, root: &AbsolutePath) -> Arc<dyn Git> {
	Arc::new(GitWorkdir::new(runner, root.clone()))
}

fn make_octocrab(server: &MockServer) -> Arc<Octocrab> {
	Arc::new(
		Octocrab::builder()
			.base_uri(server.base_url())
			.unwrap()
			.build()
			.unwrap(),
	)
}

fn make_decorator(
	git: Arc<dyn Git>,
	octocrab: Arc<Octocrab>,
	runner: Arc<dyn CommandRunner>,
) -> SignedCommitGit {
	SignedCommitGit::new(
		git,
		Arc::new(LocalFilesystem),
		octocrab,
		runner,
		"owner".to_string(),
		"repo".to_string(),
		false,
	)
}

fn make_decorator_dry_run(
	git: Arc<dyn Git>,
	octocrab: Arc<Octocrab>,
	runner: Arc<dyn CommandRunner>,
) -> SignedCommitGit {
	SignedCommitGit::new(
		git,
		Arc::new(LocalFilesystem),
		octocrab,
		runner,
		"owner".to_string(),
		"repo".to_string(),
		true,
	)
}

/// Returns a [`DispatchingCommandRunner`] pre-configured so that
/// `git rev-parse HEAD` returns `head_sha` and
/// `git rev-parse --abbrev-ref HEAD` returns `branch`.
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
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);

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
	// Register a catch-all mock so we can check it was never hit.
	let catch_all = server.mock(|when, then| {
		when.any_request();
		then.status(200).body("{}");
	});

	let dec = make_decorator_dry_run(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);

	let file = root.child("foo.txt");
	std::fs::write(file.as_path(), b"content").unwrap();
	dec.add(&[file.into_path_buf()]).await.unwrap();
	dec.commit("ci(release): version packages").await.unwrap();

	catch_all.assert_calls(0);
	assert!(dec.state.lock().await.pending.is_none());
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
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);

	dec.commit("ci(release): version packages").await.unwrap();
	catch_all.assert_calls(0);
}

#[tokio::test]
async fn commit_creates_blob_tree_commit_and_stores_pending() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = git_runner("abc123", "main");
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	let blob_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/blobs");
		then.status(201).json_body(json!({ "sha": "blobsha" }));
	});
	let tree_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/trees");
		then.status(201).json_body(json!({ "sha": "treesha" }));
	});
	let commit_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/commits");
		then.status(201).json_body(json!({ "sha": "newsha" }));
	});

	let file = root.child("Cargo.toml");
	std::fs::write(file.as_path(), b"[package]").unwrap();

	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);
	dec.add(&[file.into_path_buf()]).await.unwrap();
	dec.commit("ci(release): version packages").await.unwrap();

	blob_mock.assert();
	tree_mock.assert();
	commit_mock.assert();

	let state = dec.state.lock().await;
	assert!(
		state.staged.is_empty(),
		"staged should be cleared after commit"
	);
	let pending = state.pending.as_ref().expect("pending should be set");
	assert_eq!(pending.sha, "newsha");
	assert_eq!(pending.branch, "main");
}

#[tokio::test]
async fn commit_fails_on_detached_head() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	// head_sha succeeds; current_branch returns None (detached HEAD).
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
				b"HEAD\n".to_vec(), // "HEAD" means detached
			),
	);
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	let file = root.child("foo.txt");
	std::fs::write(file.as_path(), b"content").unwrap();

	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);
	dec.add(&[file.into_path_buf()]).await.unwrap();
	let result = dec.commit("ci(release): version packages").await;

	assert!(result.is_err());
	assert!(
		result.unwrap_err().to_string().contains("detached HEAD"),
		"expected detached HEAD error"
	);
}

#[tokio::test]
async fn commit_deduplicates_staged_paths() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = git_runner("abc123", "main");
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	// One blob POST expected even though the file is staged twice.
	let blob_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/blobs");
		then.status(201).json_body(json!({ "sha": "blobsha" }));
	});
	server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/trees");
		then.status(201).json_body(json!({ "sha": "treesha" }));
	});
	server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/commits");
		then.status(201).json_body(json!({ "sha": "newsha" }));
	});

	let file = root.child("Cargo.toml");
	std::fs::write(file.as_path(), b"[package]").unwrap();

	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);
	// Add the same file twice.
	dec.add(&[file.as_path().to_path_buf()]).await.unwrap();
	dec.add(&[file.into_path_buf()]).await.unwrap();
	dec.commit("ci(release): version packages").await.unwrap();

	blob_mock.assert_calls(1);
}

#[tokio::test]
async fn commit_handles_deleted_file() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = git_runner("abc123", "main");
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	// Deleted file should not trigger a blob POST.
	let blob_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/blobs");
		then.status(201).json_body(json!({ "sha": "blobsha" }));
	});
	let tree_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/trees");
		then.status(201).json_body(json!({ "sha": "treesha" }));
	});
	let commit_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/commits");
		then.status(201).json_body(json!({ "sha": "newsha" }));
	});

	// Deleted file — directory exists but file does not.
	let deleted = root.child(".cursus/some-changeset.md");
	std::fs::create_dir_all(deleted.as_path().parent().unwrap()).unwrap();

	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);
	dec.add(&[deleted.into_path_buf()]).await.unwrap();
	dec.commit("ci(release): version packages").await.unwrap();

	blob_mock.assert_calls(0);
	tree_mock.assert();
	commit_mock.assert();
}

#[tokio::test]
async fn push_with_pending_patches_ref_and_syncs_local() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	let patch_mock = server.mock(|when, then| {
		when.method(httpmock::Method::PATCH)
			.path("/repos/owner/repo/git/refs/heads/main")
			.json_body(json!({ "sha": "newsha", "force": false }));
		then.status(200).json_body(json!({}));
	});

	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);

	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			sha: "newsha".to_string(),
		});
	}

	dec.push().await.unwrap();

	patch_mock.assert();
	let calls = runner.invocations();
	assert!(
		calls
			.iter()
			.any(|c| c.program == "git" && c.args.contains(&"fetch".to_string())),
		"expected git fetch call"
	);
	assert!(
		calls.iter().any(|c| c.program == "git"
			&& c.args.contains(&"reset".to_string())
			&& c.args.contains(&"FETCH_HEAD".to_string())),
		"expected git reset --hard FETCH_HEAD call"
	);
	assert!(dec.state.lock().await.pending.is_none());
}

#[tokio::test]
async fn force_push_creates_branch_when_ref_does_not_exist() {
	// Simulates the first prepare run where the release branch doesn't exist
	// on the remote yet — PATCH returns 422, so we fall back to POST to create it.
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	// PATCH returns 422 "Reference does not exist".
	let patch_mock = server.mock(|when, then| {
		when.method(httpmock::Method::PATCH)
			.path("/repos/owner/repo/git/refs/heads/cursus-release/main");
		then.status(422)
			.json_body(json!({ "message": "Reference does not exist" }));
	});
	// POST creates the ref.
	let post_mock = server.mock(|when, then| {
		when.method(httpmock::Method::POST)
			.path("/repos/owner/repo/git/refs")
			.json_body(json!({
				"ref": "refs/heads/cursus-release/main",
				"sha": "newsha"
			}));
		then.status(201).json_body(json!({}));
	});

	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "cursus-release/main".to_string(),
			sha: "newsha".to_string(),
		});
	}

	dec.force_push_branch("cursus-release/main").await.unwrap();

	patch_mock.assert_calls(1);
	post_mock.assert_calls(1);
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
		when.any_request();
		then.status(200).body("{}");
	});

	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);

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
async fn force_push_branch_with_pending_patches_ref_force_true() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();

	let patch_mock = server.mock(|when, then| {
		when.method(httpmock::Method::PATCH)
			.path("/repos/owner/repo/git/refs/heads/release-branch")
			.json_body(json!({ "sha": "newsha", "force": true }));
		then.status(200).json_body(json!({}));
	});

	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);

	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			sha: "newsha".to_string(),
		});
	}

	dec.force_push_branch("release-branch").await.unwrap();
	patch_mock.assert();
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
		when.method(httpmock::Method::PATCH)
			.path("/repos/owner/repo/git/refs/heads/main");
		then.status(422).body("unprocessable entity");
	});

	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);
	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			sha: "newsha".to_string(),
		});
	}

	let result = dec.push().await;
	assert!(result.is_err(), "expected error on 422");
	assert!(dec.state.lock().await.pending.is_none());
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

	server.mock(|when, then| {
		when.method(httpmock::Method::PATCH);
		then.status(200).json_body(json!({}));
	});

	let dec = SignedCommitGit::new(
		git,
		Arc::new(LocalFilesystem),
		make_octocrab(&server),
		Arc::clone(&dry_runner) as Arc<dyn CommandRunner>,
		"owner".to_string(),
		"repo".to_string(),
		false,
	);

	{
		let mut state = dec.state.lock().await;
		state.pending = Some(PendingCommit {
			branch: "main".to_string(),
			sha: "newsha".to_string(),
		});
	}

	dec.push().await.unwrap();

	assert!(
		inner_runner.invocations().is_empty(),
		"DryRunCommandRunner should suppress fetch and reset"
	);
}

/// Builds a `SignedCommitGit` whose inner `Git` impl uses the supplied runner.
/// Convenience wrapper for the trait-delegation tests below.
fn make_delegating_decorator(
	runner: Arc<dyn CommandRunner>,
) -> (tempfile::TempDir, SignedCommitGit) {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let git = make_git(Arc::clone(&runner), &root);
	let server = MockServer::start();
	let dec = make_decorator(git, make_octocrab(&server), runner);
	(dir, dec)
}

#[tokio::test]
async fn path_delegates_to_inner() {
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let (dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	assert_eq!(dec.path().as_path(), dir.path());
}

#[tokio::test]
async fn head_sha_delegates_to_inner() {
	let runner = git_runner("deadbeef", "main");
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	assert_eq!(dec.head_sha().await.unwrap(), "deadbeef");
}

#[tokio::test]
async fn is_dirty_delegates_to_inner() {
	// Returning non-empty stdout for `status --porcelain` means dirty.
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
		"git",
		vec!["status".to_string(), "--porcelain".to_string()],
		0,
		b" M foo\n".to_vec(),
	));
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	assert!(dec.is_dirty().await.unwrap());
}

#[tokio::test]
async fn current_branch_delegates_to_inner() {
	let runner = git_runner("abc", "feature/foo");
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
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
			"--quiet".to_string(),
			"refs/tags/v1.0.0".to_string(),
		],
		0,
		b"sha\n".to_vec(),
	));
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
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
		b"git@github.com:owner/repo.git\n".to_vec(),
	));
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	assert_eq!(
		dec.remote_origin_url().await.unwrap().as_deref(),
		Some("git@github.com:owner/repo.git"),
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
		b"3\n".to_vec(),
	));
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	assert_eq!(dec.rev_list_count("origin/main..HEAD").await.unwrap(), 3);
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
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
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
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
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
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	assert_eq!(
		dec.diff_tree_names("HEAD").await.unwrap(),
		vec!["a.txt".to_string(), "b.txt".to_string()],
	);
}

#[tokio::test]
async fn diff_names_delegates_to_inner() {
	let runner = Arc::new(DispatchingCommandRunner::new(0).on_stdout("git", 0, b"x.md\n".to_vec()));
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	let out = dec.diff_names(&["--name-only"]).await.unwrap();
	assert_eq!(out, vec!["x.md".to_string()]);
}

#[tokio::test]
async fn log_added_commit_delegates_to_inner() {
	use std::path::Path;
	let runner =
		Arc::new(DispatchingCommandRunner::new(0).on_stdout("git", 0, b"shaaaa\n".to_vec()));
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
	let result = dec.log_added_commit(Path::new("README.md")).await.unwrap();
	assert_eq!(result.as_deref(), Some("shaaaa"));
}

#[tokio::test]
async fn tag_delegates_to_inner() {
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
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
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
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
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
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
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
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
	let (_dir, dec) = make_delegating_decorator(Arc::clone(&runner) as Arc<dyn CommandRunner>);
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
async fn debug_impl_shows_owner_repo_dry_run() {
	let dir = tempfile::tempdir().unwrap();
	let root = AbsolutePath::new(dir.path()).unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let git = make_git(Arc::clone(&runner) as Arc<dyn CommandRunner>, &root);
	let server = MockServer::start();
	let dec = make_decorator(
		git,
		make_octocrab(&server),
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
	);
	let formatted = format!("{dec:?}");
	assert!(formatted.contains("owner"));
	assert!(formatted.contains("repo"));
	assert!(formatted.contains("dry_run"));
}
