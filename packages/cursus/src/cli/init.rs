//! The `init` subcommand.

use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::Args;
use log::info;

use crate::Env;
use crate::model::config::render_init_template;
use crate::tui::init;

use super::GlobalArgs;

/// Arguments for the `init` subcommand.
#[derive(Args, Default)]
pub struct InitArgs {}

/// Runs the `init` subcommand.
pub(crate) async fn cmd_init(
	_args: &InitArgs,
	global: &GlobalArgs,
	env: &Env,
) -> anyhow::Result<ExitCode> {
	if global.no_interactive {
		bail!("cursus init is interactive-only. Scripts can write .cursus/config.toml directly.");
	}

	let git = env.git();
	let git_workdir = git.path();

	let detected_github = crate::github::remote::GitHubRepo::detect_in(git)
		.await
		.ok()
		.flatten();

	let env_clone = env.clone();
	let dry_run = global.dry_run;
	let result =
		match tokio::task::spawn_blocking(move || init::run(&env_clone, dry_run, detected_github))
			.await
			.context("TUI task panicked")??
		{
			Some(r) => r,
			None => return Ok(ExitCode::from(2)),
		};

	let config_toml = render_init_template(&result)?;

	if global.dry_run {
		print!("{config_toml}");
		return Ok(ExitCode::SUCCESS);
	}

	let cursus_dir = git_workdir.child(".cursus");
	env.fs().create_dir_all(&cursus_dir).await?;

	let config_path = cursus_dir.child("config.toml");
	env.fs().write(&config_path, config_toml.as_bytes()).await?;
	info!("Created {}", config_path.display());

	if result.open_editor {
		env.run_editor_on(config_path.as_ref(), git_workdir.as_ref())
			.await?;
	}

	Ok(ExitCode::SUCCESS)
}
