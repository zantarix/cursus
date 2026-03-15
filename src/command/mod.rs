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

/// Abstracts command execution to allow testing without real processes.
///
/// All commands run with the specified working directory (`cwd`), removing
/// the need for `-C` flags before execution.
///
/// Methods are split into read-only (`run`, `run_shell`) and mutating
/// (`run_mut`, `run_shell_mut`, `run_interactive`) variants. The
/// [`DryRunCommandRunner`] decorator intercepts mutating variants and
/// suppresses them, while read-only variants always execute.
pub trait CommandRunner: Send + Sync + std::fmt::Debug {
	/// Runs a program with the given arguments in the specified directory.
	///
	/// Read-only — always executes, even in dry-run mode.
	fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output>;

	/// Runs a shell command via `/bin/sh -c` in the specified directory.
	///
	/// Read-only — always executes, even in dry-run mode. Used for user-configurable
	/// commands that may use shell features such as pipes, redirects, or variable
	/// expansion (e.g. custom `lock_command`s that only read state).
	fn run_shell(&self, command: &str, cwd: &Path) -> anyhow::Result<Output>;

	/// Runs a program with the given arguments and records it as a mutating operation.
	///
	/// Mutating — skipped by [`DryRunCommandRunner`]. Use this for commands that
	/// modify state (e.g. `git add`, `git commit`, `cargo publish`).
	fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output>;

	/// Runs a shell command via `/bin/sh -c` and records it as a mutating operation.
	///
	/// Mutating — skipped by [`DryRunCommandRunner`]. Use this for shell commands that
	/// write files or modify state (e.g. custom lock file update commands).
	fn run_shell_mut(&self, command: &str, cwd: &Path) -> anyhow::Result<Output>;

	/// Runs a program with inherited stdin/stdout/stderr for interactive use (e.g. editors).
	///
	/// Mutating — skipped by [`DryRunCommandRunner`]. Unlike [`run`], this does not
	/// capture output — the child process shares the terminal directly. Returns the
	/// exit status of the child process.
	fn run_interactive(
		&self,
		program: &str,
		args: &[&str],
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
	inner: R,
}

impl<R: CommandRunner> VerboseCommandRunner<R> {
	/// Creates a new `VerboseCommandRunner` wrapping the given runner.
	pub fn new(inner: R) -> Self {
		Self { inner }
	}
}

impl<R: CommandRunner> CommandRunner for VerboseCommandRunner<R> {
	fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		log::debug!("run: {program} {} (cwd: {})", args.join(" "), cwd.display());
		self.inner.run(program, args, cwd)
	}

	fn run_shell(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
		log::debug!("run_shell: {command:?} (cwd: {})", cwd.display());
		self.inner.run_shell(command, cwd)
	}

	fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		log::debug!(
			"run_mut: {program} {} (cwd: {})",
			args.join(" "),
			cwd.display()
		);
		self.inner.run_mut(program, args, cwd)
	}

	fn run_shell_mut(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
		log::debug!("run_shell_mut: {command:?} (cwd: {})", cwd.display());
		self.inner.run_shell_mut(command, cwd)
	}

	fn run_interactive(
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
		self.inner.run_interactive(program, args, cwd)
	}
}

/// A command runner that executes real system processes.
///
/// This is the production implementation, used by the binary and by integration
/// tests that require actual shell commands (git, cargo, npm) to run.
#[derive(Debug)]
pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
	fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		std::process::Command::new(program)
			.args(args)
			.current_dir(cwd)
			.output()
			.with_context(|| format!("Failed to run '{program}'"))
	}

	fn run_shell(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
		std::process::Command::new("/bin/sh")
			.args(["-c", command])
			.current_dir(cwd)
			.output()
			.with_context(|| format!("Failed to run shell command: '{command}'"))
	}

	fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.run(program, args, cwd)
	}

	fn run_shell_mut(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
		self.run_shell(command, cwd)
	}

	fn run_interactive(
		&self,
		program: &str,
		args: &[&str],
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		std::process::Command::new(program)
			.args(args)
			.current_dir(cwd)
			.status()
			.with_context(|| format!("Failed to run '{program}'"))
	}
}

/// A command runner decorator that suppresses all mutating operations in dry-run mode.
///
/// Wraps any [`CommandRunner`] and intercepts `run_mut`, `run_shell_mut`, and
/// `run_interactive` calls, logging them at `info` level and returning a synthetic
/// success result without running the actual command. Read-only operations (`run`,
/// `run_shell`) are always forwarded to the inner runner.
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

impl CommandRunner for DryRunCommandRunner {
	fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.inner.run(program, args, cwd)
	}

	fn run_shell(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
		self.inner.run_shell(command, cwd)
	}

	fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		log::info!(
			"[dry-run] would run: {program} {} (cwd: {})",
			args.join(" "),
			cwd.display()
		);
		Ok(make_success_output())
	}

	fn run_shell_mut(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
		log::info!("[dry-run] would run: {command:?} (cwd: {})", cwd.display());
		Ok(make_success_output())
	}

	fn run_interactive(
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
}

#[cfg(test)]
mod verbose_tests {
	use std::path::Path;

	use super::*;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::test_logging::{init_test_logger, take_logs};

	#[test]
	fn verbose_runner_delegates_run_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner.run("git", &["status"], cwd);
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, vec!["status"]);
	}

	#[test]
	fn verbose_runner_delegates_run_shell_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner.run_shell("echo hello", cwd);
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_shell);
	}

	#[test]
	fn verbose_runner_delegates_run_interactive_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner.run_interactive("vim", &["file.txt"], cwd);
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_interactive);
		assert_eq!(invocations[0].program, "vim");
	}

	#[test]
	fn verbose_runner_delegates_run_mut_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner.run_mut("git", &["commit", "-m", "msg"], cwd);
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, vec!["commit", "-m", "msg"]);
	}

	#[test]
	fn verbose_runner_delegates_run_shell_mut_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner.run_shell_mut("npm install", cwd);
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_shell);
	}

	#[test]
	fn verbose_runner_logs_run_with_program_and_args() {
		init_test_logger();
		let _ = take_logs(); // clear any accumulated messages
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/some/dir");
		let _ = runner.run("cargo", &["build", "--release"], cwd);
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("cargo"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about cargo");
		assert!(msg.contains("build"), "log should contain args: {msg}");
		assert!(msg.contains("/some/dir"), "log should contain cwd: {msg}");
	}

	#[test]
	fn verbose_runner_logs_run_shell_with_command_and_cwd() {
		init_test_logger();
		let _ = take_logs();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/workspace");
		let _ = runner.run_shell("npm install", cwd);
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("npm install"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about npm install");
		assert!(msg.contains("/workspace"), "log should contain cwd: {msg}");
	}

	#[test]
	fn verbose_runner_logs_run_interactive_with_program_and_cwd() {
		init_test_logger();
		let _ = take_logs();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/edit");
		let _ = runner.run_interactive("nano", &["CHANGELOG.md"], cwd);
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("nano"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about nano");
		assert!(
			msg.contains("CHANGELOG.md"),
			"log should contain args: {msg}"
		);
		assert!(msg.contains("/edit"), "log should contain cwd: {msg}");
	}

	#[test]
	fn verbose_runner_logs_run_mut_with_program_and_args() {
		init_test_logger();
		let _ = take_logs();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/repo");
		let _ = runner.run_mut("git", &["push", "origin", "HEAD"], cwd);
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("push"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about push");
		assert!(msg.contains("/repo"), "log should contain cwd: {msg}");
	}

	#[test]
	fn verbose_runner_logs_run_shell_mut_with_command_and_cwd() {
		init_test_logger();
		let _ = take_logs();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/workspace");
		let _ = runner.run_shell_mut("pnpm install --lockfile-only", cwd);
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("pnpm install"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about pnpm install");
		assert!(msg.contains("/workspace"), "log should contain cwd: {msg}");
	}
}

#[cfg(test)]
mod dry_run_tests {
	use std::path::Path;
	use std::sync::Arc;

	use super::*;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::test_logging::{init_test_logger, take_logs};

	fn make_dry_run_runner() -> DryRunCommandRunner {
		DryRunCommandRunner::new(Arc::new(RecordingCommandRunner::new(0)))
	}

	#[test]
	fn dry_run_runner_forwards_run_to_inner() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let _ = runner.run("git", &["status"], cwd);
		let invocations = inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
	}

	#[test]
	fn dry_run_runner_forwards_run_shell_to_inner() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let _ = runner.run_shell("echo hello", cwd);
		let invocations = inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_shell);
	}

	#[test]
	fn dry_run_runner_suppresses_run_mut() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let result = runner.run_mut("git", &["commit", "-m", "msg"], cwd);
		assert!(result.is_ok());
		assert!(result.unwrap().status.success());
		// Inner runner must NOT have been called
		assert!(inner.invocations().is_empty());
	}

	#[test]
	fn dry_run_runner_suppresses_run_shell_mut() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let result = runner.run_shell_mut("npm install", cwd);
		assert!(result.is_ok());
		assert!(result.unwrap().status.success());
		assert!(inner.invocations().is_empty());
	}

	#[test]
	fn dry_run_runner_suppresses_run_interactive() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let result = runner.run_interactive("vim", &["file.txt"], cwd);
		assert!(result.is_ok());
		assert!(result.unwrap().success());
		assert!(inner.invocations().is_empty());
	}

	#[test]
	fn dry_run_runner_logs_run_mut_at_info() {
		init_test_logger();
		let _ = take_logs();
		let runner = make_dry_run_runner();
		let cwd = Path::new("/repo");
		let _ = runner.run_mut("git", &["push", "origin", "HEAD"], cwd);
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("push"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about push");
		assert!(msg.contains("dry-run"), "log should mention dry-run: {msg}");
	}

	#[test]
	fn dry_run_runner_logs_run_shell_mut_at_info() {
		init_test_logger();
		let _ = take_logs();
		let runner = make_dry_run_runner();
		let cwd = Path::new("/workspace");
		let _ = runner.run_shell_mut("npm install --package-lock-only", cwd);
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("npm install"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about npm install");
		assert!(msg.contains("dry-run"), "log should mention dry-run: {msg}");
	}

	#[test]
	fn dry_run_runner_logs_run_interactive_at_info() {
		init_test_logger();
		let _ = take_logs();
		let runner = make_dry_run_runner();
		let cwd = Path::new("/edit");
		let _ = runner.run_interactive("vim", &["README.md"], cwd);
		let logs = take_logs();
		let (level, msg) = logs
			.iter()
			.find(|(_, m)| m.contains("vim"))
			.expect("expected a log message about vim");
		assert_eq!(*level, log::Level::Info, "should log at info level");
		assert!(msg.contains("dry-run"), "log should mention dry-run: {msg}");
		assert!(
			msg.contains("interactive"),
			"log should mention interactive: {msg}"
		);
	}
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
