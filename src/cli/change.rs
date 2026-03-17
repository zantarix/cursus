//! The `change` subcommand.

use std::collections::{BTreeMap, HashSet};
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::Args;
use log::info;

use crate::conventional_commit;
use crate::git;
use crate::model::changeset::{ChangeType, Changeset};
use crate::model::config::Config;
use crate::package_manager::Project;
use crate::path::AbsolutePath;
use crate::tui::change;

use super::GlobalArgs;

/// Arguments for the `change` subcommand.
#[derive(Args, Default)]
pub struct ChangeArgs {
	/// Type of change: major, minor, or patch (required in non-interactive mode)
	#[arg(short = 't', long, conflicts_with = "auto")]
	pub change_type: Option<ChangeType>,

	/// Project name(s) to include (repeatable; defaults to all in non-interactive mode)
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

/// Assigns each changed file to the project with the longest matching path prefix,
/// returning a boolean mask parallel to `projects`.
///
/// When multiple projects share a prefix (e.g. nested projects), a file is attributed
/// only to the most specific (deepest) project(s) whose path contains it. If several
/// projects share the same deepest path (e.g. Cargo and npm both at the repo root),
/// all of them are marked. The root project (at the git root) only receives files
/// not claimed by any sub-project.
///
/// Projects whose path is outside the git root are always treated as unchanged — git
/// cannot track files outside the repository, so there are no changed files to attribute.
fn match_files_to_projects(
	projects: &[Project],
	git_path: &AbsolutePath,
	changed_files: &HashSet<String>,
) -> Vec<bool> {
	// Pre-compute relative path strings for each project.
	// `None` means the project path is outside the git root; such projects are left as false.
	let rel_paths: Vec<Option<String>> = projects
		.iter()
		.map(|p| {
			p.path()
				.strip_prefix(git_path.as_path())
				.ok()
				.map(|r| r.to_string_lossy().into_owned())
		})
		.collect();

	let mut matched = vec![false; projects.len()];

	// For each changed file, find all projects that match with the longest prefix.
	// When multiple projects share the same path (e.g. Cargo and npm both at the repo root),
	// all of them are marked changed — not just the last one in the list.
	for file in changed_files {
		let candidates: Vec<(usize, usize)> = rel_paths
			.iter()
			.enumerate()
			.filter_map(|(i, rel_opt)| {
				let rel = rel_opt.as_deref()?;
				if rel.is_empty() {
					// Root project: matches any file, but with the lowest priority (0).
					Some((i, 0usize))
				} else if file.starts_with(rel)
					&& (file.len() == rel.len() || file.as_bytes().get(rel.len()) == Some(&b'/'))
				{
					Some((i, rel.len()))
				} else {
					None
				}
			})
			.collect();

		if let Some(&(_, best_len)) = candidates.iter().max_by_key(|(_, len)| *len) {
			candidates
				.iter()
				.filter(|(_, len)| *len == best_len)
				.for_each(|(i, _)| matched[*i] = true);
		}
	}

	matched
}

/// Classifies each project as changed (`true`) or unchanged (`false`).
///
/// Considers three sources of changes:
/// - Files committed since `origin/HEAD` (`git diff --name-only origin/HEAD..HEAD`)
/// - Staged files (`git diff --name-only --cached`)
/// - Unstaged working-tree files (`git diff --name-only`)
///
/// Falls back to `vec![true; projects.len()]` if all three diff sources fail
/// (e.g. no git repo or a completely uninitialised environment).
fn classify_changed_projects(
	git: &git::GitWorkdir,
	projects: &[crate::package_manager::Project],
) -> Vec<bool> {
	// Collect changed file paths from committed, staged, and unstaged sources.
	// Each call is independent; failures are treated as empty (no files from that source).
	let sources = [
		git.diff_names(&["origin/HEAD..HEAD"]),
		git.diff_names(&["--cached"]),
		git.diff_names(&[]),
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

	match_files_to_projects(projects, git.path(), &changed_files)
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
fn validate_single_commit(git: &git::GitWorkdir) -> anyhow::Result<Option<String>> {
	let count = git.rev_list_count("origin/HEAD..HEAD")?;
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
	Ok(Some(git.log_message("HEAD")?))
}

/// Writes the auto-derived changeset and optionally commits and pushes it.
///
/// When `dry_run` is true the filesystem write is skipped and an info message
/// is logged instead. Git operations (`commit`, `push`) still execute but are
/// suppressed by the [`DryRunCommandRunner`](crate::command::DryRunCommandRunner).
fn write_auto_changeset(
	git: &git::GitWorkdir,
	dry_run: bool,
	commit_to_git: bool,
	matched: &[&Project],
	change_type: ChangeType,
	changeset_message: &str,
	description: &str,
) -> anyhow::Result<()> {
	let packages: BTreeMap<String, ChangeType> = matched
		.iter()
		.map(|p| (p.name().to_string(), change_type))
		.collect();
	let changeset = Changeset::new(packages, Some(changeset_message.to_string()));
	if dry_run {
		println!("{}", changeset.format()?);
		if commit_to_git {
			git.add(&[git.path().join(".cursus/changeset-dry-run.md")])?;
		}
	} else {
		let path = changeset.write(git)?;
		if commit_to_git {
			git.add(&[path])?;
		}
	}
	if commit_to_git {
		git.commit(&format!("chore: add changeset for {description}"))?;
		git.push()?;
	}
	Ok(())
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
fn cmd_change_auto(
	git: &git::GitWorkdir,
	args: &ChangeArgs,
	global: &GlobalArgs,
	config: Config,
) -> anyhow::Result<ExitCode> {
	let Some(message) = validate_single_commit(git)? else {
		return Ok(ExitCode::SUCCESS);
	};

	let commit = conventional_commit::parse(&message)?;
	let Some(change_type) = commit.change_type() else {
		info!(
			"Commit '{}' has no semver significance — skipping changeset",
			commit.commit_type
		);
		return Ok(ExitCode::SUCCESS);
	};

	let projects = config.load_projects()?;
	let changed_files: HashSet<String> = git.diff_tree_names("HEAD")?.into_iter().collect();
	let matched_flags = match_files_to_projects(&projects, git.path(), &changed_files);
	let matched: Vec<_> = projects
		.iter()
		.zip(matched_flags.iter())
		.filter_map(|(p, &m)| m.then_some(p))
		.collect();

	if matched.is_empty() {
		info!("No projects matched the changed files — skipping changeset");
		return Ok(ExitCode::SUCCESS);
	}

	let changeset_message = match &commit.body {
		Some(body) => format!("{}\n\n{body}", commit.description),
		None => commit.description.clone(),
	};

	write_auto_changeset(
		git,
		global.dry_run,
		config.git.enabled() && !args.no_git,
		&matched,
		change_type,
		&changeset_message,
		&commit.description,
	)?;
	Ok(ExitCode::SUCCESS)
}

/// Runs the `change` subcommand.
fn resolve_non_interactive(
	args: &ChangeArgs,
	projects: &[crate::package_manager::Project],
	project_indices: &Option<Vec<usize>>,
) -> anyhow::Result<change::ChangeResult> {
	let Some(ct) = args.change_type else {
		bail!("--change-type is required in non-interactive mode");
	};
	if args.message.is_none() {
		bail!("--message is required in non-interactive mode");
	}
	let selected: Vec<crate::package_manager::Project> = match project_indices {
		Some(indices) => indices.iter().map(|&i| projects[i].clone()).collect(),
		None => projects.to_vec(),
	};
	Ok(change::ChangeResult {
		projects: selected.into_iter().map(|p| (p, ct)).collect(),
		message: args.message.clone(),
	})
}

pub(crate) fn cmd_change(
	git: &git::GitWorkdir,
	args: &ChangeArgs,
	global: &GlobalArgs,
	config: Config,
) -> anyhow::Result<ExitCode> {
	if args.auto {
		return cmd_change_auto(git, args, global, config);
	}

	let env = config.env().context("env not set")?;
	let projects = config.load_projects()?;

	let project_indices = resolve_project_indices(&projects, &args.projects)?;

	let result = if global.no_interactive {
		resolve_non_interactive(args, &projects, &project_indices)?
	} else {
		let options = change::ChangeOptions {
			change_type: args.change_type,
			projects: project_indices,
		};
		let changed = classify_changed_projects(git, &projects);
		let mut r = match change::run(&projects, &options, &changed)? {
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
		println!("{}", changeset.format()?);
	} else {
		let path = changeset.write(git)?;
		if result.message.is_none() {
			env.run_editor_on(&path, git.path())?;
		}
	}

	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use std::path::Path;
	use std::process::Output;
	use std::sync::{Arc, Mutex};

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::package_manager::Project;
	use crate::path::AbsolutePath;

	use super::*;

	fn make_git_with_diff_output(stdout: &[u8]) -> git::GitWorkdir {
		let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(stdout.to_vec()));
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		git::GitWorkdir::new(&env, AbsolutePath::new("/nonexistent").unwrap())
	}

	fn make_git_failing() -> git::GitWorkdir {
		let runner = Arc::new(RecordingCommandRunner::new(1));
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		git::GitWorkdir::new(&env, AbsolutePath::new("/nonexistent").unwrap())
	}

	/// A command runner that returns a sequence of responses, one per call.
	///
	/// Each entry is `(exit_code, stdout)`. Once the sequence is exhausted,
	/// subsequent calls succeed with empty stdout.
	#[derive(Debug)]
	struct SequencedRunner {
		responses: Mutex<Vec<(i32, Vec<u8>)>>,
	}

	impl SequencedRunner {
		fn new(responses: Vec<(i32, Vec<u8>)>) -> Self {
			Self {
				responses: Mutex::new(responses),
			}
		}
	}

	impl CommandRunner for SequencedRunner {
		fn run(&self, _program: &str, _args: &[&str], _cwd: &Path) -> anyhow::Result<Output> {
			#[cfg(unix)]
			fn make_status(code: i32) -> std::process::ExitStatus {
				use std::os::unix::process::ExitStatusExt;
				std::process::ExitStatus::from_raw(code << 8)
			}
			#[cfg(windows)]
			fn make_status(code: i32) -> std::process::ExitStatus {
				use std::os::windows::process::ExitStatusExt;
				std::process::ExitStatus::from_raw(code as u32)
			}
			let (code, stdout) = self
				.responses
				.lock()
				.expect("mutex poisoned")
				.drain(..1)
				.next()
				.unwrap_or((0, vec![]));
			Ok(Output {
				status: make_status(code),
				stdout,
				stderr: vec![],
			})
		}

		fn run_shell(&self, _command: &str, cwd: &Path) -> anyhow::Result<Output> {
			self.run("sh", &["-c", ""], cwd)
		}

		fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
			self.run(program, args, cwd)
		}

		fn run_shell_mut(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
			self.run_shell(command, cwd)
		}

		fn run_interactive(
			&self,
			_program: &str,
			_args: &[&str],
			_cwd: &Path,
		) -> anyhow::Result<std::process::ExitStatus> {
			#[cfg(unix)]
			{
				use std::os::unix::process::ExitStatusExt;
				return Ok(std::process::ExitStatus::from_raw(0));
			}
			#[cfg(windows)]
			{
				use std::os::windows::process::ExitStatusExt;
				return Ok(std::process::ExitStatus::from_raw(0));
			}
		}
	}

	fn make_git_sequenced(responses: Vec<(i32, Vec<u8>)>) -> git::GitWorkdir {
		let runner = Arc::new(SequencedRunner::new(responses));
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		git::GitWorkdir::new(&env, AbsolutePath::new("/nonexistent").unwrap())
	}

	#[test]
	fn default_change_args() {
		let args = ChangeArgs::default();
		assert!(args.change_type.is_none());
		assert!(args.projects.is_empty());
		assert!(args.message.is_none());
		assert!(!args.auto);
		assert!(!args.no_git);
	}

	#[test]
	fn classify_changed_projects_matches_by_prefix() {
		let git = make_git_with_diff_output(b"packages/a/src/lib.rs\n");
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("b", "/nonexistent/packages/b"),
		];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![true, false]);
	}

	#[test]
	fn classify_changed_projects_does_not_match_prefix_without_separator() {
		// "packages/a-extra/foo.rs" must not match project "packages/a"
		let git = make_git_with_diff_output(b"packages/a-extra/foo.rs\n");
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("a-extra", "/nonexistent/packages/a-extra"),
		];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![false, true]);
	}

	#[test]
	fn classify_changed_projects_fallback_on_failure() {
		let git = make_git_failing();
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("b", "/nonexistent/packages/b"),
		];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![true, true]);
	}

	#[test]
	fn classify_changed_projects_empty_diff_returns_unchanged() {
		let git = make_git_with_diff_output(b"");
		let projects = vec![Project::new_test("a", "/nonexistent/packages/a")];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![false]);
	}

	#[test]
	fn classify_changed_projects_root_project_changed_when_any_file_changed() {
		let git = make_git_with_diff_output(b"src/main.rs\n");
		let projects = vec![Project::new_test("root", "/nonexistent")];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![true]);
	}

	#[test]
	fn classify_changed_projects_root_project_unchanged_when_empty_diff() {
		let git = make_git_with_diff_output(b"");
		let projects = vec![Project::new_test("root", "/nonexistent")];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![false]);
	}

	#[test]
	fn classify_changed_projects_detects_staged_only_changes() {
		// committed diff fails (no remote), staged diff has a file, unstaged is empty
		let git = make_git_sequenced(vec![
			(1, vec![]),                          // committed: fails
			(0, b"packages/a/lib.rs\n".to_vec()), // staged: has a file
			(0, vec![]),                          // unstaged: empty
		]);
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("b", "/nonexistent/packages/b"),
		];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![true, false]);
	}

	#[test]
	fn classify_changed_projects_detects_unstaged_only_changes() {
		// committed diff fails, staged is empty, unstaged has a file
		let git = make_git_sequenced(vec![
			(1, vec![]),                            // committed: fails
			(0, vec![]),                            // staged: empty
			(0, b"packages/b/index.js\n".to_vec()), // unstaged: has a file
		]);
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("b", "/nonexistent/packages/b"),
		];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![false, true]);
	}

	#[test]
	fn classify_changed_projects_unions_all_sources() {
		// Each source covers a different project
		let git = make_git_sequenced(vec![
			(0, b"packages/a/lib.rs\n".to_vec()),   // committed: project a
			(0, b"packages/b/index.js\n".to_vec()), // staged: project b
			(0, b"packages/c/main.go\n".to_vec()),  // unstaged: project c
		]);
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("b", "/nonexistent/packages/b"),
			Project::new_test("c", "/nonexistent/packages/c"),
		];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![true, true, true]);
	}

	#[test]
	fn classify_changed_projects_fallback_only_when_all_fail() {
		// All three diffs fail → all-changed fallback
		let git = make_git_sequenced(vec![
			(1, vec![]), // committed: fails
			(1, vec![]), // staged: fails
			(1, vec![]), // unstaged: fails
		]);
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("b", "/nonexistent/packages/b"),
		];
		let result = classify_changed_projects(&git, &projects);
		assert_eq!(result, vec![true, true]);
	}

	// --- match_files_to_projects ---

	#[test]
	fn match_files_to_projects_basic_prefix_match() {
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("a", "/repo/packages/a"),
			Project::new_test("b", "/repo/packages/b"),
		];
		let mut files = HashSet::new();
		files.insert("packages/a/src/lib.rs".to_string());
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![true, false]
		);
	}

	#[test]
	fn match_files_to_projects_no_match_for_different_project() {
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("a", "/repo/packages/a"),
			Project::new_test("b", "/repo/packages/b"),
		];
		let mut files = HashSet::new();
		files.insert("packages/b/src/lib.rs".to_string());
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![false, true]
		);
	}

	#[test]
	fn match_files_to_projects_no_prefix_match_without_separator() {
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("a", "/repo/packages/a"),
			Project::new_test("a-extra", "/repo/packages/a-extra"),
		];
		let mut files = HashSet::new();
		files.insert("packages/a-extra/lib.rs".to_string());
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![false, true]
		);
	}

	#[test]
	fn match_files_to_projects_nested_file_goes_to_child() {
		// A file inside the child project must only match the child, not the parent.
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("parent", "/repo/packages/a"),
			Project::new_test("child", "/repo/packages/a/sub"),
		];
		let mut files = HashSet::new();
		files.insert("packages/a/sub/src/lib.rs".to_string());
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![false, true]
		);
	}

	#[test]
	fn match_files_to_projects_nested_parent_file_goes_to_parent() {
		// A file inside the parent but outside the child must go to the parent.
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("parent", "/repo/packages/a"),
			Project::new_test("child", "/repo/packages/a/sub"),
		];
		let mut files = HashSet::new();
		files.insert("packages/a/README.md".to_string());
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![true, false]
		);
	}

	#[test]
	fn match_files_to_projects_root_project_matches_unowned_file() {
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("root", "/repo"),
			Project::new_test("a", "/repo/packages/a"),
		];
		let mut files = HashSet::new();
		files.insert("src/main.rs".to_string());
		// src/main.rs is not under packages/a, so root gets it.
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![true, false]
		);
	}

	#[test]
	fn match_files_to_projects_root_does_not_steal_from_subproject() {
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("root", "/repo"),
			Project::new_test("a", "/repo/packages/a"),
		];
		let mut files = HashSet::new();
		files.insert("packages/a/src/lib.rs".to_string());
		// packages/a/src/lib.rs belongs to "a", not root.
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![false, true]
		);
	}

	#[test]
	fn match_files_to_projects_empty_files() {
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![Project::new_test("root", "/repo")];
		let files = HashSet::new();
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![false]
		);
	}

	#[test]
	fn match_files_to_projects_outside_git_root_always_unchanged() {
		// Git cannot track files outside the repo, so out-of-root projects are always unchanged.
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![Project::new_test("outside", "/other/path")];
		let files = HashSet::new();
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![false]
		);
	}

	#[test]
	fn match_files_to_projects_outside_git_root_unchanged_even_with_files() {
		// Out-of-root project is not attributed any files; in-repo project is still matched.
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("outside", "/other/path"),
			Project::new_test("a", "/repo/packages/a"),
		];
		let mut files = HashSet::new();
		files.insert("packages/a/src/lib.rs".to_string());
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![false, true]
		);
	}

	#[test]
	fn match_files_to_projects_unowned_file_with_no_root() {
		// A file that doesn't fall under any project's path should not mark any project.
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("a", "/repo/packages/a"),
			Project::new_test("b", "/repo/packages/b"),
		];
		let mut files = HashSet::new();
		files.insert("other/random.txt".to_string());
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![false, false]
		);
	}

	#[test]
	fn match_files_to_projects_multiple_at_same_path_all_marked() {
		// When multiple projects share the same path (e.g. Cargo and npm at the repo root),
		// all of them are marked changed when a file in their shared directory changes.
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("npm-root", "/repo"),
			Project::new_test("cargo-root", "/repo"),
			Project::new_test("sub", "/repo/packages/sub"),
		];
		let mut files = HashSet::new();
		files.insert("README.md".to_string());
		// README.md is not under packages/sub, so only the two root projects match.
		// Both share priority 0 (root), so both must be marked.
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![true, true, false]
		);
	}

	#[test]
	fn match_files_to_projects_exact_path_length_match() {
		// A changed file whose path is exactly equal to the project's relative path
		// (e.g. "my-pkg" as a changed file, project at "/repo/my-pkg").
		// Guards `==`→`!=` on `file.len() == rel.len()` boundary check.
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![Project::new_test("my-pkg", "/repo/my-pkg")];
		let mut files = HashSet::new();
		files.insert("my-pkg".to_string()); // exactly matches rel path, no trailing /
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![true]
		);
	}

	#[test]
	fn match_files_to_projects_multiple_at_same_path_subproject_wins() {
		// When multiple projects share the same root path, a deeper subproject
		// still wins for files inside it — the shared-root projects are not marked.
		let path = AbsolutePath::new("/repo").unwrap();
		let projects = vec![
			Project::new_test("npm-root", "/repo"),
			Project::new_test("cargo-root", "/repo"),
			Project::new_test("sub", "/repo/packages/sub"),
		];
		let mut files = HashSet::new();
		files.insert("packages/sub/src/lib.rs".to_string());
		assert_eq!(
			match_files_to_projects(&projects, &path, &files),
			vec![false, false, true]
		);
	}
}
