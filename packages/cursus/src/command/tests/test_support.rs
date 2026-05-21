mod dispatching_tests {
	use std::path::Path;

	use crate::command::test_support::*;
	use crate::command::{CommandRunner, shell_program};

	#[tokio::test]
	async fn dispatching_runner_returns_default_when_no_rule_matches() {
		let runner = DispatchingCommandRunner::new(1);
		let cwd = Path::new("/tmp");
		let output = runner.run("unknown", &[], cwd).await.unwrap();
		assert!(!output.status.success());
	}

	#[tokio::test]
	async fn dispatching_runner_matches_program_name() {
		let runner = DispatchingCommandRunner::new(1).on("git", 0);
		let cwd = Path::new("/tmp");
		let output = runner.run("git", &["status"], cwd).await.unwrap();
		assert!(output.status.success());
	}

	#[tokio::test]
	async fn dispatching_runner_first_matching_rule_wins() {
		let runner = DispatchingCommandRunner::new(1).on("git", 0).on("git", 2); // should never be reached
		let cwd = Path::new("/tmp");
		let output = runner.run("git", &[], cwd).await.unwrap();
		assert!(output.status.success());
	}

	#[tokio::test]
	async fn dispatching_runner_matches_args_prefix() {
		let runner =
			DispatchingCommandRunner::new(0).on_with_args("git", vec!["push".to_string()], 42);
		let cwd = Path::new("/tmp");
		let output = runner
			.run("git", &["push", "origin", "HEAD"], cwd)
			.await
			.unwrap();
		#[cfg(unix)]
		{
			use std::os::unix::process::ExitStatusExt;
			assert_eq!(output.status.into_raw(), 42 << 8);
		}
	}

	#[tokio::test]
	async fn dispatching_runner_falls_through_when_args_prefix_does_not_match() {
		let runner =
			DispatchingCommandRunner::new(0).on_with_args("git", vec!["push".to_string()], 42);
		let cwd = Path::new("/tmp");
		// "fetch" does not match the "push" prefix rule; default (0) is used
		let output = runner.run("git", &["fetch"], cwd).await.unwrap();
		assert!(output.status.success());
	}

	#[tokio::test]
	async fn dispatching_runner_returns_configured_stdout() {
		let runner = DispatchingCommandRunner::new(0).on_stdout("npm", 0, b"test-user\n".to_vec());
		let cwd = Path::new("/tmp");
		let output = runner.run("npm", &["whoami"], cwd).await.unwrap();
		assert_eq!(output.stdout, b"test-user\n");
	}

	#[tokio::test]
	async fn dispatching_runner_returns_configured_stderr() {
		let runner = DispatchingCommandRunner::new(0).on_stderr(
			"cargo",
			1,
			b"error: not logged in\n".to_vec(),
		);
		let cwd = Path::new("/tmp");
		let output = runner.run("cargo", &[], cwd).await.unwrap();
		assert_eq!(output.stderr, b"error: not logged in\n");
	}

	#[tokio::test]
	async fn dispatching_runner_on_rule_accepts_full_dispatch_rule() {
		let rule = DispatchRule {
			program: "npm".to_string(),
			args: Some(vec!["whoami".to_string()]),
			exit_code: 0,
			stdout: b"alice\n".to_vec(),
			stderr: Vec::new(),
		};
		let runner = DispatchingCommandRunner::new(1).on_rule(rule);
		let cwd = Path::new("/tmp");
		let output = runner.run("npm", &["whoami"], cwd).await.unwrap();
		assert_eq!(output.stdout, b"alice\n");
		assert!(output.status.success());
	}

	#[tokio::test]
	async fn dispatching_runner_records_invocations() {
		let runner = DispatchingCommandRunner::new(0).on("git", 0);
		let cwd = Path::new("/tmp");
		let _ = runner.run("git", &["status"], cwd).await.unwrap();
		let _ = runner
			.run_mut("git", &["commit", "-m", "msg"], cwd)
			.await
			.unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 2);
		assert_eq!(invocations[0].args, vec!["status"]);
		assert_eq!(invocations[1].args, vec!["commit", "-m", "msg"]);
	}

	#[tokio::test]
	async fn dispatching_runner_records_streaming_invocations() {
		let runner = DispatchingCommandRunner::new(0);
		let cwd = Path::new("/tmp");
		let _ = runner.run_streaming("npm install", cwd).await.unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_shell);
		assert!(invocations[0].is_streaming);
		assert!(!invocations[0].is_interactive);
		assert_eq!(invocations[0].program, shell_program());
	}

	#[tokio::test]
	async fn dispatching_runner_records_interactive_invocations() {
		let runner = DispatchingCommandRunner::new(0);
		let cwd = Path::new("/tmp");
		let _ = runner
			.run_interactive("vim", &["file.txt"], cwd)
			.await
			.unwrap();
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].is_interactive);
		assert_eq!(invocations[0].program, "vim");
	}
}
