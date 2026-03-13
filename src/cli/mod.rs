//! Command-line interface for cursus.

mod change;
mod ci;
mod init;
mod prepare;
mod publish;

pub use change::ChangeArgs;
pub(crate) use change::cmd_change;
pub use ci::CiArgs;
pub(crate) use ci::cmd_ci;
pub use init::InitArgs;
pub(crate) use init::cmd_init;
pub use prepare::PrepareArgs;
pub(crate) use prepare::cmd_prepare;
pub use publish::PublishArgs;
pub(crate) use publish::cmd_publish;

use clap::{ArgAction, Args, Parser, Subcommand};

/// Global arguments that apply to all subcommands.
#[derive(Args, Debug, Clone)]
pub struct GlobalArgs {
	/// Enable interactive mode (default)
	#[arg(long, global = true, default_value_t = true, action = ArgAction::SetTrue, overrides_with = "no_interactive")]
	pub interactive: bool,

	/// Disable interactive prompts
	#[arg(long, global = true, action = ArgAction::SetTrue, overrides_with = "interactive")]
	pub no_interactive: bool,

	/// Increase log verbosity; use twice (`-vv`) for trace output
	#[arg(short = 'v', long, global = true, action = ArgAction::Count, conflicts_with = "silent")]
	pub verbose: u8,

	/// Suppress all output except errors
	#[arg(short = 's', long, global = true, action = ArgAction::SetTrue, conflicts_with = "verbose")]
	pub silent: bool,

	/// Preview changes without modifying any files or running registry commands
	#[arg(short = 'n', long, global = true, action = ArgAction::SetTrue)]
	pub dry_run: bool,
}

impl Default for GlobalArgs {
	fn default() -> Self {
		Self {
			interactive: true,
			no_interactive: false,
			verbose: 0,
			silent: false,
			dry_run: false,
		}
	}
}

/// Command-line interface for cursus.
#[derive(Parser)]
#[command(name = "cursus", about = "Release management", version)]
pub struct Cli {
	#[command(flatten)]
	pub global: GlobalArgs,

	#[command(subcommand)]
	pub command: Option<Command>,
}

/// Available subcommands.
#[derive(Subcommand)]
pub enum Command {
	/// Record a change to the project
	Change(ChangeArgs),
	/// Auto-detect repo state and run prepare or publish as needed (for CI use)
	Ci(CiArgs),
	/// Initialize a new cursus configuration using the setup wizard
	Init(InitArgs),
	/// Prepare a release: bump versions, generate changelogs, manage branches
	Prepare(PrepareArgs),
	/// Publish packages to their registries
	Publish(PublishArgs),
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn global_args_default() {
		let args = GlobalArgs::default();
		assert!(args.interactive);
		assert!(!args.no_interactive);
		assert_eq!(args.verbose, 0);
		assert!(!args.silent);
		assert!(!args.dry_run);
	}

	#[test]
	fn dry_run_flag_sets_true() {
		let cli = Cli::try_parse_from(["cursus", "--dry-run"]).unwrap();
		assert!(cli.global.dry_run);
	}

	#[test]
	fn dry_run_short_flag_sets_true() {
		let cli = Cli::try_parse_from(["cursus", "-n"]).unwrap();
		assert!(cli.global.dry_run);
	}

	#[test]
	fn dry_run_flag_after_subcommand_sets_true() {
		let cli = Cli::try_parse_from(["cursus", "prepare", "--dry-run"]).unwrap();
		assert!(cli.global.dry_run);
	}

	#[test]
	fn dry_run_short_flag_after_subcommand_sets_true() {
		let cli = Cli::try_parse_from(["cursus", "prepare", "-n"]).unwrap();
		assert!(cli.global.dry_run);
	}

	#[test]
	fn verbose_flag_sets_count() {
		let cli = Cli::try_parse_from(["cursus", "-v"]).unwrap();
		assert_eq!(cli.global.verbose, 1);
	}

	#[test]
	fn verbose_flag_stacks() {
		let cli = Cli::try_parse_from(["cursus", "-vv"]).unwrap();
		assert_eq!(cli.global.verbose, 2);
	}

	#[test]
	fn silent_flag_sets_true() {
		let cli = Cli::try_parse_from(["cursus", "-s"]).unwrap();
		assert!(cli.global.silent);
	}

	#[test]
	fn verbose_flag_three_sets_count_three() {
		// Three or more -v flags all map to Trace in determine_log_level; this
		// test documents that the u8 count saturates safely and keeps counting.
		let cli = Cli::try_parse_from(["cursus", "-vvv"]).unwrap();
		assert_eq!(cli.global.verbose, 3);
	}

	#[test]
	fn verbose_and_silent_conflict() {
		let result = Cli::try_parse_from(["cursus", "-v", "-s"]);
		assert!(result.is_err());
	}
}
