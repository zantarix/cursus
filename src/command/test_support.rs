use super::*;

use std::path::PathBuf;
use std::process::Output;
use std::sync::Mutex;

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
		let runner = DispatchingCommandRunner::new(0).on_stdout("npm", 0, b"test-user\n".to_vec());
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
