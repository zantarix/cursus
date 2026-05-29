//! Cursus is a CLI tool that manages project configuration via an interactive TUI setup wizard.

// `cargo llvm-cov` sets `--cfg coverage` and `--cfg coverage_nightly` as a pair.
// We gate `feature(coverage_attribute)` on `coverage_nightly` so the unstable
// `coverage(off)` attribute is only enabled under coverage measurement. Normal
// `cargo build`/`test`/`clippy` and downstream stable builds don't set the cfg,
// so this is inert there.
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

pub mod cli;
pub mod command;
pub mod conventional_commit;
pub(crate) mod env;
pub mod filesystem;
pub mod forge;
pub mod git;
pub mod locale;
pub mod model;
pub mod package_manager;
pub mod path;
pub mod redact;
pub(crate) mod shell;
pub mod tui;
pub mod utils;

#[cfg(any(test, feature = "test-support"))]
pub mod test_logging;

#[cfg(test)]
mod tests;

use std::process::ExitCode;

pub use env::Env;

/// Dispatches a pre-parsed CLI to the appropriate subcommand.
///
/// `config` is `None` when no `.cursus/config.toml` exists (e.g. a fresh
/// repo before `cursus init`). Commands that require configuration will
/// produce a clear error.
pub async fn run(
	cli: cli::Cli,
	env: Env,
	config: Option<model::config::Config>,
) -> anyhow::Result<ExitCode> {
	// Set the process-global locale from the environment before any output.
	locale::set_locale(env.locale());

	let dry_run = cli.global.dry_run;

	match cli.command {
		Some(cli::Command::Init(args)) => cli::cmd_init(&args, &cli.global, &env).await,
		Some(cli::Command::Verify(args)) => cli::cmd_verify(&args, &env).await,
		command => {
			let config = config.ok_or_else(|| {
				anyhow::anyhow!("No configuration found. Run 'cursus init' to create one.")
			})?;
			match command {
				Some(cli::Command::Change(args)) => {
					cli::cmd_change(&args, &cli.global, &env, config).await
				}
				Some(cli::Command::Prepare(args)) => {
					cli::cmd_prepare(&args, dry_run, &env, config).await
				}
				Some(cli::Command::Publish(args)) => {
					cli::cmd_publish(&args, dry_run, &env, config).await
				}
				Some(cli::Command::Ci(args)) => cli::cmd_ci(&args, dry_run, &env, config).await,
				None => {
					cli::cmd_change(&cli::ChangeArgs::default(), &cli.global, &env, config).await
				}
				Some(cli::Command::Init(_)) | Some(cli::Command::Verify(_)) => {
					// The outer match arms already handle Init and Verify; these arms cannot be reached.
					anyhow::bail!(
						"Unexpected Init/Verify command in inner dispatch - this is a bug, please report it."
					)
				}
			}
		}
	}
}
