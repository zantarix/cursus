//! Application environment threaded through the library boundary.

use std::path::Path;
use std::process::{ExitStatus, Output};
use std::sync::Arc;

use crate::command::{CommandRunner, DryRunCommandRunner};
use crate::filesystem::Filesystem;
use crate::git::Git;
use crate::github::client::GitHubClient;

/// Environment variables and runtime dependencies used by Cursus.
///
/// Populated from the process environment at the binary boundary and threaded
/// into the library so that internal functions never read `std::env` directly.
/// Carries the [`CommandRunner`], [`Filesystem`], [`Git`], and optional
/// [`GitHubClient`] so that all I/O can be intercepted or replaced.
#[derive(Debug, Clone)]
pub struct Env {
	/// The configured editor for opening changeset files.
	///
	/// Resolved from `VISUAL` then `EDITOR` in the binary entry point.
	editor: Option<String>,
	/// The command runner used for all external process invocations.
	runner: Arc<dyn CommandRunner>,
	/// The filesystem implementation used for all file I/O.
	filesystem: Arc<dyn Filesystem>,
	/// The git implementation for repository operations.
	///
	/// Must be constructed after dry-run wrapping so that
	/// [`GitWorkdir`][crate::git::GitWorkdir] receives the wrapped
	/// [`CommandRunner`].
	git: Arc<dyn Git>,
	/// The GitHub client for API operations, if a token was provided.
	github_client: Option<Arc<dyn GitHubClient>>,
	/// Whether an OIDC-capable CI environment is detected.
	///
	/// `true` when `ACTIONS_ID_TOKEN_REQUEST_URL` (GitHub Actions) or
	/// `CI_JOB_JWT_V2` (GitLab CI) is set.
	oidc_environment: bool,
	/// Whether `NODE_AUTH_TOKEN` is set in the environment.
	node_auth_token_present: bool,
	/// Whether `CARGO_REGISTRY_TOKEN` is set in the environment.
	cargo_registry_token_present: bool,
	/// The BCP 47 locale tag to use for all user-visible messages.
	///
	/// Resolved from `CURSUS_LOCALE`, then the system locale, then `"en"` by
	/// the binary entry point. The library never reads locale environment
	/// variables directly.
	locale: String,
}

impl Env {
	/// Creates an `Env` with the given command runner, filesystem, and git implementation.
	///
	/// Use the builder methods ([`with_editor`][Self::with_editor],
	/// [`with_github_client`][Self::with_github_client]) to add optional configuration.
	pub fn new(
		runner: Arc<dyn CommandRunner>,
		filesystem: Arc<dyn Filesystem>,
		git: Arc<dyn Git>,
	) -> Self {
		Self {
			runner,
			filesystem,
			git,
			editor: None,
			github_client: None,
			oidc_environment: false,
			node_auth_token_present: false,
			cargo_registry_token_present: false,
			locale: crate::locale::DEFAULT_LOCALE.to_string(),
		}
	}

	/// Sets whether an OIDC-capable CI environment is detected.
	pub fn with_oidc_environment(mut self, oidc_environment: bool) -> Self {
		self.oidc_environment = oidc_environment;
		self
	}

	/// Sets whether `NODE_AUTH_TOKEN` is present in the environment.
	pub fn with_node_auth_token_present(mut self, present: bool) -> Self {
		self.node_auth_token_present = present;
		self
	}

	/// Sets whether `CARGO_REGISTRY_TOKEN` is present in the environment.
	pub fn with_cargo_registry_token_present(mut self, present: bool) -> Self {
		self.cargo_registry_token_present = present;
		self
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

	/// Sets the locale for all user-visible messages.
	///
	/// The `locale` string should be a BCP 47 tag (e.g. `"en"`, `"en-US"`,
	/// `"pt-BR"`). Defaults to `"en"`.
	pub fn with_locale(mut self, locale: String) -> Self {
		self.locale = locale;
		self
	}

	/// Wraps the current command runner in a [`DryRunCommandRunner`] that suppresses
	/// all mutating operations.
	///
	/// This is called automatically by [`crate::run_with`] when `--dry-run` is set,
	/// so all code paths (both the binary and integration tests) benefit from the
	/// dry-run protection without any manual composition.
	pub fn with_dry_run_runner(self) -> Self {
		let dry_runner: Arc<dyn CommandRunner> =
			Arc::new(DryRunCommandRunner::new(Arc::clone(&self.runner)));
		Self {
			runner: dry_runner,
			filesystem: self.filesystem,
			editor: self.editor,
			git: self.git,
			github_client: self.github_client,
			oidc_environment: self.oidc_environment,
			node_auth_token_present: self.node_auth_token_present,
			cargo_registry_token_present: self.cargo_registry_token_present,
			locale: self.locale,
		}
	}

	/// Applies global CLI flags to this environment.
	///
	/// Currently handles `--dry-run` by wrapping the command runner in a
	/// [`DryRunCommandRunner`].
	pub fn apply_global(self, global: &crate::cli::GlobalArgs) -> Self {
		if global.dry_run {
			self.with_dry_run_runner()
		} else {
			self
		}
	}

	/// Returns the configured editor, if one was set.
	pub(crate) fn editor(&self) -> Option<&str> {
		self.editor.as_deref()
	}

	/// Returns the filesystem implementation.
	pub fn fs(&self) -> &dyn Filesystem {
		&*self.filesystem
	}

	/// Returns the command runner.
	pub fn runner(&self) -> Arc<dyn CommandRunner> {
		Arc::clone(&self.runner)
	}

	/// Returns the git implementation.
	pub fn git(&self) -> &dyn Git {
		&*self.git
	}

	/// Returns the GitHub client, if one was configured.
	pub(crate) fn github_client(&self) -> Option<&dyn GitHubClient> {
		self.github_client.as_deref()
	}

	/// Returns `true` when an OIDC-capable CI environment is detected.
	pub(crate) fn oidc_environment(&self) -> bool {
		self.oidc_environment
	}

	/// Returns `true` when `NODE_AUTH_TOKEN` is present in the environment.
	pub(crate) fn node_auth_token_present(&self) -> bool {
		self.node_auth_token_present
	}

	/// Returns `true` when `CARGO_REGISTRY_TOKEN` is present in the environment.
	pub(crate) fn cargo_registry_token_present(&self) -> bool {
		self.cargo_registry_token_present
	}

	/// Returns the BCP 47 locale tag for user-visible messages.
	pub(crate) fn locale(&self) -> &str {
		&self.locale
	}

	/// Finds a default editor by probing well-known editors on the system PATH.
	///
	/// On Windows, checks for `notepad` via `where.exe`. On Unix, checks for
	/// `nano`, `vim`, `vi`, and `emacs` via `which`.
	fn find_default_editor(&self, cwd: &Path) -> Option<String> {
		let (probe_cmd, candidates): (&str, &[&str]) = if cfg!(windows) {
			("where.exe", &["notepad"])
		} else {
			("which", &["nano", "vim", "vi", "emacs"])
		};
		candidates
			.iter()
			.copied()
			.find(|cmd| {
				self.run(probe_cmd, &[cmd], cwd)
					.is_ok_and(|o| o.status.success())
			})
			.map(String::from)
	}

	/// Opens the user's editor on the specified file.
	///
	/// Resolves the editor from `self.editor()`, falling back to the first
	/// available platform-appropriate editor: `notepad` on Windows, or the first
	/// of `nano`, `vim`, `vi`, `emacs` found on Unix. The working directory for
	/// the editor process is `cwd`.
	///
	/// The editor string is passed to [`run_shell_interactive`][Self::run_shell_interactive]
	/// so that multi-word values such as `code --wait` are interpreted correctly by the
	/// shell. The file path is quoted via [`crate::shell::shell_quote`] to prevent word
	/// splitting on filenames that contain spaces or other special characters.
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
		let shell_cmd = format!("{editor} {}", crate::shell::shell_quote(&path_str));
		let status = self
			.run_shell_interactive(&shell_cmd, cwd)
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

	/// Runs a shell command via the platform shell in the specified directory.
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

	/// Runs a mutating shell command via the platform shell in the specified directory.
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

	/// Runs a shell command via the platform shell with inherited stdin/stdout/stderr.
	///
	/// Delegates to the underlying [`CommandRunner`]. Skipped by [`DryRunCommandRunner`].
	pub fn run_shell_interactive(&self, command: &str, cwd: &Path) -> anyhow::Result<ExitStatus> {
		self.runner.run_shell_interactive(command, cwd)
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;
	use std::sync::Arc;

	use crate::command::test_support::RecordingCommandRunner;
	use crate::command::{CommandRunner, shell_flag, shell_program};
	use crate::filesystem::LocalFilesystem;
	use crate::github::client::GitHubClient;
	use crate::github::client::test_support::RecordingGitHubClient;

	use super::*;

	fn recording_env(exit_code: i32) -> (Arc<RecordingCommandRunner>, Env, tempfile::TempDir) {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(exit_code));
		let git = Arc::new(crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			crate::path::AbsolutePath::new(dir.path()).unwrap(),
		));
		let env = Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
			git,
		);
		(runner, env, dir)
	}

	#[test]
	fn new_has_no_editor_or_github_client() {
		let (_, env, _dir) = recording_env(0);
		assert!(env.editor().is_none());
		assert!(env.github_client().is_none());
	}

	#[test]
	fn new_has_false_auth_flags() {
		let (_, env, _dir) = recording_env(0);
		assert!(!env.oidc_environment());
		assert!(!env.node_auth_token_present());
		assert!(!env.cargo_registry_token_present());
	}

	#[test]
	fn with_oidc_environment_sets_flag() {
		let (_, env, _dir) = recording_env(0);
		let env = env.with_oidc_environment(true);
		assert!(env.oidc_environment());
	}

	#[test]
	fn with_node_auth_token_present_sets_flag() {
		let (_, env, _dir) = recording_env(0);
		let env = env.with_node_auth_token_present(true);
		assert!(env.node_auth_token_present());
	}

	#[test]
	fn with_cargo_registry_token_present_sets_flag() {
		let (_, env, _dir) = recording_env(0);
		let env = env.with_cargo_registry_token_present(true);
		assert!(env.cargo_registry_token_present());
	}

	#[test]
	fn new_has_default_locale() {
		let (_, env, _dir) = recording_env(0);
		assert_eq!(env.locale(), crate::locale::DEFAULT_LOCALE);
	}

	#[test]
	fn with_locale_sets_locale() {
		let (_, env, _dir) = recording_env(0);
		let env = env.with_locale("pt-BR".to_string());
		assert_eq!(env.locale(), "pt-BR");
	}

	#[test]
	fn with_dry_run_runner_preserves_auth_flags() {
		let (_, env, _dir) = recording_env(0);
		let env = env
			.with_oidc_environment(true)
			.with_node_auth_token_present(true)
			.with_cargo_registry_token_present(true);
		let dry_env = env.with_dry_run_runner();
		assert!(dry_env.oidc_environment());
		assert!(dry_env.node_auth_token_present());
		assert!(dry_env.cargo_registry_token_present());
	}

	#[test]
	fn with_dry_run_runner_preserves_locale() {
		let (_, env, _dir) = recording_env(0);
		let env = env.with_locale("fr".to_string());
		let dry_env = env.with_dry_run_runner();
		assert_eq!(dry_env.locale(), "fr");
	}

	#[test]
	fn with_dry_run_runner_preserves_git() {
		let (_, env, _dir) = recording_env(0);
		let path = env.git().path().clone();
		let dry_env = env.with_dry_run_runner();
		assert_eq!(dry_env.git().path(), &path);
	}

	#[test]
	fn with_editor_sets_editor() {
		let (_, env, _dir) = recording_env(0);
		let env = env.with_editor("vim".to_string());
		assert_eq!(env.editor(), Some("vim"));
	}

	#[test]
	fn with_github_client_sets_client() {
		let (_, env, _dir) = recording_env(0);
		let client = Arc::new(RecordingGitHubClient::new()) as Arc<dyn GitHubClient>;
		let env = env.with_github_client(Arc::clone(&client));
		assert!(env.github_client().is_some());
	}

	#[test]
	fn with_editor_opt_some_sets_editor() {
		let (_, env, _dir) = recording_env(0);
		let env = env.with_editor_opt(Some("nano".to_string()));
		assert_eq!(env.editor(), Some("nano"));
	}

	#[test]
	fn with_editor_opt_none_clears_editor() {
		let (_, env, _dir) = recording_env(0);
		let env = env.with_editor("vim".to_string()).with_editor_opt(None);
		assert!(env.editor().is_none());
	}

	#[test]
	fn with_github_client_opt_some_sets_client() {
		let (_, env, _dir) = recording_env(0);
		let client = Arc::new(RecordingGitHubClient::new()) as Arc<dyn GitHubClient>;
		let env = env.with_github_client_opt(Some(client));
		assert!(env.github_client().is_some());
	}

	#[test]
	fn with_github_client_opt_none_clears_client() {
		let (_, env, _dir) = recording_env(0);
		let client = Arc::new(RecordingGitHubClient::new()) as Arc<dyn GitHubClient>;
		let env = env.with_github_client(client).with_github_client_opt(None);
		assert!(env.github_client().is_none());
	}

	#[test]
	fn run_delegates_to_runner() {
		let (runner, env, _dir) = recording_env(0);
		env.run("echo", &["hello"], Path::new(".")).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, "echo");
		assert_eq!(invocations[0].args, ["hello"]);
	}

	#[test]
	fn run_shell_delegates_to_runner() {
		let (runner, env, _dir) = recording_env(0);
		env.run_shell("echo hello", Path::new(".")).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, shell_program());
		assert_eq!(invocations[0].args, [shell_flag(), "echo hello"]);
	}

	#[test]
	fn run_mut_delegates_to_runner() {
		let (runner, env, _dir) = recording_env(0);
		env.run_mut("git", &["commit", "-m", "msg"], Path::new("."))
			.unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, ["commit", "-m", "msg"]);
	}

	#[test]
	fn run_shell_mut_delegates_to_runner() {
		let (runner, env, _dir) = recording_env(0);
		env.run_shell_mut("npm install", Path::new(".")).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, shell_program());
		assert!(invocations[0].is_shell);
	}

	#[test]
	fn run_interactive_delegates_to_runner() {
		let (runner, env, _dir) = recording_env(0);
		env.run_interactive("vim", &[], Path::new(".")).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations[0].program, "vim");
		assert!(invocations[0].is_interactive);
	}

	#[test]
	fn with_dry_run_runner_suppresses_run_mut() {
		let (runner, env, _dir) = recording_env(0);
		let dry_env = env.with_dry_run_runner();
		dry_env
			.run_mut("git", &["push", "origin", "HEAD"], Path::new("."))
			.unwrap();
		// The inner recording runner must NOT have been called (DryRunCommandRunner intercepts)
		assert!(runner.invocations().is_empty());
	}

	#[test]
	fn with_dry_run_runner_still_forwards_run() {
		let (runner, env, _dir) = recording_env(0);
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

		let (runner, env, _dir) = recording_env(0);
		let env = env.with_editor("vim".to_string());
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		let editor_call = invocations
			.iter()
			.find(|i| i.is_interactive && i.is_shell)
			.expect("Expected a shell interactive invocation");
		let expected = format!("vim {}", crate::shell::shell_quote(&path.to_string_lossy()));
		assert_eq!(editor_call.args[1], expected);
	}

	#[test]
	fn run_editor_on_ignores_empty_editor_string() {
		let workdir = tempfile::tempdir().unwrap();
		let path = workdir.path().join("config.toml");
		std::fs::write(&path, "").unwrap();

		// Empty editor → falls back to find_default_editor → runner returns 0 → "nano"
		let (runner, env, _dir) = recording_env(0);
		let env = env.with_editor(String::new());
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		let editor_call = invocations
			.iter()
			.find(|i| i.is_interactive && i.is_shell)
			.expect("Expected a shell interactive invocation");
		let expected = format!(
			"nano {}",
			crate::shell::shell_quote(&path.to_string_lossy())
		);
		assert_eq!(editor_call.args[1], expected, "Should fall back to nano");
	}

	#[test]
	fn run_editor_on_nonzero_exit_returns_error() {
		let workdir = tempfile::tempdir().unwrap();
		let path = workdir.path().join("config.toml");
		std::fs::write(&path, "").unwrap();

		let (_, env, _dir) = recording_env(1);
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
		let (runner, env, _dir) = recording_env(0);
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		let editor_call = invocations
			.iter()
			.find(|i| i.is_interactive && i.is_shell)
			.expect("Expected a shell interactive invocation");
		let expected = format!(
			"nano {}",
			crate::shell::shell_quote(&path.to_string_lossy())
		);
		assert_eq!(editor_call.args[1], expected);
	}

	#[test]
	fn run_editor_on_no_editor_found_returns_error() {
		let workdir = tempfile::tempdir().unwrap();
		let path = workdir.path().join("config.toml");
		std::fs::write(&path, "").unwrap();

		// Runner exit_code=1 → all which calls fail → no default found
		let (_, env, _dir) = recording_env(1);
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

		let (runner, env, _dir) = recording_env(0);
		let env = env.with_editor("vim".to_string());
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		let editor_call = invocations
			.iter()
			.find(|i| i.is_interactive && i.is_shell)
			.expect("Expected a shell interactive editor invocation");
		assert_eq!(
			editor_call.cwd,
			workdir.path(),
			"Editor should be invoked with the provided cwd, not the file's parent"
		);
	}

	#[test]
	fn run_editor_on_handles_multi_word_editor() {
		let workdir = tempfile::tempdir().unwrap();
		let path = workdir.path().join("config.toml");
		std::fs::write(&path, "").unwrap();

		let (runner, env, _dir) = recording_env(0);
		let env = env.with_editor("code --wait".to_string());
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		let editor_call = invocations
			.iter()
			.find(|i| i.is_interactive && i.is_shell)
			.expect("Expected a shell interactive invocation");
		let expected = format!(
			"code --wait {}",
			crate::shell::shell_quote(&path.to_string_lossy())
		);
		assert_eq!(editor_call.args[1], expected);
	}

	#[test]
	fn run_editor_on_handles_path_with_single_quote() {
		let workdir = tempfile::tempdir().unwrap();
		// Path whose name contains a single quote — tests the '\\'' escaping logic.
		let path = workdir.path().join("it's a file.toml");
		std::fs::write(&path, "").unwrap();

		let (runner, env, _dir) = recording_env(0);
		let env = env.with_editor("vim".to_string());
		env.run_editor_on(&path, workdir.path()).unwrap();

		let invocations = runner.invocations();
		let editor_call = invocations
			.iter()
			.find(|i| i.is_interactive && i.is_shell)
			.expect("Expected a shell interactive invocation");
		let expected = format!("vim {}", crate::shell::shell_quote(&path.to_string_lossy()));
		assert_eq!(editor_call.args[1], expected);
	}

	#[test]
	fn run_shell_interactive_delegates_to_runner() {
		let (runner, env, _dir) = recording_env(0);
		let cwd = tempfile::tempdir().unwrap();
		env.run_shell_interactive("echo hello", cwd.path()).unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_shell);
		assert!(invocations[0].is_interactive);
	}
}
