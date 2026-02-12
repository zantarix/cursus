//! Command-line interface for chronicle.

mod change;
mod init;

pub use change::{ChangeArgs, cmd_change};
pub use init::{InitArgs, cmd_init};

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
}

impl Default for GlobalArgs {
	fn default() -> Self {
		Self {
			interactive: true,
			no_interactive: false,
		}
	}
}

/// Command-line interface for chronicle.
#[derive(Parser)]
#[command(name = "chronicle", about = "Release management")]
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
	/// Initialize a new chronicle configuration using the setup wizard
	Init(InitArgs),
}
