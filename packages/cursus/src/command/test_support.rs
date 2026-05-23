use super::*;

use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Output;
use std::sync::Mutex;

/// A recorded command invocation.
#[derive(Debug, Clone)]
pub struct Invocation {
	/// The program name (or the platform shell for shell commands).
	pub program: String,
	/// The arguments passed to the program.
	pub args: Vec<String>,
	/// The working directory.
	pub cwd: PathBuf,
	/// Whether this was a shell invocation (`run_shell_interactive` / `run_streaming`).
	pub is_shell: bool,
	/// Whether this was an interactive invocation (`run_interactive` / `run_shell_interactive`).
	pub is_interactive: bool,
	/// Whether this was a streaming invocation (`run_streaming`).
	pub is_streaming: bool,
}

/// A command runner that records all invocations and returns a configured output.
///
/// All commands return the same exit code, stdout, and stderr configured at
/// construction. Use the builder methods to set non-default values.
///
/// Both `run` and `run_mut` (and interactive / streaming variants) are recorded identically —
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
		is_streaming: bool,
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
				is_streaming,
			});
	}
}

#[async_trait]
impl CommandRunner for RecordingCommandRunner {
	async fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.record(
			program,
			args.iter().map(|s| s.to_string()).collect(),
			cwd,
			false,
			false,
			false,
		);
		Ok(self.make_output())
	}

	async fn run_mut(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		// Records the invocation (recording runner does not suppress mutations).
		self.run(program, args, cwd).await
	}

	async fn run_interactive(
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
			false,
		);
		Ok(self.make_output().status)
	}

	async fn run_shell_interactive(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		self.record(
			shell_program(),
			vec![shell_flag().to_string(), command.to_string()],
			cwd,
			true,
			true,
			false,
		);
		Ok(self.make_output().status)
	}

	async fn run_streaming(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		self.record(
			shell_program(),
			vec![shell_flag().to_string(), command.to_string()],
			cwd,
			true,
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
/// `run_shell_interactive` and `run_streaming` record the invocation with the platform shell
/// program (see [`shell_program`]) and args `[<shell_flag>, <command>]`. Dispatch rules must
/// therefore match against the platform shell when targeting these commands. For most
/// test scenarios the commands of interest are invoked via `run` / `run_mut`; add an
/// explicit shell rule only when you need to control shell command output.
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
	pub fn on_stdout(self, program: impl Into<String>, exit_code: i32, stdout: Vec<u8>) -> Self {
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
	pub fn on_stderr(self, program: impl Into<String>, exit_code: i32, stderr: Vec<u8>) -> Self {
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
		self.rules
			.iter()
			.find(|rule| {
				rule.program == program
					&& rule.args.as_ref().is_none_or(|prefix| {
						prefix.len() <= args.len()
							&& prefix
								.iter()
								.zip(args.iter())
								.all(|(a, b)| a.as_str() == *b)
					})
			})
			.map_or_else(
				|| (self.default_exit_code, Vec::new(), Vec::new()),
				|rule| (rule.exit_code, rule.stdout.clone(), rule.stderr.clone()),
			)
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
		is_streaming: bool,
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
				is_streaming,
			});
	}
}

#[async_trait]
impl CommandRunner for DispatchingCommandRunner {
	async fn run(&self, program: &str, args: &[&str], cwd: &Path) -> anyhow::Result<Output> {
		self.record(
			program,
			args.iter().map(|s| s.to_string()).collect(),
			cwd,
			false,
			false,
			false,
		);
		Ok(self.make_output_for(program, args))
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
		self.record(
			program,
			args.iter().map(|s| s.to_string()).collect(),
			cwd,
			false,
			true,
			false,
		);
		Ok(self.make_output_for(program, args).status)
	}

	async fn run_shell_interactive(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		self.record(
			shell_program(),
			vec![shell_flag().to_string(), command.to_string()],
			cwd,
			true,
			true,
			false,
		);
		Ok(self
			.make_output_for(shell_program(), &[shell_flag(), command])
			.status)
	}

	async fn run_streaming(
		&self,
		command: &str,
		cwd: &Path,
	) -> anyhow::Result<std::process::ExitStatus> {
		self.record(
			shell_program(),
			vec![shell_flag().to_string(), command.to_string()],
			cwd,
			true,
			false,
			true,
		);
		Ok(self
			.make_output_for(shell_program(), &[shell_flag(), command])
			.status)
	}
}
