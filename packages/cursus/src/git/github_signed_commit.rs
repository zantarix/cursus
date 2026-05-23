//! [`GitHubSignedCommit`] decorator that creates verified commits via the GitHub Git Data API.
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
use crate::redact::redact_credentials;

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

pub(crate) struct PendingCommit {
	pub(crate) branch: String,
	pub(crate) sha: String,
}

pub(crate) struct State {
	pub(crate) staged: Vec<PathBuf>,
	pub(crate) pending: Option<PendingCommit>,
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
pub struct GitHubSignedCommit {
	inner: Arc<dyn Git>,
	fs: Arc<dyn Filesystem>,
	octocrab: Arc<Octocrab>,
	runner: Arc<dyn CommandRunner>,
	owner: String,
	repo: String,
	dry_run: bool,
	pub(crate) state: Mutex<State>,
}

impl std::fmt::Debug for GitHubSignedCommit {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GitHubSignedCommit")
			.field("owner", &self.owner)
			.field("repo", &self.repo)
			.field("dry_run", &self.dry_run)
			.finish_non_exhaustive()
	}
}

impl GitHubSignedCommit {
	/// Creates a new `GitHubSignedCommit` decorator.
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
			let raw = String::from_utf8_lossy(&fetch.stderr);
			let stderr = redact_credentials(&raw);
			anyhow::bail!("git fetch origin {branch} failed: {stderr}");
		}

		let reset = self
			.runner
			.run_mut("git", &["reset", "--hard", "FETCH_HEAD"], cwd)
			.await
			.context("git reset failed after API ref update")?;
		if !reset.status.success() {
			let raw = String::from_utf8_lossy(&reset.stderr);
			let stderr = redact_credentials(&raw);
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
		anyhow::bail!(
			"GitHub API returned {status}: {}",
			redact_credentials(&text)
		);
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
			anyhow::bail!(
				"GitHub API returned {status}: {}",
				redact_credentials(&text)
			);
		}
		return Ok(());
	}
	anyhow::bail!(
		"GitHub API returned {status}: {}",
		redact_credentials(&text)
	);
}

// ── Git trait impl ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
#[async_trait]
impl Git for GitHubSignedCommit {
	fn path(&self) -> &AbsolutePath {
		self.inner.path()
	}

	async fn head_sha(&self) -> anyhow::Result<String> {
		self.inner.head_sha().await
	}

	async fn path_exists_at_head(&self, path: &Path) -> anyhow::Result<bool> {
		self.inner.path_exists_at_head(path).await
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
