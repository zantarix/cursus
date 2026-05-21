//! The `change` subcommand.

use std::collections::{BTreeMap, HashSet};
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::Args;
use log::info;

use crate::git::Git;
use crate::model::changeset::{ChangeType, Changeset, derive_changeset};
use crate::model::config::Config;
use crate::package_manager::matching::match_files_to_projects_in_scope;
use crate::tui::change;

use super::GlobalArgs;

/// Arguments for the `change` subcommand.
#[derive(Args, Default)]
pub struct ChangeArgs {
	/// Type of change: major, minor, or patch (required in non-interactive mode)
	#[arg(short = 't', long, conflicts_with = "auto")]
	pub change_type: Option<ChangeType>,

	/// Project name(s) to include (repeatable; defaults to git-changed projects, or all if none detected)
	#[arg(short = 'p', long = "project")]
	pub projects: Vec<String>,

	/// Description message for the changeset (required in non-interactive mode)
	#[arg(short = 'm', long, conflicts_with = "auto")]
	pub message: Option<String>,

	/// Derive changeset from the single Conventional Commit on this branch
	#[arg(long, conflicts_with_all = ["change_type", "message"])]
	pub auto: bool,

	/// Skip committing and pushing the changeset to git (only with --auto)
	#[arg(long, requires = "auto")]
	pub no_git: bool,
}

/// Classifies each project as changed (`true`) or unchanged (`false`).
///
/// Considers three sources of changes:
/// - Files committed since `origin/HEAD` (`git diff --name-only origin/HEAD..HEAD`)
/// - Staged files (`git diff --name-only --cached`)
/// - Unstaged working-tree files (`git diff --name-only`)
///
/// `all_projects` is used for longest-prefix attribution so that files inside
/// ignored sub-projects (present in `all_projects` but absent from `projects`)
/// are not mis-attributed to their releasable parents.
///
/// Falls back to `vec![true; projects.len()]` if all three diff sources fail
/// (e.g. no git repo or a completely uninitialised environment).
pub(crate) async fn classify_changed_projects(
	git: &dyn Git,
	projects: &[crate::package_manager::Project],
	all_projects: &[crate::package_manager::Project],
) -> Vec<bool> {
	// Collect changed file paths from committed, staged, and unstaged sources.
	// Each call is independent; failures are treated as empty (no files from that source).
	let sources = [
		git.diff_names(&["origin/HEAD..HEAD"]).await,
		git.diff_names(&["--cached"]).await,
		git.diff_names(&[]).await,
	];
	let any_succeeded = sources.iter().any(|r| r.is_ok());
	if !any_succeeded {
		// Cannot determine changes at all — conservatively treat all as changed.
		return vec![true; projects.len()];
	}
	let changed_files: HashSet<String> = sources
		.into_iter()
		.filter_map(|r| r.ok())
		.flatten()
		.collect();

	match_files_to_projects_in_scope(projects, all_projects, git.path(), &changed_files)
}

/// Maps `--project` names to indices into the project list.
///
/// Returns `Ok(None)` when `names` is empty (meaning all projects).
/// Returns an error if any name is not found in `projects`.
fn resolve_project_indices(
	projects: &[crate::package_manager::Project],
	names: &[String],
) -> anyhow::Result<Option<Vec<usize>>> {
	if names.is_empty() {
		return Ok(None);
	}
	let indices = names
		.iter()
		.map(|name| {
			projects
				.iter()
				.position(|p| p.name() == name)
				.ok_or_else(|| anyhow::anyhow!("Unknown project: {name}"))
		})
		.collect::<anyhow::Result<Vec<_>>>()?;
	Ok(Some(indices))
}

/// Validates that there is exactly one commit ahead of `origin/HEAD`.
///
/// Returns `Ok(Some(message))` when exactly one commit is ahead.
/// Returns `Ok(None)` when more than one commit is ahead (caller should skip).
/// Returns an error when zero commits are ahead.
async fn validate_single_commit(git: &dyn Git) -> anyhow::Result<Option<String>> {
	let count = git.rev_list_count("origin/HEAD..HEAD").await?;
	if count == 0 {
		bail!("No commits ahead of origin/HEAD — nothing to derive a changeset from");
	}
	if count > 1 {
		info!(
			"Branch has {count} commits ahead of origin/HEAD; \
			 skipping --auto (expected exactly 1)"
		);
		return Ok(None);
	}
	Ok(Some(git.log_message("HEAD").await?))
}

/// Runs `cursus change --auto`: derives a changeset from the single
/// Conventional Commit on the current branch.
///
/// Returns `ExitCode::SUCCESS` without creating a changeset when:
/// - There is more than one commit ahead of `origin/HEAD` (recursion guard).
/// - The commit type has no semver significance (e.g., `chore:`, `docs:`).
/// - No project paths overlap with the files changed by the commit.
///
/// # Errors
///
/// Returns an error when zero commits are ahead or the message is invalid.
async fn cmd_change_auto(
	args: &ChangeArgs,
	global: &GlobalArgs,
	env: &crate::Env,
	config: Config,
) -> anyhow::Result<ExitCode> {
	let git = env.git();
	let Some(message) = validate_single_commit(git).await? else {
		return Ok(ExitCode::SUCCESS);
	};

	let (all_projects, projects) = config.load_projects_partitioned(env).await?;
	let changed_files: HashSet<String> = git.diff_tree_names("HEAD").await?.into_iter().collect();
	let matched_flags =
		match_files_to_projects_in_scope(&projects, &all_projects, git.path(), &changed_files);
	let matched: Vec<_> = projects
		.iter()
		.zip(matched_flags.iter())
		.filter_map(|(p, &m)| m.then_some(p))
		.collect();

	if matched.is_empty() {
		info!("No projects matched the changed files — skipping changeset");
		return Ok(ExitCode::SUCCESS);
	}

	let project_names: Vec<&str> = matched.iter().map(|p| p.name()).collect();
	let Some(changeset) = derive_changeset(&message, &project_names)? else {
		info!("Commit has no semver significance — skipping changeset");
		return Ok(ExitCode::SUCCESS);
	};

	let description = changeset
		.message
		.as_deref()
		.and_then(|m| m.lines().next())
		.unwrap_or("auto-derived changeset");

	if global.dry_run {
		// Intentionally use println! instead of log::info! so that --silent
		// suppresses all other output and stdout contains only the changeset
		// content, making it safe to redirect (e.g. `cursus change --dry-run > file`).
		println!("{}", changeset.format()?);
		if config.git.enabled() && !args.no_git {
			git.add(&[git.path().join(".cursus/changeset-dry-run.md")])
				.await?;
		}
	} else {
		let path = changeset.write(git, env.fs()).await?;
		info!("Created changeset: {}", path.display());
		if config.git.enabled() && !args.no_git {
			git.add(&[path]).await?;
		}
	}
	if config.git.enabled() && !args.no_git {
		git.commit(&format!("chore: add changeset for {description}"))
			.await?;
		git.push().await?;
	}
	Ok(ExitCode::SUCCESS)
}

/// Builds the non-interactive [`change::ChangeResult`].
///
/// Selection rules (when `project_indices` is `None`, i.e. no `--project` flags):
/// - Projects flagged as changed in `changed` are selected.
/// - Falls back to all projects when no project is flagged as changed (e.g. clean
///   working tree, or when all git diff sources failed and the fallback all-true
///   vector was replaced by an all-false one).
///
/// When `project_indices` is `Some`, those explicit indices are used verbatim and
/// `changed` is ignored.
///
/// # Invariant
/// `changed` must be parallel to `projects` (same length).
pub(crate) fn resolve_non_interactive(
	args: &ChangeArgs,
	projects: &[crate::package_manager::Project],
	project_indices: &Option<Vec<usize>>,
	changed: &[bool],
) -> anyhow::Result<change::ChangeResult> {
	debug_assert_eq!(
		projects.len(),
		changed.len(),
		"changed must be parallel to projects"
	);
	let Some(ct) = args.change_type else {
		bail!("--change-type is required in non-interactive mode");
	};
	if args.message.is_none() {
		bail!("--message is required in non-interactive mode");
	}
	let selected: Vec<crate::package_manager::Project> = match project_indices {
		Some(indices) => indices.iter().map(|&i| projects[i].clone()).collect(),
		None => {
			let changed_projects: Vec<_> = projects
				.iter()
				.zip(changed.iter())
				.filter_map(|(p, &c)| c.then_some(p.clone()))
				.collect();
			if changed_projects.is_empty() {
				projects.to_vec()
			} else {
				changed_projects
			}
		}
	};
	Ok(change::ChangeResult {
		projects: selected.into_iter().map(|p| (p, ct)).collect(),
		message: args.message.clone(),
	})
}

/// Runs the `change` subcommand.
pub(crate) async fn cmd_change(
	args: &ChangeArgs,
	global: &GlobalArgs,
	env: &crate::Env,
	config: Config,
) -> anyhow::Result<ExitCode> {
	if args.auto {
		return cmd_change_auto(args, global, env, config).await;
	}

	let git = env.git();
	let (all_projects, projects) = config.load_projects_partitioned(env).await?;

	let project_indices = resolve_project_indices(&projects, &args.projects)?;
	let changed = classify_changed_projects(git, &projects, &all_projects).await;

	let result = if global.no_interactive {
		resolve_non_interactive(args, &projects, &project_indices, &changed)?
	} else {
		let options = change::ChangeOptions {
			change_type: args.change_type,
			projects: project_indices.clone(),
		};
		let projects_clone = projects.clone();
		let mut r = match tokio::task::spawn_blocking(move || {
			change::run(&projects_clone, &options, &changed)
		})
		.await
		.context("TUI task panicked")??
		{
			Some(r) => r,
			None => return Ok(ExitCode::from(2)),
		};
		// --message always takes precedence over any TUI-entered message.
		if let Some(msg) = &args.message {
			r.message = Some(msg.clone());
		}
		r
	};

	let packages: BTreeMap<String, ChangeType> = result
		.projects
		.iter()
		.map(|(p, ct)| (p.name().to_string(), *ct))
		.collect();

	let changeset = Changeset::new(packages, result.message.clone());

	if global.dry_run {
		// Intentionally use println! instead of log::info! so that --silent
		// suppresses all other output and stdout contains only the changeset
		// content, making it safe to redirect (e.g. `cursus change --dry-run > file`).
		println!("{}", changeset.format()?);
	} else {
		let path = changeset.write(git, env.fs()).await?;
		info!("Created changeset: {}", path.display());
		if result.message.is_none() {
			env.run_editor_on(&path, git.path()).await?;
		}
	}

	Ok(ExitCode::SUCCESS)
}
