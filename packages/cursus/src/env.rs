//! Application environment threaded through the library boundary.

use std::path::Path;
use std::process::{ExitStatus, Output};
use std::sync::Arc;

use crate::command::{CommandRunner, DryRunCommandRunner};
use crate::filesystem::Filesystem;
use crate::forge::CodeForgeClient;
use crate::git::Git;

/// Environment variables and runtime dependencies used by Cursus.
///
/// Populated from the process environment at the binary boundary and threaded
/// into the library so that internal functions never read `std::env` directly.
/// Carries the [`CommandRunner`], [`Filesystem`], [`Git`], and optional
/// [`CodeForgeClient`] so that all I/O can be intercepted or replaced.
#[derive(Debug, Clone)]
pub struct Env {
	/// The configured editor for opening changeset files.
	///
	/// Resolved from `VISUAL` then `EDITOR` in the binary entry point.
	///
	/// Per POSIX [`environ(7)`](https://man7.org/linux/man-pages/man7/environ.7.html),
	/// `$VISUAL`/`$EDITOR` are defined as "any string acceptable as a `command_string`
	/// operand to `sh -c`", meaning they may contain flags and shell syntax
	/// (e.g. `EDITOR="emacs -nw"`, `EDITOR="vim --nofork"`).
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
	/// The code forge client for API operations, or a reason why it is unavailable.
	code_forge_client: Result<Arc<dyn CodeForgeClient>, String>,
	/// Whether an OIDC-capable CI environment is detected.
	///
	/// `true` when `ACTIONS_ID_TOKEN_REQUEST_URL` (GitHub Actions) or
	/// `CI_JOB_JWT_V2` (GitLab CI) is set.
	oidc_environment: bool,
	/// Whether `NODE_AUTH_TOKEN` is set in the environment.
	node_auth_token_present: bool,
	/// Whether `CARGO_REGISTRY_TOKEN` is set in the environment.
	cargo_registry_token_present: bool,
	/// `true` when the active GitLab forge client was built from `CI_JOB_TOKEN`
	/// with no `GITLAB_TOKEN` PAT available.
	///
	/// Used by the `prepare` preflight to surface a clear error before any
	/// merge-request API call is attempted — `CI_JOB_TOKEN` has read-only
	/// access to the Merge Requests API on GitLab (see ADR-056). Always
	/// `false` when the active forge is GitHub or when no forge is configured.
	gitlab_uses_job_token_only: bool,
	/// User-facing label for the active forge (e.g. `"GitHub"`, `"GitLab"`).
	///
	/// Captured automatically from [`CodeForgeClient::forge_name`] when a
	/// client is wired up via [`with_code_forge_client`](Self::with_code_forge_client)
	/// or a successful [`with_code_forge_client_result`](Self::with_code_forge_client_result).
	/// Defaults to `"the configured forge"` when no client has been set
	/// (e.g. token missing in dry-run), so dry-run preview messages and
	/// pre-flight errors still have a sensible noun phrase to interpolate.
	code_forge_name: &'static str,
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
	/// [`with_code_forge_client`][Self::with_code_forge_client]) to add optional configuration.
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
			code_forge_client: Err("No code forge client configured".into()),
			oidc_environment: false,
			node_auth_token_present: false,
			cargo_registry_token_present: false,
			gitlab_uses_job_token_only: false,
			code_forge_name: "the configured forge",
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

	/// Sets whether the active GitLab client was constructed from `CI_JOB_TOKEN`
	/// only (no `GITLAB_TOKEN` PAT).
	///
	/// Consumed by the prepare preflight to fail fast before any merge-request
	/// API call when the token cannot create or update merge requests.
	pub fn with_gitlab_uses_job_token_only(mut self, value: bool) -> Self {
		self.gitlab_uses_job_token_only = value;
		self
	}

	/// Sets the editor to open changeset files with.
	pub fn with_editor(mut self, editor: String) -> Self {
		self.editor = Some(editor);
		self
	}

	/// Sets the code forge client for API operations.
	///
	/// Also captures the forge's user-facing name (via
	/// [`CodeForgeClient::forge_name`]) so [`code_forge_name`](Self::code_forge_name)
	/// stays in sync with the configured client without a second setter call.
	pub fn with_code_forge_client(mut self, client: Arc<dyn CodeForgeClient>) -> Self {
		self.code_forge_name = client.forge_name();
		self.code_forge_client = Ok(client);
		self
	}

	/// Sets the editor from an `Option`, overwriting any previously set value.
	///
	/// Passing `None` clears a previously set editor.
	pub fn with_editor_opt(mut self, editor: Option<String>) -> Self {
		self.editor = editor;
		self
	}

	/// Sets the code forge client from a `Result`, overwriting any previously set value.
	///
	/// Passing `Err(reason)` records why the client is unavailable. On `Ok`,
	/// also captures [`CodeForgeClient::forge_name`] so
	/// [`code_forge_name`](Self::code_forge_name) reflects the active forge.
	pub fn with_code_forge_client_result(
		mut self,
		client: Result<Arc<dyn CodeForgeClient>, String>,
	) -> Self {
		if let Ok(c) = &client {
			self.code_forge_name = c.forge_name();
		}
		self.code_forge_client = client;
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
			code_forge_client: self.code_forge_client,
			oidc_environment: self.oidc_environment,
			node_auth_token_present: self.node_auth_token_present,
			cargo_registry_token_present: self.cargo_registry_token_present,
			gitlab_uses_job_token_only: self.gitlab_uses_job_token_only,
			code_forge_name: self.code_forge_name,
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

	/// Returns the code forge client, or a reason why it is unavailable.
	pub(crate) fn code_forge_client(&self) -> Result<&dyn CodeForgeClient, &str> {
		self.code_forge_client
			.as_ref()
			.map(|c| &**c as &dyn CodeForgeClient)
			.map_err(|e| e.as_str())
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

	/// Returns `true` when the active GitLab client was built from `CI_JOB_TOKEN`
	/// without a `GITLAB_TOKEN` PAT fallback.
	pub(crate) fn gitlab_uses_job_token_only(&self) -> bool {
		self.gitlab_uses_job_token_only
	}

	/// Returns the user-facing label for the active forge.
	///
	/// Prefer this over `env.code_forge_client().map(|c| c.forge_name())`
	/// when the active code path needs a forge name without a specific
	/// client invocation in scope (dry-run previews, pre-flight error
	/// messages, etc.). Falls back to `"the configured forge"` when no
	/// forge has been wired up.
	pub(crate) fn code_forge_name(&self) -> &'static str {
		self.code_forge_name
	}

	/// Returns the BCP 47 locale tag for user-visible messages.
	pub(crate) fn locale(&self) -> &str {
		&self.locale
	}

	/// Finds a default editor by probing well-known editors on the system PATH.
	///
	/// On Windows, checks for `notepad` via `where.exe`. On Unix, checks for
	/// `nano`, `vim`, `vi`, and `emacs` via `which`.
	async fn find_default_editor(&self, cwd: &Path) -> Option<String> {
		let (probe_cmd, candidates): (&str, &[&str]) = if cfg!(windows) {
			("where.exe", &["notepad"])
		} else {
			("which", &["nano", "vim", "vi", "emacs"])
		};
		for cmd in candidates {
			if self
				.run(probe_cmd, &[cmd], cwd)
				.await
				.is_ok_and(|o| o.status.success())
			{
				return Some((*cmd).to_string());
			}
		}
		None
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
	pub async fn run_editor_on(&self, path: &Path, cwd: &Path) -> anyhow::Result<()> {
		use anyhow::Context as _;
		let editor = match self.editor().filter(|v| !v.is_empty()).map(String::from) {
			Some(e) => e,
			None => self
				.find_default_editor(cwd)
				.await
				.context("No editor found. Set the VISUAL or EDITOR environment variable.")?,
		};
		let path_str = path.to_string_lossy();
		let shell_cmd = format!("{editor} {}", crate::shell::shell_quote(&path_str));
		let status = self
			.run_shell_interactive(&shell_cmd, cwd)
			.await
			.with_context(|| format!("Failed to open editor: {editor}"))?;
		if !status.success() {
			anyhow::bail!("Editor exited with status: {status}");
		}
		Ok(())
	}

	/// Runs a program with the given arguments in the specified directory.
	///
	/// Delegates to the underlying [`CommandRunner`]. Read-only.
	pub async fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.runner.run(program, args, cwd).await
	}

	/// Runs a mutating program with the given arguments in the specified directory.
	///
	/// Delegates to the underlying [`CommandRunner`]. Skipped by [`DryRunCommandRunner`].
	pub async fn run_mut(
		&self,
		program: &str,
		args: &[&str],
		cwd: &Path,
	) -> anyhow::Result<Output> {
		self.runner.run_mut(program, args, cwd).await
	}

	/// Runs a program with inherited stdin/stdout/stderr for interactive use.
	///
	/// Delegates to the underlying [`CommandRunner`]. Skipped by [`DryRunCommandRunner`].
	pub async fn run_interactive(
		&self,
		program: &str,
		args: &[&str],
		cwd: &Path,
	) -> anyhow::Result<ExitStatus> {
		self.runner.run_interactive(program, args, cwd).await
	}

	/// Runs a shell command via the platform shell with inherited stdin/stdout/stderr.
	///
	/// Delegates to the underlying [`CommandRunner`]. Skipped by [`DryRunCommandRunner`].
	pub async fn run_shell_interactive(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<ExitStatus> {
		self.runner.run_shell_interactive(command, cwd).await
	}

	/// Runs a shell command via the platform shell, streaming output live to the terminal.
	///
	/// Delegates to the underlying [`CommandRunner`]. Skipped by [`DryRunCommandRunner`].
	pub async fn run_streaming(&self, command: &str, cwd: &Path) -> anyhow::Result<ExitStatus> {
		self.runner.run_streaming(command, cwd).await
	}
}
