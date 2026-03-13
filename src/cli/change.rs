//! The `change` subcommand.

use std::collections::{BTreeMap, HashSet};
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::Args;

use crate::git;
use crate::model::changeset::{ChangeType, Changeset};
use crate::model::config::Config;
use crate::tui::change;

use super::GlobalArgs;

/// Arguments for the `change` subcommand.
#[derive(Args, Default)]
pub struct ChangeArgs {
	/// Type of change: major, minor, or patch (required in non-interactive mode)
	#[arg(short = 't', long)]
	pub change_type: Option<ChangeType>,

	/// Project name(s) to include (repeatable; defaults to all in non-interactive mode)
	#[arg(short = 'p', long = "project")]
	pub projects: Vec<String>,

	/// Description message for the changeset (required in non-interactive mode)
	#[arg(short = 'm', long)]
	pub message: Option<String>,
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

	projects
		.iter()
		.map(|project| {
			let project_path = project.path();
			let git_path = git.path();
			let rel = match project_path.strip_prefix(git_path.as_path()) {
				Ok(r) => r,
				// Project path is not under the git root — treat as changed to avoid
				// silently hiding it from the "Changed" group.
				Err(_) => return true,
			};
			let rel_str = rel.to_string_lossy();
			if rel_str.is_empty() {
				// Root project: any changed file counts
				!changed_files.is_empty()
			} else {
				changed_files.iter().any(|file| {
					file.starts_with(rel_str.as_ref())
						&& (file.len() == rel_str.len()
							|| file.as_bytes().get(rel_str.len()) == Some(&b'/'))
				})
			}
		})
		.collect()
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

/// Runs the `change` subcommand.
pub(crate) fn cmd_change(
	git: &git::GitWorkdir,
	args: &ChangeArgs,
	global: &GlobalArgs,
	config: Config,
) -> anyhow::Result<ExitCode> {
	let env = config.env().context("env not set")?;
	let projects = config.load_projects()?;

	let project_indices = resolve_project_indices(&projects, &args.projects)?;

	let result = if global.no_interactive {
		let Some(ct) = args.change_type else {
			bail!("--change-type is required in non-interactive mode");
		};
		if args.message.is_none() {
			bail!("--message is required in non-interactive mode");
		}
		let selected_projects = match &project_indices {
			Some(indices) => indices.iter().map(|&i| projects[i].clone()).collect(),
			None => projects.clone(),
		};
		change::ChangeResult {
			projects: selected_projects,
			change_type: ct,
		}
	} else {
		let options = change::ChangeOptions {
			change_type: args.change_type,
			projects: project_indices,
		};
		let changed = classify_changed_projects(git, &projects);
		match change::run(&projects, &options, &changed)? {
			Some(r) => r,
			None => return Ok(ExitCode::from(2)),
		}
	};

	let packages: BTreeMap<String, ChangeType> = result
		.projects
		.iter()
		.map(|p| (p.name().to_string(), result.change_type))
		.collect();

	let changeset = Changeset::new(packages, args.message.clone());

	let path = changeset.write(git)?;

	if args.message.is_none() {
		env.run_editor_on(&path, git.path())?;
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
}
