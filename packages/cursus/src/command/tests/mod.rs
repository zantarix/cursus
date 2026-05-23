mod test_support;

mod verbose_tests {
	use std::path::Path;

	use crate::command::test_support::RecordingCommandRunner;
	use crate::command::*;
	use crate::test_logging::{init_test_logger, take_logs};

	#[tokio::test]
	async fn verbose_runner_delegates_run_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner.run("git", &["status"], cwd).await;
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, vec!["status"]);
	}

	#[tokio::test]
	async fn verbose_runner_delegates_run_interactive_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner.run_interactive("vim", &["file.txt"], cwd).await;
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_interactive);
		assert_eq!(invocations[0].program, "vim");
	}

	#[tokio::test]
	async fn verbose_runner_delegates_run_mut_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner.run_mut("git", &["commit", "-m", "msg"], cwd).await;
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
		assert_eq!(invocations[0].args, vec!["commit", "-m", "msg"]);
	}

	#[tokio::test]
	async fn verbose_runner_logs_run_with_program_and_args() {
		init_test_logger();
		let _ = take_logs(); // clear any accumulated messages
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/some/dir");
		let _ = runner.run("cargo", &["build", "--release"], cwd).await;
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("cargo"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about cargo");
		assert!(msg.contains("build"), "log should contain args: {msg}");
		assert!(msg.contains("/some/dir"), "log should contain cwd: {msg}");
	}

	#[tokio::test]
	async fn verbose_runner_logs_run_interactive_with_program_and_cwd() {
		init_test_logger();
		let _ = take_logs();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/edit");
		let _ = runner.run_interactive("nano", &["CHANGELOG.md"], cwd).await;
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

	#[tokio::test]
	async fn verbose_runner_logs_run_mut_with_program_and_args() {
		init_test_logger();
		let _ = take_logs();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/repo");
		let _ = runner
			.run_mut("git", &["push", "origin", "HEAD"], cwd)
			.await;
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("push"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about push");
		assert!(msg.contains("/repo"), "log should contain cwd: {msg}");
	}

	#[tokio::test]
	async fn verbose_runner_delegates_run_shell_interactive_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner
			.run_shell_interactive("code --wait file.txt", cwd)
			.await;
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_shell);
		assert!(invocations[0].is_interactive);
	}

	#[tokio::test]
	async fn verbose_runner_logs_run_shell_interactive_with_command_and_cwd() {
		init_test_logger();
		let _ = take_logs();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/edit");
		let _ = runner
			.run_shell_interactive("code --wait CHANGELOG.md", cwd)
			.await;
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("code --wait"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about code --wait");
		assert!(msg.contains("/edit"), "log should contain cwd: {msg}");
	}

	#[tokio::test]
	async fn verbose_runner_delegates_run_streaming_to_inner() {
		init_test_logger();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/tmp");
		let _ = runner
			.run_streaming("pnpm install --lockfile-only", cwd)
			.await;
		let invocations = runner.inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_shell);
		assert!(invocations[0].is_streaming);
	}

	#[tokio::test]
	async fn verbose_runner_logs_run_streaming_with_command_and_cwd() {
		init_test_logger();
		let _ = take_logs();
		let inner = RecordingCommandRunner::new(0);
		let runner = VerboseCommandRunner::new(inner);
		let cwd = Path::new("/workspace");
		let _ = runner
			.run_streaming("pnpm install --lockfile-only", cwd)
			.await;
		let logs = take_logs();
		let msg = logs
			.iter()
			.find(|(_, m)| m.contains("pnpm install"))
			.map(|(_, m)| m.as_str())
			.expect("expected a log message about pnpm install");
		assert!(msg.contains("/workspace"), "log should contain cwd: {msg}");
	}
}

mod dry_run_tests {
	use std::path::Path;
	use std::sync::Arc;

	use crate::command::test_support::RecordingCommandRunner;
	use crate::command::*;
	use crate::test_logging::{init_test_logger, take_logs};

	fn make_dry_run_runner() -> DryRunCommandRunner {
		DryRunCommandRunner::new(Arc::new(RecordingCommandRunner::new(0)))
	}

	#[tokio::test]
	async fn dry_run_runner_forwards_run_to_inner() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let _ = runner.run("git", &["status"], cwd).await;
		let invocations = inner.invocations();
		assert_eq!(invocations.len(), 1);
		assert_eq!(invocations[0].program, "git");
	}

	#[tokio::test]
	async fn dry_run_runner_suppresses_run_mut() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let result = runner.run_mut("git", &["commit", "-m", "msg"], cwd).await;
		assert!(result.is_ok());
		assert!(result.unwrap().status.success());
		// Inner runner must NOT have been called
		assert!(inner.invocations().is_empty());
	}

	#[tokio::test]
	async fn dry_run_runner_suppresses_run_interactive() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let result = runner.run_interactive("vim", &["file.txt"], cwd).await;
		assert!(result.is_ok());
		assert!(result.unwrap().success());
		assert!(inner.invocations().is_empty());
	}

	#[tokio::test]
	async fn dry_run_runner_logs_run_mut_at_info() {
		init_test_logger();
		let _ = take_logs();
		let runner = make_dry_run_runner();
		let cwd = Path::new("/repo");
		let _ = runner
			.run_mut("git", &["push", "origin", "HEAD"], cwd)
			.await;
		let logs = take_logs();
		let (level, msg) = logs
			.iter()
			.find(|(_, m)| m.contains("push"))
			.expect("expected a log message about push");
		assert_eq!(*level, log::Level::Info, "should log at info level");
		assert!(msg.contains("dry-run"), "log should mention dry-run: {msg}");
	}

	#[tokio::test]
	async fn dry_run_runner_suppresses_run_shell_interactive() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let result = runner
			.run_shell_interactive("code --wait file.txt", cwd)
			.await;
		assert!(result.is_ok());
		assert!(result.unwrap().success());
		assert!(inner.invocations().is_empty());
	}

	#[tokio::test]
	async fn dry_run_runner_logs_run_shell_interactive_at_info() {
		init_test_logger();
		let _ = take_logs();
		let runner = make_dry_run_runner();
		let cwd = Path::new("/edit");
		let _ = runner
			.run_shell_interactive("code --wait README.md", cwd)
			.await;
		let logs = take_logs();
		let (level, msg) = logs
			.iter()
			.find(|(_, m)| m.contains("code --wait"))
			.expect("expected a log message about code --wait");
		assert_eq!(*level, log::Level::Info, "should log at info level");
		assert!(msg.contains("dry-run"), "log should mention dry-run: {msg}");
	}

	#[tokio::test]
	async fn dry_run_runner_logs_run_interactive_at_info() {
		init_test_logger();
		let _ = take_logs();
		let runner = make_dry_run_runner();
		let cwd = Path::new("/edit");
		let _ = runner.run_interactive("vim", &["README.md"], cwd).await;
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

	#[tokio::test]
	async fn dry_run_runner_suppresses_run_streaming() {
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let runner = DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let cwd = Path::new("/tmp");
		let result = runner
			.run_streaming("npm install --lockfile-only", cwd)
			.await;
		assert!(result.is_ok());
		assert!(result.unwrap().success());
		assert!(inner.invocations().is_empty());
	}

	#[tokio::test]
	async fn dry_run_runner_logs_run_streaming_at_info() {
		init_test_logger();
		let _ = take_logs();
		let runner = make_dry_run_runner();
		let cwd = Path::new("/workspace");
		let _ = runner
			.run_streaming("npm install --package-lock-only", cwd)
			.await;
		let logs = take_logs();
		let (level, msg) = logs
			.iter()
			.find(|(_, m)| m.contains("npm install"))
			.expect("expected a log message about npm install");
		assert_eq!(*level, log::Level::Info, "should log at info level");
		assert!(msg.contains("dry-run"), "log should mention dry-run: {msg}");
		assert!(
			msg.contains("streaming"),
			"log should mention streaming: {msg}"
		);
	}
}

mod real_command_tests {
	use crate::command::*;

	#[tokio::test]
	async fn real_runner_run_interactive_returns_success() {
		// Exercises RealCommandRunner::run_interactive so the .status() call is
		// covered — mutations that replace it with something else would break this.
		// git is a prerequisite on all platforms and is a real executable.
		let runner = RealCommandRunner;
		let cwd = std::env::temp_dir();
		let result = runner.run_interactive("git", &["--version"], &cwd).await;
		assert!(result.is_ok(), "run_interactive should succeed: {result:?}");
		assert!(
			result.unwrap().success(),
			"'git --version' must exit with status 0"
		);
	}

	#[tokio::test]
	async fn real_runner_run_shell_interactive_returns_success() {
		// Exercises RealCommandRunner::run_shell_interactive so the .status() call is
		// covered — mutations that replace it with something else would break this.
		// "echo ok" works on both /bin/sh and cmd.exe.
		let runner = RealCommandRunner;
		let cwd = std::env::temp_dir();
		let result = runner.run_shell_interactive("echo ok", &cwd).await;
		assert!(
			result.is_ok(),
			"run_shell_interactive should succeed: {result:?}"
		);
		assert!(
			result.unwrap().success(),
			"'echo ok' must exit with status 0"
		);
	}

	#[tokio::test]
	async fn real_runner_run_streaming_returns_success() {
		// Exercises RealCommandRunner::run_streaming so the .status() call is
		// covered — mutations that replace it with something else would break this.
		// "echo ok" works on both /bin/sh and cmd.exe.
		let runner = RealCommandRunner;
		let cwd = std::env::temp_dir();
		let result = runner.run_streaming("echo ok", &cwd).await;
		assert!(result.is_ok(), "run_streaming should succeed: {result:?}");
		assert!(
			result.unwrap().success(),
			"'echo ok' must exit with status 0"
		);
	}

	#[tokio::test]
	async fn real_runner_run_returns_success() {
		// `run` is the read-only variant; `git --version` is the same prerequisite
		// used by other RealCommandRunner tests.
		let runner = RealCommandRunner;
		let cwd = std::env::temp_dir();
		let output = runner
			.run("git", &["--version"], &cwd)
			.await
			.expect("git --version should succeed");
		assert!(output.status.success());
		assert!(String::from_utf8_lossy(&output.stdout).contains("git"));
	}

	#[tokio::test]
	async fn real_runner_run_mut_returns_success() {
		// `run_mut` is the mutating variant; structurally identical to `run` for
		// RealCommandRunner but the dry-run decorator changes their behaviour.
		let runner = RealCommandRunner;
		let cwd = std::env::temp_dir();
		let output = runner
			.run_mut("git", &["--version"], &cwd)
			.await
			.expect("git --version should succeed");
		assert!(output.status.success());
	}

	#[tokio::test]
	async fn real_runner_run_streaming_logs_running_at_info() {
		// The centralised pre-log is load-bearing: callers delete their own
		// pre-logs on the strength of this guarantee (ADR-046 line 38).
		// A mutant that removes log::info! would silently break that contract.
		crate::test_logging::init_test_logger();
		let _ = crate::test_logging::take_logs();
		let runner = RealCommandRunner;
		let cwd = std::env::temp_dir();
		let _ = runner.run_streaming("echo ok", &cwd).await;
		let logs = crate::test_logging::take_logs();
		let (level, msg) = logs
			.iter()
			.find(|(_, m)| m.contains("Running:"))
			.expect("expected a 'Running:' log message before streaming command");
		assert_eq!(*level, log::Level::Info, "pre-log must be at info level");
		assert!(msg.contains("echo ok"), "pre-log must include the command");
	}
}
