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
}

/// Test support types for command execution.
///
/// Provides a fake command runner implementation for use in unit tests.
/// This module is always compiled so that integration test crates can import it.
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
			use std::os::unix::process::ExitStatusExt;
			// On Unix, raw waitpid status N<<8 means "exited normally with code N".
			let raw = self.exit_code << 8;
			let status = std::process::ExitStatus::from_raw(raw);
			Output {
				status,
				stdout: self.stdout.clone(),
				stderr: self.stderr.clone(),
			}
		}

		fn record(&self, program: &str, args: Vec<String>, cwd: &Path, is_shell: bool) {
			self.invocations
				.lock()
				.expect("mutex poisoned")
				.push(Invocation {
					program: program.to_string(),
					args,
					cwd: cwd.to_path_buf(),
					is_shell,
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
			);
			Ok(self.make_output())
		}

		fn run_shell(&self, command: &str, cwd: &Path) -> anyhow::Result<Output> {
			self.record(
				"/bin/sh",
				vec!["-c".to_string(), command.to_string()],
				cwd,
				true,
			);
			Ok(self.make_output())
		}
	}
}
