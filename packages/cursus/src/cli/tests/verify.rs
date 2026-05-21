use std::sync::Arc;

use clap::Parser;
use tempfile::TempDir;

use crate::cli::Cli;
use crate::cli::verify::*;
use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::filesystem::LocalFilesystem;
use crate::path::AbsolutePath;

fn make_env() -> (crate::Env, TempDir) {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	let runner = Arc::new(RecordingCommandRunner::new(0));
	let path = AbsolutePath::new(dir.path()).unwrap();
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		Arc::new(crate::git::GitWorkdir::new(
			runner as Arc<dyn CommandRunner>,
			path,
		)),
	);
	(env, dir)
}

#[tokio::test]
async fn verify_args_default() {
	let args = VerifyArgs::default();
	assert_eq!(args.base, "origin/HEAD");
}

#[tokio::test]
async fn verify_parses_default_base() {
	let cli = Cli::try_parse_from(["cursus", "--no-interactive", "verify"]).unwrap();
	match cli.command {
		Some(crate::cli::Command::Verify(args)) => {
			assert_eq!(args.base, "origin/HEAD");
		}
		_ => panic!("Expected Verify command"),
	}
}

#[tokio::test]
async fn verify_parses_custom_base() {
	let cli =
		Cli::try_parse_from(["cursus", "--no-interactive", "verify", "--base", "main"]).unwrap();
	match cli.command {
		Some(crate::cli::Command::Verify(args)) => {
			assert_eq!(args.base, "main");
		}
		_ => panic!("Expected Verify command"),
	}
}

#[tokio::test]
async fn verify_rejects_dash_prefix_base() {
	let (env, _dir) = make_env();
	let args = VerifyArgs {
		base: "--output=/tmp/pwned".to_string(),
	};
	let err = cmd_verify(&args, &env).await.unwrap_err();
	assert!(
		err.to_string().contains("must not start with '-'"),
		"unexpected error: {err}"
	);
}

#[tokio::test]
async fn verify_rejects_empty_base() {
	let (env, _dir) = make_env();
	let args = VerifyArgs {
		base: String::new(),
	};
	let err = cmd_verify(&args, &env).await.unwrap_err();
	assert!(
		err.to_string().contains("must not be empty"),
		"unexpected error: {err}"
	);
}
