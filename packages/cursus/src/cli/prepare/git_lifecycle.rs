use std::path::PathBuf;

use anyhow::Context;
use log::info;

use crate::git::Git;
use crate::git::ref_format::validate_branch_name;
use crate::model::config::{Config, Strategy};

use super::{PrepareArgs, PrepareOutput, ReleaseInfo};

/// Git-related configuration resolved for a prepare run.
#[derive(Debug)]
pub(super) struct GitContext {
	pub(super) enabled: bool,
	pub(super) strategy: Strategy,
}

/// Branch state established during preflight checks.
#[derive(Debug)]
pub(crate) struct BranchState {
	pub(crate) original: Option<String>,
	pub(crate) release: Option<String>,
}

/// Checks that the working tree is clean before making changes.
///
/// # Errors
///
/// Returns an error if the working tree has uncommitted changes.
pub(crate) async fn check_dirty_tree(git: &dyn Git) -> anyhow::Result<()> {
	if git.is_dirty().await? {
		anyhow::bail!(
			"Working tree is dirty. Commit or stash changes before releasing.\n\
			 Run `git status` to see pending changes."
		);
	}
	Ok(())
}

/// Computes the release branch name from CLI flags, config, and current branch.
///
/// Priority order:
/// 1. `args_branch` — explicit `--branch` flag
/// 2. `{config_prefix}{current_branch}` — derived from config prefix and current branch
/// 3. `{config_prefix}detached` — fallback when HEAD is detached
///
/// # Errors
///
/// Returns an error if `args_branch` starts with `-`, which would cause git to
/// interpret the value as a flag rather than a branch name.
pub(crate) fn compute_release_branch(
	args_branch: Option<&str>,
	config_prefix: &str,
	current_branch: Option<&str>,
) -> anyhow::Result<String> {
	if let Some(branch) = args_branch {
		validate_branch_name(branch)?;
		return Ok(branch.to_string());
	}
	let base = current_branch.unwrap_or("detached");
	let composed = format!("{config_prefix}{base}");
	validate_branch_name(&composed)?;
	Ok(composed)
}

/// Stages files and creates a commit for the prepare step.
///
/// This is the core git operation for `prepare` — it only commits.
/// Pushing is handled by the strategy dispatch in `cmd_prepare`, and tagging
/// happens in `publish`.
///
/// If `dry_run` is `true`, prints what would be done without executing any git commands.
///
/// # Arguments
///
/// * `git` - Git working directory with command runner.
/// * `extra_files` - Additional files to unconditionally stage, relative to the git root.
/// * `release_infos` - The packages that were prepared for release.
/// * `modified_files` - Files to stage before committing.
/// * `commit_message` - The commit message to use, from [`GitConfig::prepare_commit_message`].
///
/// # Errors
///
/// Returns an error if any git command fails.
pub(crate) async fn stage_and_commit(
	git: &dyn Git,
	extra_files: &[String],
	release_infos: &[ReleaseInfo],
	modified_files: &[PathBuf],
	commit_message: &str,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<()> {
	if release_infos.is_empty() {
		return Ok(());
	}

	// Build the full staging list, validating that extra_files resolve inside the repo root.
	let git_workdir = git.path();
	let mut all_files = modified_files.to_vec();
	for f in extra_files {
		match git_workdir.subpath(f, fs).await {
			Ok(resolved) => all_files.push(resolved.into_path_buf()),
			Err(_) if !fs.exists(&git_workdir.child(f)).await? => {
				log::warn!("extra_files entry {:?} does not exist, skipping", f);
			}
			Err(e) => {
				return Err(e).with_context(|| {
					format!(
						"extra_files entry {:?} resolves outside the repository root",
						f
					)
				});
			}
		}
	}

	git.add(&all_files)
		.await
		.context("Failed to stage files for git commit")?;
	git.commit(commit_message)
		.await
		.context("Failed to create git commit")?;

	Ok(())
}

/// Runs pre-release checks and sets up the git branch if needed.
///
/// Validates GitHub token availability when required, checks for a dirty working
/// tree, and checks out the release branch for the branch strategy.
/// Returns a [`BranchState`] with the original and release branch names.
pub(super) async fn preflight_checks(
	git: &dyn Git,
	config: &Config,
	env: &crate::Env,
	args: &PrepareArgs,
	git_ctx: &GitContext,
	dry_run: bool,
) -> anyhow::Result<BranchState> {
	let needs_forge = git_ctx.enabled
		&& git_ctx.strategy == Strategy::Branch
		&& config.forge_enabled()
		&& !dry_run;
	if needs_forge {
		check_forge_preconditions(env)?;
	}
	if !git_ctx.enabled {
		return Ok(BranchState {
			original: None,
			release: None,
		});
	}
	if !dry_run {
		check_dirty_tree(git).await?;
	}
	if git_ctx.strategy == Strategy::Branch {
		let current = if dry_run {
			git.current_branch().await.ok().flatten()
		} else {
			git.current_branch().await?
		};
		let branch = compute_release_branch(
			args.branch.as_deref(),
			config.git.release_branch_prefix(),
			current.as_deref(),
		)?;
		git.checkout_or_reset_branch(&branch).await?;
		Ok(BranchState {
			original: current,
			release: Some(branch),
		})
	} else {
		Ok(BranchState {
			original: None,
			release: None,
		})
	}
}

/// Checks that the configured forge client is available and that its auth
/// token has the scope needed to create or update a release request.
///
/// GitLab's `CI_JOB_TOKEN` cannot create or update merge requests
/// (ADR-056); we surface that error here before the release branch is
/// pushed rather than letting the upstream 403 fire mid-flight.
pub(crate) fn check_forge_preconditions(env: &crate::Env) -> anyhow::Result<()> {
	env.code_forge_client().map_err(|reason| {
		anyhow::anyhow!(
			"Forge integration is enabled but the code forge client is unavailable: {reason}"
		)
	})?;
	if env.gitlab_uses_job_token_only() && env.code_forge_name() == "GitLab" {
		anyhow::bail!(
			"GitLab merge-request operations require GITLAB_TOKEN with `api` scope; \
			 CI_JOB_TOKEN cannot create or update merge requests. \
			 Provision a project- or group-access token and expose it as \
			 GITLAB_TOKEN in CI."
		);
	}
	Ok(())
}

/// Pushes the release branch and optionally creates/updates a pull request.
async fn push_branch_and_pr(
	git: &dyn Git,
	config: &Config,
	env: &crate::Env,
	output: &PrepareOutput,
	branches: &BranchState,
	dry_run: bool,
) -> anyhow::Result<()> {
	let Some(branch) = branches.release.as_deref() else {
		return Ok(());
	};
	info!("Pushing branch '{branch}' to origin");
	git.force_push_branch(branch).await.with_context(|| {
		format!(
			"Failed to push release branch '{branch}'. \
			 You are still on the release branch; run \
			 `git checkout <your-branch>` to return."
		)
	})?;
	let pr_result = if config.forge_enabled() {
		super::github::upsert_release_pull_request(
			config,
			env,
			&output.release_infos,
			branch,
			branches.original.as_deref(),
			dry_run,
		)
		.await
	} else {
		Ok(())
	};
	if let Some(orig) = branches.original.as_deref()
		&& let Err(checkout_err) = git.checkout(orig).await
	{
		log::error!("Failed to check out original branch after release: {checkout_err:#}");
	}
	pr_result
}

/// Stages, commits, and pushes release changes according to the configured git strategy.
pub(super) async fn finalize_git_lifecycle(
	git: &dyn Git,
	config: &Config,
	env: &crate::Env,
	output: &PrepareOutput,
	branches: &BranchState,
	git_ctx: &GitContext,
	dry_run: bool,
) -> anyhow::Result<()> {
	if !git_ctx.enabled {
		return Ok(());
	}
	stage_and_commit(
		git,
		&config.git.extra_files,
		&output.release_infos,
		&output.modified_files,
		config.git.prepare_commit_message(),
		env.fs(),
	)
	.await?;
	match git_ctx.strategy {
		Strategy::Push => git.push().await?,
		Strategy::Branch => {
			push_branch_and_pr(git, config, env, output, branches, dry_run).await?;
		}
	}
	Ok(())
}

/// Resolves git-enabled flag, strategy, and emits a warning for incompatible flags.
pub(super) fn setup_git_context(config: &Config, args: &PrepareArgs) -> GitContext {
	let enabled = config.git.enabled() && !args.no_git;
	let strategy = config.git.strategy();
	if args.branch.is_some() && strategy == Strategy::Push {
		log::warn!("--branch has no effect with the push strategy; ignoring");
	}
	GitContext { enabled, strategy }
}
