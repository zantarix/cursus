//! Abstraction over command execution for testability.
//!
//! Provides the [`CommandRunner`] trait so that code that shells out to external
//! programs can be tested without hitting real registries or remotes.
//!
//! [`RealCommandRunner`] is the production implementation used by the binary.
//! [`test_support::RecordingCommandRunner`] is a fake implementation for unit tests.

use std::path::Path;
use std::process::Output;

use anyhow::Context;

/// Abstracts command execution to allow testing without real processes.
///
/// All commands run with the specified working directory (`cwd`), removing
/// the need for `-C` flags before execution.
pub trait CommandRunner: Send + Sync + std::fmt::Debug {
	/// Runs a program with the given arguments in the specified directory.
	fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output>;

	/// Runs a shell command via `/bin/sh -c` in the specified directory.
	///
	/// Used for user-configurable commands that may use shell features such as
	/// pipes, redirects, or variable expansion (e.g. custom `lock_command`s).
	fn run_shell(&self, command: &str, cwd: &Path) -> anyhow::Result<Output>;

	/// Runs a program with inherited stdin/stdout/stderr for interactive use (e.g. editors).
	///
	/// Unlike [`run`], this does not capture output — the child process shares the
	/// terminal directly. Returns the exit status of the child process.
	fn run_interactive(
		&self,
		program: &str,
		args: &[&str],
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus>;
}

/// A command runner decorator that logs each invocation at `debug` level.
///
/// Wraps any [`CommandRunner`] and emits a `log::debug!` message before
/// delegating to the inner runner. Fern filters the messages according to the
/// configured log level, so this wrapper is always active and has no effect
/// when the log level is above `Debug`.
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
}

/// Test support types for command execution.
///
/// Provides a fake command runner implementation for use in unit and integration
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
		/// Whether this was a shell invocation (`run_shell`).
		pub is_shell: bool,
		/// Whether this was an interactive invocation (`run_interactive`).
		pub is_interactive: bool,
	}

	/// A command runner that records all invocations and returns a configured output.
	///
	/// All commands return the same exit code, stdout, and stderr configured at
	/// construction. Use the builder methods to set non-default values.
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
}
