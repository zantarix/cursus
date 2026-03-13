//! Application environment threaded through the library boundary.

use std::path::Path;
use std::process::{ExitStatus, Output};
use std::sync::Arc;

use crate::command::{CommandRunner, DryRunCommandRunner};
use crate::github::client::GitHubClient;

/// Environment variables and runtime dependencies used by Cursus.
///
/// Populated from the process environment at the binary boundary and threaded
/// into the library so that internal functions never read `std::env` directly.
/// Also carries the [`CommandRunner`] so that all I/O can be intercepted in tests,
/// and an optional [`GitHubClient`] for GitHub operations.
#[derive(Debug, Clone)]
pub struct Env {
	/// The configured editor for opening changeset files.
	///
	/// Resolved from `VISUAL` then `EDITOR` in the binary entry point.
	editor: Option<String>,
	/// The command runner used for all external process invocations.
	runner: Arc<dyn CommandRunner>,
	/// The GitHub client for API operations, if a token was provided.
	github_client: Option<Arc<dyn GitHubClient>>,
}

impl Env {
	/// Creates an `Env` with the given command runner.
	///
	/// Use the builder methods ([`with_editor`][Self::with_editor],
	/// [`with_github_client`][Self::with_github_client]) to add optional configuration.
	pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
		Self {
			runner,
			editor: None,
			github_client: None,
		}
	}

	/// Sets the editor to open changeset files with.
	pub fn with_editor(mut self, editor: String) -> Self {
		self.editor = Some(editor);
		self
	}

	/// Sets the GitHub client for API operations.
	pub fn with_github_client(mut self, client: Arc<dyn GitHubClient>) -> Self {
		self.github_client = Some(client);
		self
	}

	/// Sets the editor from an `Option`, overwriting any previously set value.
	///
	/// Passing `None` clears a previously set editor.
	pub fn with_editor_opt(mut self, editor: Option<String>) -> Self {
		self.editor = editor;
		self
	}

	/// Sets the GitHub client from an `Option`, overwriting any previously set value.
	///
	/// Passing `None` clears a previously set client.
	pub fn with_github_client_opt(mut self, client: Option<Arc<dyn GitHubClient>>) -> Self {
		self.github_client = client;
		self
	}

	/// Wraps the current command runner in a [`DryRunCommandRunner`] that suppresses
	/// all mutating operations.
	///
	/// This is called automatically by [`crate::run_with`] when `--dry-run` is set,
	/// so all code paths (both the binary and integration tests) benefit from the
	/// dry-run protection without any manual composition.
	pub(crate) fn with_dry_run_runner(self) -> Self {
		let dry_runner: Arc<dyn CommandRunner> =
			Arc::new(DryRunCommandRunner::new(Arc::clone(&self.runner)));
		Self {
			runner: dry_runner,
			editor: self.editor,
			github_client: self.github_client,
		}
	}

	/// Returns the configured editor, if one was set.
	pub(crate) fn editor(&self) -> Option<&str> {
		self.editor.as_deref()
	}

	/// Returns the GitHub client, if one was configured.
	pub(crate) fn github_client(&self) -> Option<&dyn GitHubClient> {
		self.github_client.as_deref()
	}

	/// Finds a default editor by checking for `nano`, `vim`, then `vi` on the system PATH.
	fn find_default_editor(&self, cwd: &Path) -> Option<String> {
		["nano", "vim", "vi", "emacs"]
			.into_iter()
			.find(|cmd| {
				self.run("which", &[cmd], cwd)
					.is_ok_and(|o| o.status.success())
			})
			.map(String::from)
	}

	/// Opens the user's editor on the specified file.
	///
	/// Resolves the editor from `self.editor()`, falling back to the first
	/// available editor from `nano`, `vim`, `vi`, or `emacs`. The working directory for
	/// the editor process is `cwd`.
	///
	/// # Errors
	///
	/// Returns an error if no editor is found or the editor process fails.
	pub fn run_editor_on(&self, path: &Path, cwd: &Path) -> anyhow::Result<()> {
		use anyhow::Context as _;
		let editor = self
			.editor()
			.filter(|v| !v.is_empty())
			.map(String::from)
			.or_else(|| self.find_default_editor(cwd))
			.context("No editor found. Set the VISUAL or EDITOR environment variable.")?;
		let path_str = path.to_string_lossy();
		let status = self
			.run_interactive(&editor, &[path_str.as_ref()], cwd)
			.with_context(|| format!("Failed to open editor: {editor}"))?;
		if !status.success() {
			anyhow::bail!("Editor exited with status: {status}");
		}
		Ok(())
	}

	/// Runs a program with the given arguments in the specified directory.
	///
	/// Delegates to the underlying [`CommandRunner`]. Read-only.
	pub fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.runner.run(program, args, cwd)
	}

	/// Runs a shell command via `/bin/sh -c` in the specified directory.
	///
	/// Delegates to the underlying [`CommandRunner`]. Read-only.
	pub fn run_shell(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
		self.runner.run_shell(command, cwd)
	}

	/// Runs a mutating program with the given arguments in the specified directory.
	///
	/// Delegates to the underlying [`CommandRunner`]. Skipped by [`DryRunCommandRunner`].
	pub fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.runner.run_mut(program, args, cwd)
	}

	/// Runs a mutating shell command via `/bin/sh -c` in the specified directory.
	///
	/// Delegates to the underlying [`CommandRunner`]. Skipped by [`DryRunCommandRunner`].
	pub fn run_shell_mut(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
		self.runner.run_shell_mut(command, cwd)
	}

	/// Runs a program with inherited stdin/stdout/stderr for interactive use.
	///
	/// Delegates to the underlying [`CommandRunner`]. Skipped by [`DryRunCommandRunner`].
	pub fn run_interactive(
		&self,
		program: &str,
		args: &[&str],
		cwd: &Path,
	) -> anyhow::Result<ExitStatus> {
		self.runner.run_interactive(program, args, cwd)
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;
	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::github::client::GitHubClient;
	use crate::github::client::test_support::RecordingGitHubClient;

	use super::*;

	fn recording_env(exit_code: i32) -> (Arc<RecordingCommandRunner>, Env) {
		let runner = Arc::new(RecordingCommandRunner::new(exit_code));
		let env = Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		(runner, env)
	}

	#[test]
	fn new_has_no_editor_or_github_client() {
		let (_, env) = recording_env(0);
		assert!(env.editor().is_none());
		assert!(env.github_client().is_none());
	}

	#[test]
	fn with_editor_sets_editor() {
		let (_, env) = recording_env(0);
		let env = env.with_editor("vim".to_string());
		assert_eq!(env.editor(), Some("vim"));
	}

	#[test]
	fn with_github_client_sets_client() {
		let (_, env) = recording_env(0);
		let client = Arc::new(RecordingGitHubClient::new()) as Arc<dyn GitHubClient>;
		let env = env.with_github_client(Arc::clone(&client));
		assert!(env.github_client().is_some());
	}

	#[test]
	fn with_editor_opt_some_sets_editor() {
		let (_, env) = recording_env(0);
		let env = env.with_editor_opt(Some("nano".to_string()));
		assert_eq!(env.editor(), Some("nano"));
	}

	#[test]
	fn with_editor_opt_none_clears_editor() {
		let (_, env) = recording_env(0);
		let env = env.with_editor("vim".to_string()).with_editor_opt(None);
		assert!(env.editor().is_none());
	}

	#[test]
	fn with_github_client_opt_some_sets_client() {
		let (_, env) = recording_env(0);
		let client = Arc::new(RecordingGitHubClient::new()) as Arc<dyn GitHubClient>;
		let env = env.with_github_client_opt(Some(client));
		assert!(env.github_client().is_some());
	}

	#[test]
	fn with_github_client_opt_none_clears_client() {
		let (_, env) = recording_env(0);
		let client = Arc::new(RecordingGitHubClient::new()) as Arc<dyn GitHubClient>;
		let env = env.with_github_client(client).with_github_client_opt(None);
		assert!(env.github_client().is_none());
	}

	#[test]
	fn run_delegates_to_runner() {
		let (runner, env) = recording_env(0);
		env.run("echo", &["hello"], Path::new(".")).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, "echo");
		assert_eq!(invocations[0].args, ["hello"]);
	}

	#[test]
	fn run_shell_delegates_to_runner() {
		let (runner, env) = recording_env(0);
		env.run_shell("echo hello", Path::new(".")).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, "/bin/sh");
		assert_eq!(invocations[0].args, ["-c", "echo hello"]);
	}

	#[test]
	fn run_mut_delegates_to_runner() {
		let (runner, env) = recording_env(0);
		env.run_mut("git", &["commit", "-m", "msg"], Path::new("."))
			.unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, ["commit", "-m", "msg"]);
	}

	#[test]
	fn run_shell_mut_delegates_to_runner() {
		let (runner, env) = recording_env(0);
		env.run_shell_mut("npm install", Path::new(".")).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, "/bin/sh");
		assert!(invocations[0].is_shell);
	}

	#[test]
	fn run_interactive_delegates_to_runner() {
		let (runner, env) = recording_env(0);
		env.run_interactive("vim", &[], Path::new(".")).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, "vim");
		assert!(invocations[0].is_interactive);
	}

	#[test]
	fn with_dry_run_runner_suppresses_run_mut() {
		let (runner, env) = recording_env(0);
		let dry_env = env.with_dry_run_runner();
		dry_env
			.run_mut("git", &["push", "origin", "HEAD"], Path::new("."))
			.unwrap();
		// The inner recording runner must NOT have been called (DryRunCommandRunner intercepts)
		assert!(runner.invocations().is_empty());
	}

	#[test]
	fn with_dry_run_runner_still_forwards_run() {
		let (runner, env) = recording_env(0);
		let dry_env = env.with_dry_run_runner();
		dry_env.run("git", &["status"], Path::new(".")).unwrap();
		// Read-only run is forwarded to the inner runner
		assert_eq!(runner.invocations().len(), 1);
		assert_eq!(runner.invocations()[0].program, "git");
	}

	// run_editor_on tests

	#[test]
	fn run_editor_on_uses_editor_when_set() {
		let workdir = tempfile::tempdir().unwrap();
		let path = workdir.path().join("config.toml");
		std::fs::write(&path, "").unwrap();

		let (runner, env) = recording_env(0);
		let env = env.with_editor("vim".to_string());
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, "vim");
		assert!(invocations[0].is_interactive);
	}

	#[test]
	fn run_editor_on_ignores_empty_editor_string() {
		let workdir = tempfile::tempdir().unwrap();
		let path = workdir.path().join("config.toml");
		std::fs::write(&path, "").unwrap();

		// Empty editor → falls back to find_default_editor → runner returns 0 → "nano"
		let (runner, env) = recording_env(0);
		let env = env.with_editor(String::new());
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		let editor_call = invocations.last().unwrap();
		assert_eq!(
			editor_call.program, "nano",
			"Should fall back to nano when editor is empty"
		);
	}

	#[test]
	fn run_editor_on_nonzero_exit_returns_error() {
		let workdir = tempfile::tempdir().unwrap();
		let path = workdir.path().join("config.toml");
		std::fs::write(&path, "").unwrap();

		let (_, env) = recording_env(1);
		let env = env.with_editor("vim".to_string());
		let result = env.run_editor_on(&path, workdir.path());

		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("Editor exited with status")
		);
	}

	#[test]
	fn run_editor_on_falls_back_to_default_editor() {
		let workdir = tempfile::tempdir().unwrap();
		let path = workdir.path().join("config.toml");
		std::fs::write(&path, "").unwrap();

		// No editor set, runner exit_code=0 → which nano succeeds → "nano"
		let (runner, env) = recording_env(0);
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		let editor_call = invocations.last().unwrap();
		assert_eq!(editor_call.program, "nano");
	}

	#[test]
	fn run_editor_on_no_editor_found_returns_error() {
		let workdir = tempfile::tempdir().unwrap();
		let path = workdir.path().join("config.toml");
		std::fs::write(&path, "").unwrap();

		// Runner exit_code=1 → all which calls fail → no default found
		let (_, env) = recording_env(1);
		let result = env.run_editor_on(&path, workdir.path());

		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("No editor found"));
	}

	#[test]
	fn run_editor_on_uses_provided_cwd() {
		let workdir = tempfile::tempdir().unwrap();
		let cursus_dir = workdir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		let path = cursus_dir.join("config.toml");
		std::fs::write(&path, "").unwrap();

		let (runner, env) = recording_env(0);
		let env = env.with_editor("vim".to_string());
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		let editor_call = invocations
			.iter()
			.find(|i| i.is_interactive)
			.expect("Expected an interactive editor invocation");
		assert_eq!(
			editor_call.cwd,
			workdir.path(),
			"Editor should be invoked with the provided cwd, not the file's parent"
		);
	}
}
