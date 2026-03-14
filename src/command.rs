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

/// Test support types for command execution.
///
/// Provides fake command runner implementations for use in unit and integration
/// tests. Available when compiled with `#[cfg(test)]` (unit tests within this
/// crate) or with the `test-support` feature (external consumers such as
/// integration test crates).
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
	use std::path::PathBuf;
	use std::process::Output;
	use std::sync::Mutex;

	use super::*;

	/// A recorded command invocation.
	#[derive(Debug, Clone)]
	pub struct Invocation {
		/// The program name (or `/bin/sh` for shell commands).
		pub program: String,
		/// The arguments passed to the program.
		pub args: Vec<String>,
		/// The working directory.
		pub cwd: PathBuf,
		/// Whether this was a shell invocation (`run_shell` / `run_shell_mut`).
		pub is_shell: bool,
		/// Whether this was an interactive invocation (`run_interactive`).
		pub is_interactive: bool,
	}

	/// A command runner that records all invocations and returns a configured output.
	///
	/// All commands return the same exit code, stdout, and stderr configured at
	/// construction. Use the builder methods to set non-default values.
	///
	/// Both `run` and `run_mut` (and their shell variants) are recorded identically —
	/// the runner does not distinguish between read-only and mutating calls.
	#[derive(Debug)]
	pub struct RecordingCommandRunner {
		invocations: Mutex<Vec<Invocation>>,
		exit_code: i32,
		stdout: Vec<u8>,
		stderr: Vec<u8>,
	}

	impl RecordingCommandRunner {
		/// Creates a new recording runner whose commands exit with `exit_code`.
		pub fn new(exit_code: i32) -> Self {
			Self {
				invocations: Mutex::new(Vec::new()),
				exit_code,
				stdout: Vec::new(),
				stderr: Vec::new(),
			}
		}

		/// Configures the stdout bytes returned by all commands.
		pub fn with_stdout(mut self, stdout: Vec<u8>) -> Self {
			self.stdout = stdout;
			self
		}

		/// Configures the stderr bytes returned by all commands.
		pub fn with_stderr(mut self, stderr: Vec<u8>) -> Self {
			self.stderr = stderr;
			self
		}

		/// Returns all invocations recorded so far.
		pub fn invocations(&self) -> Vec<Invocation> {
			self.invocations.lock().expect("mutex poisoned").clone()
		}

		fn make_output(&self) -> Output {
			#[cfg(unix)]
			let status = {
				use std::os::unix::process::ExitStatusExt;
				// On Unix, raw waitpid status N<<8 means "exited normally with code N".
				std::process::ExitStatus::from_raw(self.exit_code << 8)
			};
			#[cfg(windows)]
			let status = {
				use std::os::windows::process::ExitStatusExt;
				std::process::ExitStatus::from_raw(self.exit_code as u32)
			};
			Output {
				status,
				stdout: self.stdout.clone(),
				stderr: self.stderr.clone(),
			}
		}

		fn record(
			&self,
			program: &str,
			args: Vec<String>,
			cwd: &Path,
			is_shell: bool,
			is_interactive: bool,
		) {
			self.invocations
				.lock()
				.expect("mutex poisoned")
				.push(Invocation {
					program: program.to_string(),
					args,
					cwd: cwd.to_path_buf(),
					is_shell,
					is_interactive,
				});
		}
	}

	impl CommandRunner for RecordingCommandRunner {
		fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
			self.record(
				program,
				args.iter().map(|s| s.to_string()).collect(),
				cwd,
				false,
				false,
			);
			Ok(self.make_output())
		}

		fn run_shell(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
			self.record(
				"/bin/sh",
				vec!["-c".to_string(), command.to_string()],
				cwd,
				true,
				false,
			);
			Ok(self.make_output())
		}

		fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
			// Records the invocation (recording runner does not suppress mutations).
			self.run(program, args, cwd)
		}

		fn run_shell_mut(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
			// Records the invocation (recording runner does not suppress mutations).
			self.run_shell(command, cwd)
		}

		fn run_interactive(
			&self,
			program: &str,
			args: &[&str],
			cwd: &Path,
		) -> anyhow::Result<std::process::ExitStatus> {
			self.record(
				program,
				args.iter().map(|s| s.to_string()).collect(),
				cwd,
				false,
				true,
			);
			Ok(self.make_output().status)
		}
	}

	/// A dispatch rule that matches a command invocation and specifies its response.
	///
	/// Rules are matched by `program` name; if `args` is `Some`, all listed args
	/// must appear as a prefix of the actual arguments.
	#[derive(Debug)]
	pub struct DispatchRule {
		/// The program name to match.
		pub program: String,
		/// If `Some`, all listed args must appear as a prefix of the actual args.
		pub args: Option<Vec<String>>,
		/// Exit code to return when this rule matches.
		pub exit_code: i32,
		/// Stdout bytes to return when this rule matches.
		pub stdout: Vec<u8>,
		/// Stderr bytes to return when this rule matches.
		pub stderr: Vec<u8>,
	}

	/// A command runner that dispatches to different responses based on the program name and args.
	///
	/// Rules are matched in order; the first rule whose `program` matches (and whose `args`
	/// prefix matches, if specified) wins. When no rule matches, `default_exit_code` is used
	/// with empty stdout/stderr.
	///
	/// All invocations are recorded in the same way as [`RecordingCommandRunner`].
	///
	/// # Shell commands
	///
	/// `run_shell` / `run_shell_mut` record the invocation with program `/bin/sh` and
	/// args `["-c", <command>]`. Dispatch rules must therefore match against `/bin/sh`
	/// (with an appropriate args prefix) when targeting shell commands. For most test
	/// scenarios the commands of interest are invoked via `run` / `run_mut`; add an
	/// explicit `/bin/sh` rule only when you need to control `run_shell` output.
	#[derive(Debug)]
	pub struct DispatchingCommandRunner {
		rules: Vec<DispatchRule>,
		default_exit_code: i32,
		invocations: Mutex<Vec<Invocation>>,
	}

	impl DispatchingCommandRunner {
		/// Creates a new runner that returns `default_exit_code` when no rule matches.
		pub fn new(default_exit_code: i32) -> Self {
			Self {
				rules: Vec::new(),
				default_exit_code,
				invocations: Mutex::new(Vec::new()),
			}
		}

		/// Adds a fully-specified [`DispatchRule`].
		///
		/// Useful when you need to control both stdout and stderr, or when building
		/// rules programmatically.
		pub fn on_rule(mut self, rule: DispatchRule) -> Self {
			self.rules.push(rule);
			self
		}

		/// Adds a rule matching by program name, returning the given exit code.
		pub fn on(self, program: impl Into<String>, exit_code: i32) -> Self {
			self.on_rule(DispatchRule {
				program: program.into(),
				args: None,
				exit_code,
				stdout: Vec::new(),
				stderr: Vec::new(),
			})
		}

		/// Adds a rule matching by program name and arg prefix, returning the given exit code.
		pub fn on_with_args(
			self,
			program: impl Into<String>,
			args: Vec<String>,
			exit_code: i32,
		) -> Self {
			self.on_rule(DispatchRule {
				program: program.into(),
				args: Some(args),
				exit_code,
				stdout: Vec::new(),
				stderr: Vec::new(),
			})
		}

		/// Adds a rule matching by program name, returning the given exit code and stdout.
		pub fn on_stdout(
			self,
			program: impl Into<String>,
			exit_code: i32,
			stdout: Vec<u8>,
		) -> Self {
			self.on_rule(DispatchRule {
				program: program.into(),
				args: None,
				exit_code,
				stdout,
				stderr: Vec::new(),
			})
		}

		/// Adds a rule matching by program name and arg prefix, returning the given exit code and stdout.
		pub fn on_with_args_stdout(
			self,
			program: impl Into<String>,
			args: Vec<String>,
			exit_code: i32,
			stdout: Vec<u8>,
		) -> Self {
			self.on_rule(DispatchRule {
				program: program.into(),
				args: Some(args),
				exit_code,
				stdout,
				stderr: Vec::new(),
			})
		}

		/// Adds a rule matching by program name, returning the given exit code and stderr.
		pub fn on_stderr(
			self,
			program: impl Into<String>,
			exit_code: i32,
			stderr: Vec<u8>,
		) -> Self {
			self.on_rule(DispatchRule {
				program: program.into(),
				args: None,
				exit_code,
				stdout: Vec::new(),
				stderr,
			})
		}

		/// Adds a rule matching by program name and arg prefix, returning the given exit code and stderr.
		pub fn on_with_args_stderr(
			self,
			program: impl Into<String>,
			args: Vec<String>,
			exit_code: i32,
			stderr: Vec<u8>,
		) -> Self {
			self.on_rule(DispatchRule {
				program: program.into(),
				args: Some(args),
				exit_code,
				stdout: Vec::new(),
				stderr,
			})
		}

		/// Returns all invocations recorded so far.
		pub fn invocations(&self) -> Vec<Invocation> {
			self.invocations.lock().expect("mutex poisoned").clone()
		}

		fn find_match(&self, program: &str, args: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
			for rule in &self.rules {
				if rule.program != program {
					continue;
				}
				if let Some(prefix) = &rule.args {
					let matches = prefix.len() <= args.len()
						&& prefix
							.iter()
							.zip(args.iter())
							.all(|(a, b)| a.as_str() == *b);
					if !matches {
						continue;
					}
				}
				return (rule.exit_code, rule.stdout.clone(), rule.stderr.clone());
			}
			(self.default_exit_code, Vec::new(), Vec::new())
		}

		fn make_output_for(&self, program: &str, args: &[&str]) -> Output {
			let (exit_code, stdout, stderr) = self.find_match(program, args);
			#[cfg(unix)]
			let status = {
				use std::os::unix::process::ExitStatusExt;
				std::process::ExitStatus::from_raw(exit_code << 8)
			};
			#[cfg(windows)]
			let status = {
				use std::os::windows::process::ExitStatusExt;
				std::process::ExitStatus::from_raw(exit_code as u32)
			};
			Output {
				status,
				stdout,
				stderr,
			}
		}

		fn record(
			&self,
			program: &str,
			args: Vec<String>,
			cwd: &Path,
			is_shell: bool,
			is_interactive: bool,
		) {
			self.invocations
				.lock()
				.expect("mutex poisoned")
				.push(Invocation {
					program: program.to_string(),
					args,
					cwd: cwd.to_path_buf(),
					is_shell,
					is_interactive,
				});
		}
	}

	impl CommandRunner for DispatchingCommandRunner {
		fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
			self.record(
				program,
				args.iter().map(|s| s.to_string()).collect(),
				cwd,
				false,
				false,
			);
			Ok(self.make_output_for(program, args))
		}

		fn run_shell(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
			self.record(
				"/bin/sh",
				vec!["-c".to_string(), command.to_string()],
				cwd,
				true,
				false,
			);
			Ok(self.make_output_for("/bin/sh", &["-c", command]))
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
			self.record(
				program,
				args.iter().map(|s| s.to_string()).collect(),
				cwd,
				false,
				true,
			);
			Ok(self.make_output_for(program, args).status)
		}
	}

	#[cfg(test)]
	mod dispatching_tests {
		use std::path::Path;

		use super::*;

		#[test]
		fn dispatching_runner_returns_default_when_no_rule_matches() {
			let runner = DispatchingCommandRunner::new(1);
			let cwd = Path::new("/tmp");
			let output = runner.run("unknown", &[], cwd).unwrap();
			assert!(!output.status.success());
		}

		#[test]
		fn dispatching_runner_matches_program_name() {
			let runner = DispatchingCommandRunner::new(1).on("git", 0);
			let cwd = Path::new("/tmp");
			let output = runner.run("git", &["status"], cwd).unwrap();
			assert!(output.status.success());
		}

		#[test]
		fn dispatching_runner_first_matching_rule_wins() {
			let runner = DispatchingCommandRunner::new(1).on("git", 0).on("git", 2); // should never be reached
			let cwd = Path::new("/tmp");
			let output = runner.run("git", &[], cwd).unwrap();
			assert!(output.status.success());
		}

		#[test]
		fn dispatching_runner_matches_args_prefix() {
			let runner =
				DispatchingCommandRunner::new(0).on_with_args("git", vec!["push".to_string()], 42);
			let cwd = Path::new("/tmp");
			let output = runner.run("git", &["push", "origin", "HEAD"], cwd).unwrap();
			#[cfg(unix)]
			{
				use std::os::unix::process::ExitStatusExt;
				assert_eq!(output.status.into_raw(), 42 << 8);
			}
		}

		#[test]
		fn dispatching_runner_falls_through_when_args_prefix_does_not_match() {
			let runner =
				DispatchingCommandRunner::new(0).on_with_args("git", vec!["push".to_string()], 42);
			let cwd = Path::new("/tmp");
			// "fetch" does not match the "push" prefix rule; default (0) is used
			let output = runner.run("git", &["fetch"], cwd).unwrap();
			assert!(output.status.success());
		}

		#[test]
		fn dispatching_runner_returns_configured_stdout() {
			let runner =
				DispatchingCommandRunner::new(0).on_stdout("npm", 0, b"test-user\n".to_vec());
			let cwd = Path::new("/tmp");
			let output = runner.run("npm", &["whoami"], cwd).unwrap();
			assert_eq!(output.stdout, b"test-user\n");
		}

		#[test]
		fn dispatching_runner_returns_configured_stderr() {
			let runner = DispatchingCommandRunner::new(0).on_stderr(
				"cargo",
				1,
				b"error: not logged in\n".to_vec(),
			);
			let cwd = Path::new("/tmp");
			let output = runner.run("cargo", &[], cwd).unwrap();
			assert_eq!(output.stderr, b"error: not logged in\n");
		}

		#[test]
		fn dispatching_runner_on_rule_accepts_full_dispatch_rule() {
			let rule = DispatchRule {
				program: "npm".to_string(),
				args: Some(vec!["whoami".to_string()]),
				exit_code: 0,
				stdout: b"alice\n".to_vec(),
				stderr: Vec::new(),
			};
			let runner = DispatchingCommandRunner::new(1).on_rule(rule);
			let cwd = Path::new("/tmp");
			let output = runner.run("npm", &["whoami"], cwd).unwrap();
			assert_eq!(output.stdout, b"alice\n");
			assert!(output.status.success());
		}

		#[test]
		fn dispatching_runner_records_invocations() {
			let runner = DispatchingCommandRunner::new(0).on("git", 0);
			let cwd = Path::new("/tmp");
			let _ = runner.run("git", &["status"], cwd).unwrap();
			let _ = runner
				.run_mut("git", &["commit", "-m", "msg"], cwd)
				.unwrap();
			let invocations = runner.invocations();
			assert_eq!(invocations.len(), 2);
			assert_eq!(invocations[0].args, vec!["status"]);
			assert_eq!(invocations[1].args, vec!["commit", "-m", "msg"]);
		}

		#[test]
		fn dispatching_runner_records_shell_invocations() {
			let runner = DispatchingCommandRunner::new(0);
			let cwd = Path::new("/tmp");
			let _ = runner.run_shell("npm install", cwd).unwrap();
			let invocations = runner.invocations();
			assert_eq!(invocations.len(), 1);
			assert!(invocations[0].is_shell);
			assert_eq!(invocations[0].program, "/bin/sh");
		}

		#[test]
		fn dispatching_runner_records_interactive_invocations() {
			let runner = DispatchingCommandRunner::new(0);
			let cwd = Path::new("/tmp");
			let _ = runner.run_interactive("vim", &["file.txt"], cwd).unwrap();
			let invocations = runner.invocations();
			assert_eq!(invocations.len(), 1);
			assert!(invocations[0].is_interactive);
			assert_eq!(invocations[0].program, "vim");
		}
	}
}
