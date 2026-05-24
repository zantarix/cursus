//! [`GitLabSignedCommit`] decorator that creates verified commits via the GitLab commits API.
//!
//! See ADR-058 for the full design rationale. GitLab 18.10+ SSH-signs commits
//! created through `POST /projects/:id/repository/commits` with the instance's
//! web-commits key when `author_email`/`author_name` are omitted from the
//! request body, producing a Verified commit with no long-lived key custody.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use async_trait::async_trait;
use gitlab::AsyncGitlab;
use gitlab::api::projects::repository::commits::{CommitAction, CommitActionType, CreateCommit};
use gitlab::api::projects::repository::files::Encoding;
use gitlab::api::projects::repository::tags::CreateTag;
use gitlab::api::{ApiError, AsyncQuery};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::command::CommandRunner;
use crate::filesystem::Filesystem;
use crate::forge::gitlab::GitLabProject;
use crate::git::Git;
use crate::git::ref_format::{validate_branch_name, validate_tag_name};
use crate::path::AbsolutePath;
use crate::redact::redact_credentials;

// ── Internal state ────────────────────────────────────────────────────────────

/// A staged commit awaiting `push()` or `force_push_branch()` to flush it
/// to the GitLab API. Carries the message and the deduplicated staged path
/// list rather than a SHA, because GitLab fuses commit creation and ref
/// update into a single API call.
pub(crate) struct PendingCommit {
	pub(crate) branch: String,
	pub(crate) parent_sha: String,
	pub(crate) message: String,
	pub(crate) paths: Vec<PathBuf>,
}

/// A tag awaiting `push_tag()` to flush it to the GitLab Tags API. Records the
/// target commit SHA (HEAD at `tag()` time — the release commit, already on the
/// remote) and the annotation message, rather than creating a local `git tag`
/// object that the API path would never transfer.
pub(crate) struct PendingTag {
	pub(crate) sha: String,
	pub(crate) message: String,
}

pub(crate) struct State {
	pub(crate) staged: Vec<PathBuf>,
	pub(crate) pending: Option<PendingCommit>,
	pub(crate) tags: HashMap<String, PendingTag>,
}

// ── Decorator ─────────────────────────────────────────────────────────────────

/// A [`Git`] decorator that produces Verified commits via the GitLab commits API.
///
/// When [`Git::commit`] is called, the staged paths and message are recorded
/// in pending state — no API call is made yet. When [`Git::push`] or
/// [`Git::force_push_branch`] is called, the decorator builds a
/// `POST /projects/{group/project}/repository/commits` payload (one action
/// per staged path, classified as `create`/`update`/`delete` against the
/// HEAD tree) and issues the request through the shared [`AsyncGitlab`]
/// client. Omitting `author_email` and `author_name` from the payload causes
/// GitLab to fill them with the authenticated user and SSH-sign the commit
/// with the instance's web-commits key, producing a Verified commit with no
/// long-lived key custody (ADR-058).
///
/// After the API call returns, `git fetch origin {branch}` and
/// `git reset --hard FETCH_HEAD` are run via [`CommandRunner`] to bring the
/// new commit object into the local object store and sync the local branch
/// ref, index, and working tree.
///
/// All other [`Git`] methods delegate to the wrapped inner implementation.
pub struct GitLabSignedCommit {
	inner: Arc<dyn Git>,
	fs: Arc<dyn Filesystem>,
	client: Arc<AsyncGitlab>,
	runner: Arc<dyn CommandRunner>,
	project: GitLabProject,
	dry_run: bool,
	pub(crate) state: Mutex<State>,
}

impl std::fmt::Debug for GitLabSignedCommit {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GitLabSignedCommit")
			.field("project", &self.project)
			.field("dry_run", &self.dry_run)
			.finish_non_exhaustive()
	}
}

impl GitLabSignedCommit {
	/// Creates a new `GitLabSignedCommit` decorator.
	///
	/// `runner` is used for the post-push `git fetch` / `git reset --hard`
	/// operations that sync the local working tree after the API commit lands.
	pub fn new(
		inner: Arc<dyn Git>,
		fs: Arc<dyn Filesystem>,
		client: Arc<AsyncGitlab>,
		runner: Arc<dyn CommandRunner>,
		project: GitLabProject,
		dry_run: bool,
	) -> Self {
		Self {
			inner,
			fs,
			client,
			runner,
			project,
			dry_run,
			state: Mutex::new(State {
				staged: Vec::new(),
				pending: None,
				tags: HashMap::new(),
			}),
		}
	}

	fn project_path(&self) -> String {
		format!("{}/{}", self.project.group, self.project.project)
	}

	async fn flush(&self, pending: PendingCommit, force: bool) -> anyhow::Result<()> {
		validate_branch_name(&pending.branch)?;
		let actions =
			build_commit_actions(&*self.fs, &*self.inner, &pending.paths, self.inner.path())
				.await?;
		self.post_commit_to_api(&pending, force, actions).await?;
		self.sync_local_tree(&pending.branch).await
	}

	async fn post_commit_to_api(
		&self,
		pending: &PendingCommit,
		force: bool,
		actions: Vec<CommitAction<'static>>,
	) -> anyhow::Result<()> {
		let endpoint = CreateCommit::builder()
			.project(self.project_path())
			.branch(pending.branch.clone())
			.commit_message(pending.message.clone())
			.start_sha(pending.parent_sha.clone())
			.force(force)
			.actions(actions)
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.with_context(|| {
				format!(
					"failed to build GitLab commit request for branch '{}'",
					pending.branch
				)
			})?;

		let response: CreateCommitResponse = endpoint
			.query_async(&*self.client)
			.await
			.map_err(|e| {
				let raw = format!("{e}");
				anyhow!("{}", redact_credentials(&raw).into_owned())
			})
			.with_context(|| {
				format!(
					"failed to create signed GitLab commit on branch '{}' of project {}",
					pending.branch,
					self.project_path(),
				)
			})?;
		log::info!("Created signed GitLab commit {}", response.id);
		Ok(())
	}

	async fn sync_local_tree(&self, branch: &str) -> anyhow::Result<()> {
		let cwd = self.inner.path().as_path();
		let fetch = self
			.runner
			.run_mut("git", &["fetch", "origin", branch], cwd)
			.await
			.context("git fetch failed after GitLab API commit")?;
		if !fetch.status.success() {
			let raw = String::from_utf8_lossy(&fetch.stderr);
			let stderr = redact_credentials(&raw);
			anyhow::bail!("git fetch origin {branch} failed: {stderr}");
		}

		let reset = self
			.runner
			.run_mut("git", &["reset", "--hard", "FETCH_HEAD"], cwd)
			.await
			.context("git reset failed after GitLab API commit")?;
		if !reset.status.success() {
			let raw = String::from_utf8_lossy(&reset.stderr);
			let stderr = redact_credentials(&raw);
			anyhow::bail!("git reset --hard FETCH_HEAD failed: {stderr}");
		}

		Ok(())
	}
}

// ── Action classification ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateCommitResponse {
	id: String,
}

async fn build_commit_actions(
	fs: &dyn Filesystem,
	inner: &dyn Git,
	staged: &[PathBuf],
	repo_root: &AbsolutePath,
) -> anyhow::Result<Vec<CommitAction<'static>>> {
	let mut actions: Vec<CommitAction<'static>> = Vec::with_capacity(staged.len());
	for path in staged {
		let rel = path.strip_prefix(repo_root.as_path()).with_context(|| {
			format!(
				"staged path {} is not under repo root {}",
				path.display(),
				repo_root.as_path().display()
			)
		})?;
		let rel_str = rel.to_string_lossy().into_owned();

		let abs = AbsolutePath::new(path)
			.with_context(|| format!("staged path is not absolute: {}", path.display()))?;

		if !fs.exists(&abs).await? {
			// File was deleted (e.g. a consumed changeset).
			let action = CommitAction::builder()
				.action(CommitActionType::Delete)
				.file_path(rel_str)
				.build()
				.map_err(|e| anyhow!(e.to_string()))
				.context("failed to build GitLab delete action")?;
			actions.push(action);
			continue;
		}

		let bytes = fs.read(&abs).await?;
		// Base64 is universally safe — no UTF-8 fallback needed for binary files.
		let action_type = if inner.path_exists_at_head(rel).await? {
			CommitActionType::Update
		} else {
			CommitActionType::Create
		};
		let action = CommitAction::builder()
			.action(action_type)
			.file_path(rel_str)
			.content(bytes)
			.encoding(Encoding::Base64)
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.context("failed to build GitLab create/update action")?;
		actions.push(action);
	}
	Ok(actions)
}

// ── Git trait impl ────────────────────────────────────────────────────────────

#[async_trait]
impl Git for GitLabSignedCommit {
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

	/// Records the pending commit. The actual API call is deferred until
	/// `push()` or `force_push_branch()`, because GitLab fuses commit creation
	/// and ref update into a single call.
	///
	/// In dry-run mode, logs the intended action and returns without recording
	/// state (explicit guard required because `AsyncGitlab` is not a
	/// [`CommandRunner`] and is not intercepted by
	/// [`crate::command::DryRunCommandRunner`]).
	async fn commit(&self, message: &str) -> anyhow::Result<()> {
		if self.dry_run {
			log::info!("(dry-run) would create signed GitLab commit: {message}");
			return Ok(());
		}

		// Drain `staged` under a short-lived lock — `std::mem::take` atomically
		// moves it out and leaves an empty Vec in its place. This is TOCTOU-safe:
		// any concurrent `add()` between this lock and the final write lands in
		// the fresh empty Vec rather than being silently dropped by a later clear.
		let staged = {
			let mut state = self.state.lock().await;
			if state.staged.is_empty() {
				return Ok(());
			}
			std::mem::take(&mut state.staged)
		};

		// Dedup while preserving order: a path may appear more than once if
		// `add()` is called multiple times with overlapping file lists.
		let mut seen = std::collections::HashSet::new();
		let paths: Vec<PathBuf> = staged
			.into_iter()
			.filter(|p| seen.insert(p.clone()))
			.collect();

		let branch = self
			.inner
			.current_branch()
			.await?
			.context("cannot create signed GitLab commit with a detached HEAD")?;
		let parent_sha = self.inner.head_sha().await?;

		let mut state = self.state.lock().await;
		state.pending = Some(PendingCommit {
			branch,
			parent_sha,
			message: message.to_string(),
			paths,
		});

		Ok(())
	}

	/// Records the tag's target SHA and message for a later API-driven push.
	///
	/// Unlike the inner impl, this does **not** run `git tag -a`: the GitLab Tags
	/// API re-creates the tag from a target ref and never transfers a local tag
	/// object, so a local tag would be pure overhead — and `git tag -a` requires
	/// a configured git identity that the API path deliberately avoids. HEAD here
	/// is the release commit, already on the remote, so it is the tag's target.
	async fn tag(&self, tag_name: &str, message: &str) -> anyhow::Result<()> {
		// Validate at record time (mirrors `GitWorkdir::tag`) so a bad name fails
		// here rather than producing a confusing error later at push time.
		validate_tag_name(tag_name)?;
		// Mirror `commit()`: in dry-run, record nothing — `push_tag()` is also
		// guarded, so there is no pending entry for it to flush.
		if self.dry_run {
			log::debug!("(dry-run) skipping tag record for {tag_name}");
			return Ok(());
		}
		let sha = self.inner.head_sha().await?;
		let mut state = self.state.lock().await;
		state.tags.insert(
			tag_name.to_string(),
			PendingTag {
				sha,
				message: message.to_string(),
			},
		);
		Ok(())
	}

	/// Pushes to origin (non-forced). If a commit is pending, flushes it via
	/// the GitLab commits API and syncs the local working tree.
	async fn push(&self) -> anyhow::Result<()> {
		if self.dry_run {
			log::info!("(dry-run) would push via GitLab API");
			return Ok(());
		}

		let pending = self.state.lock().await.pending.take();
		match pending {
			Some(p) => self.flush(p, false).await,
			None => self.inner.push().await,
		}
	}

	async fn checkout(&self, branch: &str) -> anyhow::Result<()> {
		self.inner.checkout(branch).await
	}

	async fn checkout_or_reset_branch(&self, branch: &str) -> anyhow::Result<()> {
		self.inner.checkout_or_reset_branch(branch).await
	}

	/// Force-pushes a named branch. If a commit is pending, flushes it via
	/// the GitLab commits API with `force: true` and syncs the local tree.
	async fn force_push_branch(&self, branch: &str) -> anyhow::Result<()> {
		if self.dry_run {
			log::info!("(dry-run) would force-push branch {branch} via GitLab API");
			return Ok(());
		}

		let pending = self.state.lock().await.pending.take();
		match pending {
			Some(p) => {
				// `branch` arg overrides the recorded branch — force-push
				// always targets the explicitly-named branch.
				let pending = PendingCommit {
					branch: branch.to_string(),
					..p
				};
				self.flush(pending, true).await
			}
			None => self.inner.force_push_branch(branch).await,
		}
	}

	/// No-op: there is no local tag to delete in the API path.
	///
	/// `create_and_push_tags` calls this to clean up after a failed push so a
	/// retry can re-create the tag. API tag creation is idempotent on retry (see
	/// [`push_tag`](Self::push_tag)), so there is nothing to undo — and delegating
	/// to the inner `git tag -d` would emit a spurious "Failed to delete local
	/// tag" warning for a tag that was never created locally.
	async fn delete_tag(&self, tag: &str) -> anyhow::Result<()> {
		log::debug!("(api) skipping local tag deletion for {tag}; nothing to clean up");
		Ok(())
	}

	/// Creates the tag on the remote via the GitLab Tags API.
	///
	/// The tag's target SHA and annotation message come from the matching
	/// [`tag()`](Self::tag) call, which must run first (it always does in the
	/// publish flow). Pushing a tag that was never recorded is a broken
	/// invariant, not a recoverable state — synthesising one from HEAD would push
	/// a tag the caller never asked for — so an absent pending tag is an error.
	/// Tolerates the "already exists" response so a re-run after a partial
	/// failure is idempotent.
	///
	/// In dry-run mode, logs the intended action and returns without an API call
	/// (explicit guard required because `AsyncGitlab` is not a [`CommandRunner`]
	/// and is not intercepted by [`crate::command::DryRunCommandRunner`]).
	async fn push_tag(&self, tag: &str) -> anyhow::Result<()> {
		// Validate before the dry-run guard so a malformed tag name is rejected
		// even in dry-run, rather than logged unsanitised.
		validate_tag_name(tag)?;
		if self.dry_run {
			log::info!("(dry-run) would create tag {tag} via GitLab API");
			return Ok(());
		}

		let PendingTag { sha, message } =
			self.state.lock().await.tags.remove(tag).with_context(|| {
				format!("no pending tag '{tag}' to push; tag() must be called before push_tag()")
			})?;

		let mut builder = CreateTag::builder();
		builder.project(self.project_path()).tag_name(tag).ref_(sha);
		if !message.is_empty() {
			builder.message(message);
		}
		let endpoint = builder
			.build()
			.map_err(|e| anyhow!(e.to_string()))
			.with_context(|| format!("failed to build GitLab tag request for '{tag}'"))?;

		match gitlab::api::ignore(endpoint)
			.query_async(&*self.client)
			.await
		{
			Ok(()) => {
				log::info!("Created tag {tag} via GitLab API");
				Ok(())
			}
			// A fresh CI re-run after a partial failure re-attempts the push;
			// GitLab reports a duplicate tag as `400 Bad Request` with an
			// "already exists" message, which we treat as success. Gating on the
			// status code (not just the message substring) avoids masking an
			// unrelated failure whose body happens to contain that phrase. GitLab
			// may render the message as a bare string (`GitlabWithStatus`) or as a
			// structured object (`GitlabObjectWithStatus`), so both are matched.
			Err(ApiError::GitlabWithStatus { status, msg })
				if status.as_u16() == 400 && msg.contains("already exists") =>
			{
				log::info!("Tag {tag} already exists on the remote, skipping");
				Ok(())
			}
			Err(ApiError::GitlabObjectWithStatus { status, obj })
				if status.as_u16() == 400 && obj.to_string().contains("already exists") =>
			{
				log::info!("Tag {tag} already exists on the remote, skipping");
				Ok(())
			}
			Err(e) => {
				let raw = format!("{e}");
				let redacted = redact_credentials(&raw);
				Err(anyhow!("{}", redacted.into_owned())).with_context(|| {
					format!(
						"failed to create tag '{tag}' on project {}",
						self.project_path()
					)
				})
			}
		}
	}
}
