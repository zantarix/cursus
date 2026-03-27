//! The `verify` subcommand — checks that the current branch adds at least one changeset.

use std::process::ExitCode;

use clap::Args;
use log::{debug, info};

use crate::model::changeset::filter_changeset_paths;

/// Arguments for the `verify` subcommand.
#[derive(Args, Debug, Clone)]
pub struct VerifyArgs {
	/// Base ref to compare against (e.g. `origin/HEAD`, `main`)
	#[arg(long, default_value = "origin/HEAD")]
	pub base: String,
}

impl Default for VerifyArgs {
	fn default() -> Self {
		Self {
			base: "origin/HEAD".to_string(),
		}
	}
}

/// Runs the `verify` subcommand.
///
/// Checks whether the current branch has added at least one changeset file
/// compared to `args.base`. Returns:
/// - `ExitCode::SUCCESS` (0) if at least one changeset was added.
/// - `ExitCode::from(2)` if no changeset was added.
/// - Propagates errors as `Err` (exit code 1 from the caller).
pub(crate) fn cmd_verify(args: &VerifyArgs, env: &crate::Env) -> anyhow::Result<ExitCode> {
	let git = env.git();
	debug!("Verifying changesets against base ref: {}", args.base);

	if args.base.is_empty() {
		anyhow::bail!("Invalid base ref: base ref must not be empty");
	}
	if args.base.starts_with('-') {
		anyhow::bail!(
			"Invalid base ref '{}': base refs must not start with '-'",
			args.base
		);
	}

	let range = format!("{}..HEAD", args.base);
	let names = git.diff_names(&["--diff-filter=A", &range, "--", ".cursus/"])?;

	let changesets: Vec<&str> = filter_changeset_paths(&names);

	if changesets.is_empty() {
		log::warn!("{}", crate::t!("verify-no-changeset", "base" => &args.base));
		return Ok(ExitCode::from(2));
	}

	log::info!(
		"{}",
		crate::t!("verify-found-changesets", "count" => changesets.len())
	);
	for name in &changesets {
		info!("  {name}");
	}
	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use clap::Parser;
	use tempfile::TempDir;

	use super::*;
	use crate::cli::Cli;
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

	#[test]
	fn verify_args_default() {
		let args = VerifyArgs::default();
		assert_eq!(args.base, "origin/HEAD");
	}

	#[test]
	fn verify_parses_default_base() {
		let cli = Cli::try_parse_from(["cursus", "--no-interactive", "verify"]).unwrap();
		match cli.command {
			Some(crate::cli::Command::Verify(args)) => {
				assert_eq!(args.base, "origin/HEAD");
			}
			_ => panic!("Expected Verify command"),
		}
	}

	#[test]
	fn verify_parses_custom_base() {
		let cli = Cli::try_parse_from(["cursus", "--no-interactive", "verify", "--base", "main"])
			.unwrap();
		match cli.command {
			Some(crate::cli::Command::Verify(args)) => {
				assert_eq!(args.base, "main");
			}
			_ => panic!("Expected Verify command"),
		}
	}

	#[test]
	fn verify_rejects_dash_prefix_base() {
		let (env, _dir) = make_env();
		let args = VerifyArgs {
			base: "--output=/tmp/pwned".to_string(),
		};
		let err = cmd_verify(&args, &env).unwrap_err();
		assert!(
			err.to_string().contains("must not start with '-'"),
			"unexpected error: {err}"
		);
	}

	#[test]
	fn verify_rejects_empty_base() {
		let (env, _dir) = make_env();
		let args = VerifyArgs {
			base: String::new(),
		};
		let err = cmd_verify(&args, &env).unwrap_err();
		assert!(
			err.to_string().contains("must not be empty"),
			"unexpected error: {err}"
		);
	}
}
