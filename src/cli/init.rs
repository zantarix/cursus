//! The `init` subcommand.

use std::process::ExitCode;

use anyhow::bail;
use clap::Args;
use log::info;

use crate::Env;
use crate::model::config::render_init_template;
use crate::path::AbsolutePath;
use crate::tui::init;

use super::GlobalArgs;

/// Arguments for the `init` subcommand.
#[derive(Args, Default)]
pub struct InitArgs {}

/// Runs the `init` subcommand.
pub(crate) fn cmd_init(
	git_workdir: &AbsolutePath,
	_args: &InitArgs,
	global: &GlobalArgs,
	env: &Env,
) -> anyhow::Result<ExitCode> {
	if global.no_interactive {
		bail!(
			"chronicle init is interactive-only. Scripts can write .chronicle/config.toml directly."
		);
	}

	let result = match init::run(git_workdir, env)? {
		Some(r) => r,
		None => return Ok(ExitCode::from(2)),
	};

	let config_toml = render_init_template(&result)?;

	let chronicle_dir = git_workdir.as_ref().join(".chronicle");
	std::fs::create_dir_all(&chronicle_dir)?;

	let config_path = chronicle_dir.join("config.toml");
	std::fs::write(&config_path, &config_toml)?;
	info!("Created {}", config_path.display());

	if result.open_editor {
		env.run_editor_on(&config_path, git_workdir.as_ref())?;
	}

	Ok(ExitCode::SUCCESS)
}
