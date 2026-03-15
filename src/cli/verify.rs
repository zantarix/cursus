//! The `verify` subcommand — checks that the current branch adds at least one changeset.

use std::process::ExitCode;

use clap::Args;
use log::{debug, info, warn};

use crate::git::GitWorkdir;
use crate::model::changeset::is_changeset_filename;

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
pub(crate) fn cmd_verify(git: &GitWorkdir, args: &VerifyArgs) -> anyhow::Result<ExitCode> {
	debug!("Verifying changesets against base ref: {}", args.base);

	let range = format!("{}..HEAD", args.base);
	let names = git.diff_names(&["--diff-filter=A", &range, "--", ".cursus/"])?;

	let changesets: Vec<String> = names
		.into_iter()
		.filter(|name| {
			let filename = std::path::Path::new(name)
				.file_name()
				.and_then(|n| n.to_str())
				.unwrap_or(name.as_str());
			is_changeset_filename(filename)
		})
		.collect();

	if changesets.is_empty() {
		warn!(
			"No changeset files found on this branch compared to {}. \
			Run `cursus change` to record your changes.",
			args.base
		);
		return Ok(ExitCode::from(2));
	}

	info!("Found {} changeset(s) on this branch:", changesets.len());
	for name in &changesets {
		info!("  {name}");
	}
	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use clap::Parser;

	use super::*;
	use crate::cli::Cli;

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
}
