//! Cursus is a CLI tool that manages project configuration via an interactive TUI setup wizard.

#![feature(coverage_attribute)]

pub mod cli;
pub mod command;
pub mod conventional_commit;
pub(crate) mod env;
pub mod filesystem;
pub mod git;
pub mod github;
pub mod locale;
pub mod model;
pub mod package_manager;
pub mod path;
pub(crate) mod shell;
pub mod tui;
pub mod utils;

#[cfg(any(test, feature = "test-support"))]
pub mod test_logging;

use std::process::ExitCode;

pub use env::Env;

/// Dispatches a pre-parsed CLI to the appropriate subcommand.
pub fn run(cli: cli::Cli, env: Env) -> anyhow::Result<ExitCode> {
	// Set the process-global locale from the environment before any output.
	locale::set_locale(env.locale());

	let dry_run = cli.global.dry_run;

	match cli.command {
		Some(cli::Command::Init(args)) => cli::cmd_init(&args, &cli.global, &env),
		Some(cli::Command::Verify(args)) => cli::cmd_verify(&args, &env),
		command => {
			let config = model::config::load(&env)?;
			match command {
				Some(cli::Command::Change(args)) => cli::cmd_change(&args, &cli.global, config),
				Some(cli::Command::Prepare(args)) => cli::cmd_prepare(&args, dry_run, config),
				Some(cli::Command::Publish(args)) => cli::cmd_publish(&args, dry_run, config),
				Some(cli::Command::Ci(args)) => cli::cmd_ci(&args, dry_run, config),
				None => cli::cmd_change(&cli::ChangeArgs::default(), &cli.global, config),
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
