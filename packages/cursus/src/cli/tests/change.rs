use std::path::Path;
use std::process::Output;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::cli::change::*;
use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::model::changeset::ChangeType;
use crate::package_manager::Project;
use crate::path::AbsolutePath;

fn make_git_with_diff_output(stdout: &[u8]) -> crate::git::GitWorkdir {
	let runner = Arc::new(RecordingCommandRunner::new(0).with_stdout(stdout.to_vec()))
		as Arc<dyn CommandRunner>;
	crate::git::GitWorkdir::new(runner, AbsolutePath::new("/nonexistent").unwrap())
}

fn make_git_failing() -> crate::git::GitWorkdir {
	let runner = Arc::new(RecordingCommandRunner::new(1)) as Arc<dyn CommandRunner>;
	crate::git::GitWorkdir::new(runner, AbsolutePath::new("/nonexistent").unwrap())
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

#[async_trait]
impl CommandRunner for SequencedRunner {
	async fn run(&self, _program: &str, _args: &[&str], _cwd: &Path) -> anyhow::Result<Output> {
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

	async fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.run(program, args, cwd).await
	}

	async fn run_interactive(
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

	async fn run_shell_interactive(
		&self,
		_command: &str,
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

	async fn run_streaming(
		&self,
		_command: &str,
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

fn make_git_sequenced(responses: Vec<(i32, Vec<u8>)>) -> crate::git::GitWorkdir {
	let runner = Arc::new(SequencedRunner::new(responses)) as Arc<dyn CommandRunner>;
	crate::git::GitWorkdir::new(runner, AbsolutePath::new("/nonexistent").unwrap())
}

#[tokio::test]
async fn default_change_args() {
	let args = ChangeArgs::default();
	assert!(args.change_type.is_none());
	assert!(args.projects.is_empty());
	assert!(args.message.is_none());
	assert!(!args.auto);
	assert!(!args.no_git);
}

#[tokio::test]
async fn classify_changed_projects_matches_by_prefix() {
	let git = make_git_with_diff_output(b"packages/a/src/lib.rs\n");
	let projects = vec![
		Project::new_test("a", "/nonexistent/packages/a"),
		Project::new_test("b", "/nonexistent/packages/b"),
	];
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![true, false]);
}

#[tokio::test]
async fn classify_changed_projects_does_not_match_prefix_without_separator() {
	// "packages/a-extra/foo.rs" must not match project "packages/a"
	let git = make_git_with_diff_output(b"packages/a-extra/foo.rs\n");
	let projects = vec![
		Project::new_test("a", "/nonexistent/packages/a"),
		Project::new_test("a-extra", "/nonexistent/packages/a-extra"),
	];
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![false, true]);
}

#[tokio::test]
async fn classify_changed_projects_fallback_on_failure() {
	let git = make_git_failing();
	let projects = vec![
		Project::new_test("a", "/nonexistent/packages/a"),
		Project::new_test("b", "/nonexistent/packages/b"),
	];
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![true, true]);
}

#[tokio::test]
async fn classify_changed_projects_empty_diff_returns_unchanged() {
	let git = make_git_with_diff_output(b"");
	let projects = vec![Project::new_test("a", "/nonexistent/packages/a")];
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![false]);
}

#[tokio::test]
async fn classify_changed_projects_root_project_changed_when_any_file_changed() {
	let git = make_git_with_diff_output(b"src/main.rs\n");
	let projects = vec![Project::new_test("root", "/nonexistent")];
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![true]);
}

#[tokio::test]
async fn classify_changed_projects_root_project_unchanged_when_empty_diff() {
	let git = make_git_with_diff_output(b"");
	let projects = vec![Project::new_test("root", "/nonexistent")];
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![false]);
}

#[tokio::test]
async fn classify_changed_projects_detects_staged_only_changes() {
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
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![true, false]);
}

#[tokio::test]
async fn classify_changed_projects_detects_unstaged_only_changes() {
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
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![false, true]);
}

#[tokio::test]
async fn classify_changed_projects_unions_all_sources() {
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
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![true, true, true]);
}

#[tokio::test]
async fn classify_changed_projects_fallback_only_when_all_fail() {
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
	let result = classify_changed_projects(&git, &projects, &projects).await;
	assert_eq!(result, vec![true, true]);
}

#[tokio::test]
async fn classify_changed_projects_ignored_subproject_blocks_parent_attribution() {
	// foo/tests is ignored (not in `projects`), but is in `all_projects`.
	// A file inside foo/tests must NOT be attributed to foo.
	let git = make_git_with_diff_output(b"packages/foo/tests/README.md\n");
	let projects = vec![
		Project::new_test("root", "/nonexistent"),
		Project::new_test("foo", "/nonexistent/packages/foo"),
	];
	let all_projects = vec![
		Project::new_test("root", "/nonexistent"),
		Project::new_test("foo", "/nonexistent/packages/foo"),
		Project::new_test("foo-tests", "/nonexistent/packages/foo/tests"),
	];
	let result = classify_changed_projects(&git, &projects, &all_projects).await;
	assert_eq!(result, vec![false, false]);
}

fn make_args(change_type: ChangeType, message: &str) -> ChangeArgs {
	ChangeArgs {
		change_type: Some(change_type),
		message: Some(message.to_string()),
		..Default::default()
	}
}

#[test]
fn resolve_non_interactive_uses_changed_when_no_project_flags() {
	let projects = vec![
		Project::new_test("a", "/nonexistent/packages/a"),
		Project::new_test("b", "/nonexistent/packages/b"),
	];
	let args = make_args(ChangeType::Patch, "fix thing");
	let result = resolve_non_interactive(&args, &projects, &None, &[true, false]).unwrap();
	let names: Vec<_> = result.projects.iter().map(|(p, _)| p.name()).collect();
	assert_eq!(names, vec!["a"]);
}

#[test]
fn resolve_non_interactive_falls_back_to_all_when_no_changes() {
	let projects = vec![
		Project::new_test("a", "/nonexistent/packages/a"),
		Project::new_test("b", "/nonexistent/packages/b"),
	];
	let args = make_args(ChangeType::Patch, "fix thing");
	let result = resolve_non_interactive(&args, &projects, &None, &[false, false]).unwrap();
	let names: Vec<_> = result.projects.iter().map(|(p, _)| p.name()).collect();
	assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn resolve_non_interactive_all_changed_returns_all() {
	let projects = vec![
		Project::new_test("a", "/nonexistent/packages/a"),
		Project::new_test("b", "/nonexistent/packages/b"),
	];
	let args = make_args(ChangeType::Minor, "feat thing");
	let result = resolve_non_interactive(&args, &projects, &None, &[true, true]).unwrap();
	let names: Vec<_> = result.projects.iter().map(|(p, _)| p.name()).collect();
	assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn resolve_non_interactive_explicit_projects_override_changed() {
	let projects = vec![
		Project::new_test("a", "/nonexistent/packages/a"),
		Project::new_test("b", "/nonexistent/packages/b"),
	];
	let args = make_args(ChangeType::Patch, "fix thing");
	// Explicit: only project index 0 ("a"); changed says only "b" changed
	let result = resolve_non_interactive(&args, &projects, &Some(vec![0]), &[false, true]).unwrap();
	let names: Vec<_> = result.projects.iter().map(|(p, _)| p.name()).collect();
	assert_eq!(names, vec!["a"]);
}

#[test]
fn resolve_non_interactive_explicit_projects_override_when_all_changed() {
	let projects = vec![
		Project::new_test("a", "/nonexistent/packages/a"),
		Project::new_test("b", "/nonexistent/packages/b"),
	];
	let args = make_args(ChangeType::Patch, "fix thing");
	// Explicit: only project index 0 ("a"); both projects are changed — explicit still wins
	let result = resolve_non_interactive(&args, &projects, &Some(vec![0]), &[true, true]).unwrap();
	let names: Vec<_> = result.projects.iter().map(|(p, _)| p.name()).collect();
	assert_eq!(names, vec!["a"]);
}
