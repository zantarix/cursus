use std::path::PathBuf;

use anyhow::Context;
use log::info;

use crate::git::Git;
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
pub(super) struct BranchState {
	pub(super) original: Option<String>,
	pub(super) release: Option<String>,
}

/// Checks that the working tree is clean before making changes.
///
/// # Errors
///
/// Returns an error if the working tree has uncommitted changes.
pub(super) async fn check_dirty_tree(git: &dyn Git) -> anyhow::Result<()> {
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
pub(super) fn compute_release_branch(
	args_branch: Option<&str>,
	config_prefix: &str,
	current_branch: Option<&str>,
) -> anyhow::Result<String> {
	if let Some(branch) = args_branch {
		if branch.is_empty() {
			anyhow::bail!("Invalid branch name: branch name must not be empty");
		}
		if branch.starts_with('-') {
			anyhow::bail!("Invalid branch name '{branch}': branch names must not start with '-'");
		}
		return Ok(branch.to_string());
	}
	let base = current_branch.unwrap_or("detached");
	Ok(format!("{config_prefix}{base}"))
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
pub(super) async fn stage_and_commit(
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
	if git_ctx.enabled
		&& git_ctx.strategy == Strategy::Branch
		&& config.github.enabled
		&& !dry_run
		&& env.github_client().is_none()
	{
		anyhow::bail!(
			"GitHub integration is enabled but no GitHub token found. \
			 Set GH_TOKEN or GITHUB_TOKEN environment variable."
		);
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
	let pr_result = if config.github.enabled {
		super::github::upsert_release_pull_request(
			git,
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

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::{DispatchingCommandRunner, RecordingCommandRunner};
	use crate::filesystem::LocalFilesystem;
	use crate::model::config;

	use super::*;

	// ── stage_and_commit ──────────────────────────────────────────────────────

	#[tokio::test]
	async fn stage_and_commit_empty_releases_is_noop() {
		let dir = tempfile::tempdir().unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let result = stage_and_commit(
			&git,
			&[],
			&[],
			&[],
			"ci(release): version packages",
			&crate::filesystem::LocalFilesystem,
		)
		.await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn stage_and_commit_dry_run_suppresses_git_commands() {
		let dir = tempfile::tempdir().unwrap();
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let dry_run_runner =
			crate::command::DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::new(dry_run_runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let result = stage_and_commit(
			&git,
			&[],
			&release_infos,
			&[],
			"ci(release): version packages",
			&crate::filesystem::LocalFilesystem,
		)
		.await;
		assert!(result.is_ok());
		// DryRunCommandRunner suppresses run_mut calls — the inner recorder receives nothing.
		assert!(inner.invocations().is_empty());
	}

	#[tokio::test]
	async fn extra_files_outside_repo_is_rejected() {
		// Create an outer temp dir with a repo subdir and a secret file alongside it.
		let outer = tempfile::tempdir().unwrap();
		let repo_dir = outer.path().join("repo");
		std::fs::create_dir(&repo_dir).unwrap();
		let secret = outer.path().join("secret.txt");
		std::fs::write(&secret, "secret").unwrap();
		// "../secret.txt" from repo_dir resolves to outer/secret.txt (outside the repo).
		let extra_files = vec!["../secret.txt".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(&repo_dir).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let result = stage_and_commit(
			&git,
			&extra_files,
			&release_infos,
			&[],
			"ci(release): version packages",
			&crate::filesystem::LocalFilesystem,
		)
		.await;
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[cfg(unix)]
	#[tokio::test]
	async fn extra_files_symlink_outside_repo_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		// Create a symlink inside the tempdir pointing to /tmp (outside the repo).
		let symlink_path = dir.path().join("escape");
		std::os::unix::fs::symlink("/tmp", &symlink_path).unwrap();
		let extra_files = vec!["escape".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let result = stage_and_commit(
			&git,
			&extra_files,
			&release_infos,
			&[],
			"ci(release): version packages",
			&crate::filesystem::LocalFilesystem,
		)
		.await;
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[tokio::test]
	async fn extra_files_nonexistent_is_skipped() {
		let dir = tempfile::tempdir().unwrap();
		let extra_files = vec!["does-not-exist.txt".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		// Should succeed (non-existent file is a warning, not an error).
		let result = stage_and_commit(
			&git,
			&extra_files,
			&release_infos,
			&[],
			"ci(release): version packages",
			&crate::filesystem::LocalFilesystem,
		)
		.await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn extra_files_absolute_path_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		let extra_files = vec!["/etc/passwd".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let result = stage_and_commit(
			&git,
			&extra_files,
			&release_infos,
			&[],
			"ci(release): version packages",
			&crate::filesystem::LocalFilesystem,
		)
		.await;
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[tokio::test]
	async fn compute_release_branch_uses_flag_over_all() {
		assert_eq!(
			compute_release_branch(Some("my-branch"), "release/", Some("main")).unwrap(),
			"my-branch"
		);
	}

	#[tokio::test]
	async fn compute_release_branch_uses_config_prefix() {
		assert_eq!(
			compute_release_branch(None, "release/", Some("main")).unwrap(),
			"release/main"
		);
	}

	#[tokio::test]
	async fn compute_release_branch_uses_default_prefix() {
		assert_eq!(
			compute_release_branch(None, "cursus-release/", Some("main")).unwrap(),
			"cursus-release/main"
		);
	}

	#[tokio::test]
	async fn compute_release_branch_detached_fallback() {
		assert_eq!(
			compute_release_branch(None, "cursus-release/", None).unwrap(),
			"cursus-release/detached"
		);
	}

	#[tokio::test]
	async fn compute_release_branch_rejects_dash_prefix() {
		let result = compute_release_branch(Some("--detach"), "release/", Some("main"));
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("must not start with '-'")
		);
	}

	#[tokio::test]
	async fn compute_release_branch_rejects_single_dash() {
		let result = compute_release_branch(Some("-"), "release/", None);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("must not start with '-'")
		);
	}

	#[tokio::test]
	async fn compute_release_branch_rejects_empty() {
		let result = compute_release_branch(Some(""), "release/", Some("main"));
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("must not be empty")
		);
	}

	#[tokio::test]
	async fn check_dirty_tree_succeeds_when_clean() {
		let dir = tempfile::tempdir().unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0)); // empty stdout → clean
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let result = check_dirty_tree(&git).await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn check_dirty_tree_fails_when_dirty() {
		let dir = tempfile::tempdir().unwrap();
		let runner =
			Arc::new(RecordingCommandRunner::new(0).with_stdout(b" M src/main.rs\n".to_vec()));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let result = check_dirty_tree(&git).await;
		assert!(result.is_err());
		assert!(
			result.unwrap_err().to_string().contains("dirty"),
			"Expected 'dirty' in error message"
		);
	}

	fn make_test_env(dir: &std::path::Path) -> crate::Env {
		let r = Arc::new(crate::command::test_support::RecordingCommandRunner::new(0))
			as Arc<dyn CommandRunner>;
		crate::Env::new(
			Arc::clone(&r),
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(
				r,
				crate::path::AbsolutePath::new(dir).unwrap(),
			)),
		)
	}

	/// Sets up a temp dir with a Cargo project, branch strategy git config, and GitHub config.
	async fn setup_branch_strategy_with_github() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		crate::model::config::Config::new(&make_test_env(dir.path()))
			.with_cargo(crate::model::config::CargoConfig::enabled())
			.with_git(
				crate::model::config::GitConfig::enabled_config()
					.with_strategy(crate::model::config::Strategy::Branch),
			)
			.with_github(
				crate::model::config::GitHubConfig::enabled_config()
					.with_owner("acme".to_string())
					.with_repo("app".to_string())
					.with_pull_request_title("My Release PR".to_string()),
			)
			.save()
			.await
			.unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"test-pkg\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
		)
		.unwrap();
		std::fs::create_dir_all(dir.path().join("src")).unwrap();
		std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
		let cursus_dir = dir.path().join(".cursus");
		std::fs::write(
			cursus_dir.join("change.md"),
			"+++\ntest-pkg = \"patch\"\n+++\n\nFix\n",
		)
		.unwrap();
		dir
	}

	#[tokio::test]
	async fn cmd_prepare_branch_strategy_with_github_creates_pr() {
		use crate::github::client::GitHubClient;
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let dir = setup_branch_strategy_with_github().await;
		let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
			"git",
			vec![
				"rev-parse".to_string(),
				"--abbrev-ref".to_string(),
				"HEAD".to_string(),
			],
			0,
			b"main\n".to_vec(),
		));
		let client = Arc::new(RecordingGitHubClient::new());
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				dir_abs.clone(),
			)),
		)
		.with_github_client(Arc::clone(&client) as Arc<dyn GitHubClient>);
		let config = config::load(&env).await.unwrap();
		let args = PrepareArgs::default();

		let result = super::super::cmd_prepare(&args, false, config).await;
		assert!(result.is_ok(), "Expected Ok, got: {result:?}");

		let invocations = client.invocations();
		let pr = invocations
			.iter()
			.find(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. }));
		assert!(pr.is_some(), "Expected PR creation, got: {invocations:?}");
		if let Some(GitHubInvocation::CreatePullRequest { title, gh_repo, .. }) = pr {
			assert_eq!(title, "My Release PR");
			assert_eq!(gh_repo.owner, "acme");
			assert_eq!(gh_repo.repo, "app");
		}
	}

	#[tokio::test]
	async fn cmd_prepare_branch_strategy_pr_failure_is_fatal() {
		use crate::github::client::GitHubClient;
		use crate::github::client::test_support::RecordingGitHubClient;
		let dir = setup_branch_strategy_with_github().await;
		let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args_stdout(
			"git",
			vec![
				"rev-parse".to_string(),
				"--abbrev-ref".to_string(),
				"HEAD".to_string(),
			],
			0,
			b"main\n".to_vec(),
		));
		let client = Arc::new(RecordingGitHubClient::new().with_create_pr_failure());
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				dir_abs.clone(),
			)),
		)
		.with_github_client(Arc::clone(&client) as Arc<dyn GitHubClient>);
		let config = config::load(&env).await.unwrap();
		let args = PrepareArgs::default();

		let result = super::super::cmd_prepare(&args, false, config).await;
		assert!(
			result.is_err(),
			"PR failure should be fatal, got: {result:?}"
		);
	}

	#[tokio::test]
	async fn cmd_prepare_no_github_client_errors() {
		let dir = setup_branch_strategy_with_github().await;
		let runner = Arc::new(RecordingCommandRunner::new(0));
		// No github client — pre-flight check should error
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				dir_abs.clone(),
			)),
		);
		let config = config::load(&env).await.unwrap();
		let args = PrepareArgs::default();

		let result = super::super::cmd_prepare(&args, false, config).await;
		assert!(result.is_err(), "Expected Err without github client");
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("no GitHub token"),
			"Expected 'no GitHub token' error, got: {msg}"
		);
	}
}
