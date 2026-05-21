//! The `verify` subcommand — checks that the current branch adds at least one changeset.

use std::process::ExitCode;

use clap::Args;
use log::{debug, info};

use crate::git::ref_format::validate_revision;
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
pub(crate) async fn cmd_verify(args: &VerifyArgs, env: &crate::Env) -> anyhow::Result<ExitCode> {
	let git = env.git();
	debug!("Verifying changesets against base ref: {}", args.base);

	validate_revision(&args.base)?;

	let range = format!("{}..HEAD", args.base);
	let names = git
		.diff_names(&["--diff-filter=A", &range, "--", ".cursus/"])
		.await?;

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
