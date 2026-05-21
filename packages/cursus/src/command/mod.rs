//! Abstraction over command execution for testability.
//!
//! Provides the [`CommandRunner`] trait so that code that shells out to external
//! programs can be tested without hitting real registries or remotes.
//!
//! [`RealCommandRunner`] is the production implementation used by the binary.
//! [`DryRunCommandRunner`] is a decorator that skips mutating operations.
//! [`test_support::RecordingCommandRunner`] is a fake implementation for unit tests.

use std::path::Path;
use std::process::Output;
use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;

/// Returns the platform shell executable: `cmd.exe` on Windows, `/bin/sh` on Unix.
pub(crate) fn shell_program() -> &'static str {
	if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }
}

/// Returns the platform shell command flag: `/C` on Windows, `-c` on Unix.
pub(crate) fn shell_flag() -> &'static str {
	if cfg!(windows) { "/C" } else { "-c" }
}

/// Abstracts command execution to allow testing without real processes.
///
/// All commands run with the specified working directory (`cwd`), removing
/// the need for `-C` flags before execution.
///
/// Methods are split into read-only (`run`) and mutating
/// (`run_mut`, `run_interactive`, `run_shell_interactive`, `run_streaming`) variants. The
/// [`DryRunCommandRunner`] decorator intercepts mutating variants and
/// suppresses them, while read-only variants always execute.
#[async_trait]
pub trait CommandRunner: Send + Sync + std::fmt::Debug {
	/// Runs a program with the given arguments in the specified directory.
	///
	/// Read-only — always executes, even in dry-run mode.
	async fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output>;

	/// Runs a program with the given arguments and records it as a mutating operation.
	///
	/// Mutating — skipped by [`DryRunCommandRunner`]. Use this for commands that
	/// modify state (e.g. `git add`, `git commit`, `cargo publish`).
	async fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output>;

	/// Runs a program with inherited stdin/stdout/stderr for interactive use (e.g. editors).
	///
	/// Mutating — skipped by [`DryRunCommandRunner`]. Unlike [`run`], this does not
	/// capture output — the child process shares the terminal directly. Returns the
	/// exit status of the child process.
	async fn run_interactive(
		&self,
		program: &str,
		args: &[&str],
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus>;

	/// Runs a shell command via the platform shell with inherited stdin/stdout/stderr.
	///
	/// Mutating — skipped by [`DryRunCommandRunner`]. Use this for user-supplied commands
	/// that must be shell-interpreted (e.g. `EDITOR="code --wait"`) and need to interact
	/// with the terminal directly, including reading from stdin.
	async fn run_shell_interactive(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus>;

	/// Runs a shell command via the platform shell, streaming output live to the terminal.
	///
	/// Mutating — skipped by [`DryRunCommandRunner`]. The child process's `stdout` and
	/// `stderr` are inherited from the parent so all output appears immediately on the
	/// user's terminal. `stdin` is set to null so the command cannot block waiting for
	/// user input. Returns the exit status of the child process.
	///
	/// Use this for user-configurable shell commands (e.g. `github.build_command`,
	/// `npm.lock_command`) where live progress feedback matters more than captured output.
	/// [`RealCommandRunner`] emits `log::info!("Running: {command}")` before spawning;
	/// callers must not add their own pre-log.
	async fn run_streaming(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus>;
}

// --- Helpers for constructing synthetic outputs ---

fn make_success_output() -> Output {
	#[cfg(unix)]
	let status = {
		use std::os::unix::process::ExitStatusExt;
		std::process::ExitStatus::from_raw(0)
	};
	#[cfg(windows)]
	let status = {
		use std::os::windows::process::ExitStatusExt;
		std::process::ExitStatus::from_raw(0)
	};
	Output {
		status,
		stdout: Vec::new(),
		stderr: Vec::new(),
	}
}

fn make_success_exit_status() -> std::process::ExitStatus {
	make_success_output().status
}

// ---

/// A command runner decorator that logs each invocation at `debug` level.
///
/// Wraps any [`CommandRunner`] and emits a `log::debug!` message before
/// delegating to the inner runner. The global log level filter suppresses
/// these messages when the level is above `Debug`.
#[derive(Debug)]
pub struct VerboseCommandRunner<R: CommandRunner> {
	pub(crate) inner: R,
}

impl<R: CommandRunner> VerboseCommandRunner<R> {
	/// Creates a new `VerboseCommandRunner` wrapping the given runner.
	pub fn new(inner: R) -> Self {
		Self { inner }
	}
}

#[async_trait]
impl<R: CommandRunner> CommandRunner for VerboseCommandRunner<R> {
	async fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		log::debug!("run: {program} {} (cwd: {})", args.join(" "), cwd.display());
		self.inner.run(program, args, cwd).await
	}

	async fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		log::debug!(
			"run_mut: {program} {} (cwd: {})",
			args.join(" "),
			cwd.display()
		);
		self.inner.run_mut(program, args, cwd).await
	}

	async fn run_interactive(
		&self,
		program: &str,
		args: &[&str],
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		log::debug!(
			"run_interactive: {program} {} (cwd: {})",
			args.join(" "),
			cwd.display()
		);
		self.inner.run_interactive(program, args, cwd).await
	}

	async fn run_shell_interactive(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		log::debug!(
			"run_shell_interactive: {command:?} (cwd: {})",
			cwd.display()
		);
		self.inner.run_shell_interactive(command, cwd).await
	}

	async fn run_streaming(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		log::debug!("run_streaming: {command:?} (cwd: {})", cwd.display());
		self.inner.run_streaming(command, cwd).await
	}
}

/// A command runner that executes real system processes.
///
/// This is the production implementation, used by the binary and by integration
/// tests that require actual shell commands (git, cargo, npm) to run.
#[derive(Debug)]
pub struct RealCommandRunner;

#[async_trait]
impl CommandRunner for RealCommandRunner {
	async fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		tokio::process::Command::new(program)
			.args(args)
			.current_dir(cwd)
			.output()
			.await
			.with_context(|| format!("Failed to run '{program}'"))
	}

	async fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.run(program, args, cwd).await
	}

	async fn run_interactive(
		&self,
		program: &str,
		args: &[&str],
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		let program = program.to_string();
		let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
		let cwd = cwd.to_path_buf();
		tokio::task::spawn_blocking(move || {
			std::process::Command::new(&program)
				.args(&args)
				.current_dir(&cwd)
				.status()
				.with_context(|| format!("Failed to run '{program}'"))
		})
		.await
		.context("spawn_blocking panicked")?
	}

	async fn run_shell_interactive(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		let command = command.to_string();
		let cwd = cwd.to_path_buf();
		tokio::task::spawn_blocking(move || {
			std::process::Command::new(shell_program())
				.args([shell_flag(), &command])
				.current_dir(&cwd)
				.status()
				.with_context(|| format!("Failed to run shell command: '{command}'"))
		})
		.await
		.context("spawn_blocking panicked")?
	}

	async fn run_streaming(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		use std::process::Stdio;
		log::info!("Running: {command}");
		tokio::process::Command::new(shell_program())
			.args([shell_flag(), command])
			.current_dir(cwd)
			.stdin(Stdio::null())
			.stdout(Stdio::inherit())
			.stderr(Stdio::inherit())
			.status()
			.await
			.with_context(|| format!("Failed to run streaming command: '{command}'"))
	}
}

/// A command runner decorator that suppresses all mutating operations in dry-run mode.
///
/// Wraps any [`CommandRunner`] and intercepts `run_mut`, `run_interactive`,
/// `run_shell_interactive`, and `run_streaming` calls, logging them at `info` level
/// and returning a synthetic success result without running the actual command.
/// Read-only operations (`run`) are always forwarded to the inner runner.
///
/// Compose this at the outermost layer when dry-run mode is active.
#[derive(Debug)]
pub struct DryRunCommandRunner {
	inner: Arc<dyn CommandRunner>,
}

impl DryRunCommandRunner {
	/// Creates a new `DryRunCommandRunner` wrapping the given runner.
	pub fn new(inner: Arc<dyn CommandRunner>) -> Self {
		Self { inner }
	}
}

#[async_trait]
impl CommandRunner for DryRunCommandRunner {
	async fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.inner.run(program, args, cwd).await
	}

	async fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		log::info!(
			"[dry-run] would run: {program} {} (cwd: {})",
			args.join(" "),
			cwd.display()
		);
		Ok(make_success_output())
	}

	async fn run_interactive(
		&self,
		program: &str,
		args: &[&str],
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		log::info!(
			"[dry-run] would run (interactive): {program} {} (cwd: {})",
			args.join(" "),
			cwd.display()
		);
		Ok(make_success_exit_status())
	}

	async fn run_shell_interactive(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		log::info!(
			"[dry-run] would run (shell interactive): {command:?} (cwd: {})",
			cwd.display()
		);
		Ok(make_success_exit_status())
	}

	async fn run_streaming(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		log::info!(
			"[dry-run] would run (streaming): {command:?} (cwd: {})",
			cwd.display()
		);
		Ok(make_success_exit_status())
	}
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod tests;
