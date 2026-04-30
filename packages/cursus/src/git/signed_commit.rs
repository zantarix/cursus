//! [`SignedCommitGit`] decorator that creates verified commits via the GitHub Git Data API.
//!
//! See ADR-050 for the full design rationale.

use std::path::{Path, PathBuf};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

// Encodes characters that would corrupt a URL path segment: space, ?, #, %.
// Control characters are already rejected by validate_branch_name.
// `/` is intentionally left unencoded — hierarchical refs like
// `cursus-release/main` map correctly to the GitHub API path.
// Unicode bytes are encoded by utf8_percent_encode automatically.
const BRANCH_PATH: &AsciiSet = &CONTROLS.add(b' ').add(b'?').add(b'#').add(b'%');
use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use octocrab::Octocrab;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::command::CommandRunner;
use crate::filesystem::Filesystem;
use crate::git::Git;
use crate::git::ref_format::validate_branch_name;
use crate::path::AbsolutePath;

// ── GitHub API request / response types ──────────────────────────────────────

#[derive(Serialize)]
struct CreateBlobRequest<'a> {
	content: &'a str,
	encoding: &'a str,
}

#[derive(Deserialize)]
struct CreateBlobResponse {
	sha: String,
}

#[derive(Serialize)]
struct TreeItem {
	path: String,
	mode: &'static str,
	r#type: &'static str,
	sha: Option<String>,
}

#[derive(Serialize)]
struct CreateTreeRequest<'a> {
	base_tree: &'a str,
	tree: Vec<TreeItem>,
}

#[derive(Deserialize)]
struct CreateTreeResponse {
	sha: String,
}

#[derive(Serialize)]
struct CreateCommitRequest<'a> {
	message: &'a str,
	tree: &'a str,
	parents: Vec<&'a str>,
}

#[derive(Deserialize)]
struct CreateCommitResponse {
	sha: String,
}

#[derive(Serialize)]
struct UpdateRefRequest<'a> {
	sha: &'a str,
	force: bool,
}

#[derive(Serialize)]
struct CreateRefRequest<'a> {
	r#ref: &'a str,
	sha: &'a str,
}

// ── Internal state ────────────────────────────────────────────────────────────

struct PendingCommit {
	branch: String,
	sha: String,
}

struct State {
	staged: Vec<PathBuf>,
	pending: Option<PendingCommit>,
}

// ── Decorator ─────────────────────────────────────────────────────────────────

/// A [`Git`] decorator that produces Verified commits via the GitHub Git Data API.
///
/// When [`Git::commit`] is called, blobs, a tree, and a commit object are created
/// through GitHub's REST API. Omitting `author` and `committer` from the commit
/// request causes GitHub to fill them with the App's bot identity and sign the commit
/// with the web-flow GPG key, producing a Verified commit with no long-lived key
/// custody (ADR-050).
///
/// [`Git::push`] and [`Git::force_push_branch`] update the remote branch ref via
/// `PATCH /repos/{owner}/{repo}/git/refs/heads/{branch}` (using `force: false` and
/// `force: true` respectively), then run `git fetch origin {branch}` and
/// `git reset --hard FETCH_HEAD` to download the new commit object into the local
/// object store and sync the local branch ref, index, and working tree.
///
/// All other [`Git`] methods delegate to the wrapped inner implementation.
pub struct SignedCommitGit {
	inner: Arc<dyn Git>,
	fs: Arc<dyn Filesystem>,
	octocrab: Arc<Octocrab>,
	runner: Arc<dyn CommandRunner>,
	owner: String,
	repo: String,
	dry_run: bool,
	state: Mutex<State>,
}

impl std::fmt::Debug for SignedCommitGit {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("SignedCommitGit")
			.field("owner", &self.owner)
			.field("repo", &self.repo)
			.field("dry_run", &self.dry_run)
			.finish_non_exhaustive()
	}
}

impl SignedCommitGit {
	/// Creates a new `SignedCommitGit` decorator.
	///
	/// `runner` is used for the post-push `git fetch` / `git reset --hard` operations
	/// that sync the local working tree after a remote ref update.
	pub fn new(
		inner: Arc<dyn Git>,
		fs: Arc<dyn Filesystem>,
		octocrab: Arc<Octocrab>,
		runner: Arc<dyn CommandRunner>,
		owner: String,
		repo: String,
		dry_run: bool,
	) -> Self {
		Self {
			inner,
			fs,
			octocrab,
			runner,
			owner,
			repo,
			dry_run,
			state: Mutex::new(State {
				staged: Vec::new(),
				pending: None,
			}),
		}
	}

	async fn patch_ref_and_sync(&self, branch: &str, sha: &str, force: bool) -> anyhow::Result<()> {
		validate_branch_name(branch)?;
		upsert_branch_ref(&self.octocrab, &self.owner, &self.repo, branch, sha, force)
			.await
			.with_context(|| format!("failed to update remote ref for branch '{branch}'"))?;

		let cwd = self.inner.path().as_path();
		let fetch = self
			.runner
			.run_mut("git", &["fetch", "origin", branch], cwd)
			.await
			.context("git fetch failed after API ref update")?;
		if !fetch.status.success() {
			let stderr = String::from_utf8_lossy(&fetch.stderr);
			anyhow::bail!("git fetch origin {branch} failed: {stderr}");
		}

		let reset = self
			.runner
			.run_mut("git", &["reset", "--hard", "FETCH_HEAD"], cwd)
			.await
			.context("git reset failed after API ref update")?;
		if !reset.status.success() {
			let stderr = String::from_utf8_lossy(&reset.stderr);
			anyhow::bail!("git reset --hard FETCH_HEAD failed: {stderr}");
		}

		Ok(())
	}
}

// ── Low-level octocrab helpers ────────────────────────────────────────────────

async fn api_post<P: Serialize + ?Sized, R: serde::de::DeserializeOwned>(
	client: &Octocrab,
	url: String,
	body: &P,
) -> anyhow::Result<R> {
	let response = client
		._post(url, Some(body))
		.await
		.context("GitHub API POST failed")?;
	let status = response.status();
	let text = client
		.body_to_string(response)
		.await
		.context("failed to read GitHub API response body")?;
	if !status.is_success() {
		anyhow::bail!("GitHub API returned {status}: {text}");
	}
	serde_json::from_str(&text).context("failed to deserialize GitHub API response")
}

async fn build_tree_items(
	client: &Octocrab,
	owner: &str,
	repo: &str,
	fs: &dyn Filesystem,
	staged: &[PathBuf],
	repo_root: &AbsolutePath,
) -> anyhow::Result<Vec<TreeItem>> {
	let mut items: Vec<TreeItem> = Vec::with_capacity(staged.len());
	for path in staged {
		let abs = AbsolutePath::new(path)
			.with_context(|| format!("staged path is not absolute: {}", path.display()))?;
		let rel = path.strip_prefix(repo_root.as_path()).with_context(|| {
			format!(
				"staged path {} is not under repo root {}",
				path.display(),
				repo_root.as_path().display()
			)
		})?;
		let rel_str = rel.to_string_lossy().into_owned();

		if fs.exists(&abs).await? {
			let bytes = fs.read(&abs).await?;
			let content = String::from_utf8(bytes).with_context(|| {
				format!(
					"staged file {} contains non-UTF-8 bytes and cannot be committed via \
					 the GitHub API in UTF-8 mode; set `[git].signed_commits = \"off\"` to \
					 use the local git binary instead",
					rel.display()
				)
			})?;
			let blob_req = CreateBlobRequest {
				content: &content,
				encoding: "utf-8",
			};
			let url = format!("/repos/{owner}/{repo}/git/blobs");
			let blob: CreateBlobResponse = api_post(client, url, &blob_req)
				.await
				.with_context(|| format!("failed to create blob for {}", rel.display()))?;
			items.push(TreeItem {
				path: rel_str,
				mode: "100644",
				r#type: "blob",
				sha: Some(blob.sha),
			});
		} else {
			// File was deleted (e.g. a consumed changeset) — null sha removes it.
			items.push(TreeItem {
				path: rel_str,
				mode: "100644",
				r#type: "blob",
				sha: None,
			});
		}
	}
	Ok(items)
}

/// Updates or creates a branch ref on the remote.
///
/// Tries `PATCH /git/refs/heads/{branch}` first. If GitHub returns 422
/// "Reference does not exist" (the branch is new), falls back to
/// `POST /git/refs` to create it. This handles both the initial prepare run
/// (branch does not exist on remote yet) and subsequent runs (branch already
/// exists and must be force-updated).
async fn upsert_branch_ref(
	client: &Octocrab,
	owner: &str,
	repo: &str,
	branch: &str,
	sha: &str,
	force: bool,
) -> anyhow::Result<()> {
	let encoded_branch = utf8_percent_encode(branch, BRANCH_PATH);
	let patch_url = format!("/repos/{owner}/{repo}/git/refs/heads/{encoded_branch}");
	let update_req = UpdateRefRequest { sha, force };
	let response = client
		._patch(patch_url, Some(&update_req))
		.await
		.context("GitHub API PATCH failed")?;
	let status = response.status();
	if status.is_success() {
		return Ok(());
	}
	let text = client.body_to_string(response).await.unwrap_or_default();
	// 422 "Reference does not exist" means the branch is new — create it instead.
	if status.as_u16() == 422 && text.contains("Reference does not exist") {
		let full_ref = format!("refs/heads/{branch}");
		let create_req = CreateRefRequest {
			r#ref: &full_ref,
			sha,
		};
		let post_url = format!("/repos/{owner}/{repo}/git/refs");
		let response = client
			._post(post_url, Some(&create_req))
			.await
			.context("GitHub API POST failed")?;
		let status = response.status();
		if !status.is_success() {
			let text = client.body_to_string(response).await.unwrap_or_default();
			anyhow::bail!("GitHub API returned {status}: {text}");
		}
		return Ok(());
	}
	anyhow::bail!("GitHub API returned {status}: {text}");
}

// ── Git trait impl ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
#[async_trait]
impl Git for SignedCommitGit {
	fn path(&self) -> &AbsolutePath {
		self.inner.path()
	}

	async fn head_sha(&self) -> anyhow::Result<String> {
		self.inner.head_sha().await
	}

	async fn is_dirty(&self) -> anyhow::Result<bool> {
		self.inner.is_dirty().await
	}

	async fn current_branch(&self) -> anyhow::Result<Option<String>> {
		self.inner.current_branch().await
	}

	async fn tag_exists(&self, tag: &str) -> anyhow::Result<bool> {
		self.inner.tag_exists(tag).await
	}

	async fn remote_origin_url(&self) -> anyhow::Result<Option<String>> {
		self.inner.remote_origin_url().await
	}

	async fn rev_list_count(&self, range: &str) -> anyhow::Result<usize> {
		self.inner.rev_list_count(range).await
	}

	async fn log_message(&self, rev: &str) -> anyhow::Result<String> {
		self.inner.log_message(rev).await
	}

	async fn log_subject(&self, rev: &str) -> anyhow::Result<String> {
		self.inner.log_subject(rev).await
	}

	async fn log_added_commit(&self, path: &Path) -> anyhow::Result<Option<String>> {
		self.inner.log_added_commit(path).await
	}

	async fn diff_tree_names(&self, commit: &str) -> anyhow::Result<Vec<String>> {
		self.inner.diff_tree_names(commit).await
	}

	async fn diff_names(&self, extra_args: &[&str]) -> anyhow::Result<Vec<String>> {
		self.inner.diff_names(extra_args).await
	}

	/// Stages files via the inner impl and records their paths for the API commit.
	async fn add(&self, files: &[PathBuf]) -> anyhow::Result<()> {
		self.inner.add(files).await?;
		let mut state = self.state.lock().await;
		state.staged.extend_from_slice(files);
		Ok(())
	}

	/// Creates a commit via the GitHub Git Data API.
	///
	/// Omitting `author` and `committer` from the request causes GitHub to sign
	/// the commit with the web-flow GPG key when authenticated via a GitHub App
	/// installation token.
	///
	/// In dry-run mode, logs the intended action and returns without making any
	/// API calls (explicit guard required because octocrab is not a
	/// [`CommandRunner`] and is not intercepted by [`crate::command::DryRunCommandRunner`]).
	async fn commit(&self, message: &str) -> anyhow::Result<()> {
		if self.dry_run {
			log::info!("(dry-run) would create API commit: {message}");
			return Ok(());
		}

		let staged = {
			let state = self.state.lock().await;
			if state.staged.is_empty() {
				return Ok(());
			}
			// Dedup while preserving order: a path may appear more than once if
			// `add()` is called multiple times with overlapping file lists.
			let mut seen = std::collections::HashSet::new();
			state
				.staged
				.iter()
				.filter(|p| seen.insert(p.as_path()))
				.cloned()
				.collect::<Vec<_>>()
		};

		let head = self.inner.head_sha().await?;
		let branch = self
			.inner
			.current_branch()
			.await?
			.context("cannot create API commit with a detached HEAD")?;
		let repo_root = self.inner.path();

		let tree_items = build_tree_items(
			&self.octocrab,
			&self.owner,
			&self.repo,
			&*self.fs,
			&staged,
			repo_root,
		)
		.await?;

		let tree_req = CreateTreeRequest {
			base_tree: &head,
			tree: tree_items,
		};
		let url = format!("/repos/{}/{}/git/trees", self.owner, self.repo);
		let tree_resp: CreateTreeResponse = api_post(&self.octocrab, url, &tree_req)
			.await
			.context("failed to create git tree")?;

		let commit_req = CreateCommitRequest {
			message,
			tree: &tree_resp.sha,
			parents: vec![&head],
		};
		let url = format!("/repos/{}/{}/git/commits", self.owner, self.repo);
		let commit_resp: CreateCommitResponse = api_post(&self.octocrab, url, &commit_req)
			.await
			.context("failed to create git commit")?;

		log::info!("Created API commit {}", commit_resp.sha);

		let mut state = self.state.lock().await;
		state.pending = Some(PendingCommit {
			branch,
			sha: commit_resp.sha,
		});
		state.staged.clear();

		Ok(())
	}

	async fn tag(&self, tag_name: &str, message: &str) -> anyhow::Result<()> {
		self.inner.tag(tag_name, message).await
	}

	/// Pushes to origin (non-forced) and syncs the local git state.
	///
	/// If a commit was made via the API, updates the remote ref with `force: false`
	/// and runs `git fetch` + `git reset --hard FETCH_HEAD` to bring the verified
	/// commit object into the local object store. Falls back to the inner push
	/// otherwise.
	async fn push(&self) -> anyhow::Result<()> {
		let pending = self.state.lock().await.pending.take();
		match pending {
			Some(p) => self.patch_ref_and_sync(&p.branch, &p.sha, false).await,
			None => self.inner.push().await,
		}
	}

	async fn checkout(&self, branch: &str) -> anyhow::Result<()> {
		self.inner.checkout(branch).await
	}

	async fn checkout_or_reset_branch(&self, branch: &str) -> anyhow::Result<()> {
		self.inner.checkout_or_reset_branch(branch).await
	}

	/// Force-pushes a named branch to origin and syncs the local git state.
	///
	/// If a commit was made via the API, updates the remote ref with `force: true`
	/// and runs `git fetch` + `git reset --hard FETCH_HEAD`. Falls back to the
	/// inner force-push otherwise.
	async fn force_push_branch(&self, branch: &str) -> anyhow::Result<()> {
		let pending = self.state.lock().await.pending.take();
		match pending {
			Some(p) => self.patch_ref_and_sync(branch, &p.sha, true).await,
			None => self.inner.force_push_branch(branch).await,
		}
	}

	async fn delete_tag(&self, tag: &str) -> anyhow::Result<()> {
		self.inner.delete_tag(tag).await
	}

	async fn push_tag(&self, tag: &str) -> anyhow::Result<()> {
		self.inner.push_tag(tag).await
	}
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use httpmock::MockServer;
	use octocrab::Octocrab;
	use serde_json::json;

	use crate::command::test_support::{DispatchingCommandRunner, RecordingCommandRunner};
	use crate::command::{CommandRunner, DryRunCommandRunner};
	use crate::filesystem::LocalFilesystem;
	use crate::git::{Git, GitWorkdir};
	use crate::path::AbsolutePath;

	use super::SignedCommitGit;

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
			state.pending = Some(super::PendingCommit {
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
			state.pending = Some(super::PendingCommit {
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
			state.pending = Some(super::PendingCommit {
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
			state.pending = Some(super::PendingCommit {
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
			state.pending = Some(super::PendingCommit {
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
}
