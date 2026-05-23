//! Command-line interface for cursus.

mod change;
mod ci;
mod init;
mod prepare;
mod publish;
mod verify;

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
pub use verify::VerifyArgs;
pub(crate) use verify::cmd_verify;

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
	/// Verify that the current branch adds at least one changeset (for CI use)
	Verify(VerifyArgs),
}

#[cfg(test)]
mod tests;
