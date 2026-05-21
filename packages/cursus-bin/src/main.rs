#![feature(coverage_attribute)]

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser as _;
use cursus::command::{DryRunCommandRunner, RealCommandRunner, VerboseCommandRunner};

mod env_helpers;
mod forge_resolution;
mod git_setup;
mod logging;

#[cfg(test)]
mod tests;

use env_helpers::{detect_locale, env_first};
use forge_resolution::{build_octocrab, resolve_forge_client_for_env};
use git_setup::build_git;
use logging::{determine_log_level, init_logging};

/// Installs the rustls process-default CryptoProvider exactly once.
///
/// Two TLS implementations (`octocrab` and `gitlab`) live in the dep graph;
/// our feature selection ensures only `aws-lc-rs` is linked, but we still
/// install it explicitly so behaviour is deterministic regardless of dep
/// graph drift. `install_default()` returns `Err` when a provider has
/// already been installed — that path is harmless and ignored.
#[coverage(off)]
#[mutants::skip]
fn install_crypto_provider() {
	static INIT: std::sync::Once = std::sync::Once::new();
	INIT.call_once(|| {
		let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
	});
}

#[coverage(off)]
#[mutants::skip]
#[tokio::main]
async fn main() -> ExitCode {
	install_crypto_provider();
	// Parse args exactly once. Logging is initialised immediately after so
	// that every subsequent operation benefits from the user-requested level.
	let cli = match cursus::cli::Cli::try_parse() {
		Ok(cli) => cli,
		Err(e) => {
			// Help / version requests also come through here; initialise
			// logging at the default level and let clap print the output.
			init_logging(log::LevelFilter::Info);
			if let Err(print_err) = e.print() {
				log::error!("failed to print help: {print_err:#}");
			}
			return if e.use_stderr() {
				ExitCode::FAILURE
			} else {
				ExitCode::SUCCESS
			};
		}
	};

	init_logging(determine_log_level(&cli.global));

	match try_main(cli).await {
		Ok(code) => code,
		Err(e) => {
			log::error!("{e:#}");
			ExitCode::FAILURE
		}
	}
}

/// Runs the application after CLI parsing and logging setup.
///
/// Separated from [`main`] so that all fallible operations can use `?`
/// with a single error-handling site in the caller.
async fn try_main(cli: cursus::cli::Cli) -> anyhow::Result<ExitCode> {
	let cwd = std::env::current_dir()?;
	let cwd_abs = cursus::path::AbsolutePath::new(&cwd)?;

	let runner: Arc<dyn cursus::command::CommandRunner> =
		Arc::new(VerboseCommandRunner::new(RealCommandRunner));
	let filesystem: Arc<dyn cursus::filesystem::Filesystem> =
		Arc::new(cursus::filesystem::LocalFilesystem);

	// Wrap the runner for dry-run BEFORE creating GitWorkdir so it
	// receives the wrapped runner.
	let runner = if cli.global.dry_run {
		Arc::new(DryRunCommandRunner::new(runner)) as Arc<dyn cursus::command::CommandRunner>
	} else {
		runner
	};

	let git_workdir = cursus::git::find_workdir(&cwd_abs, &*filesystem)
		.await
		.context("No git repository found")?;

	// Load config before constructing the final Git impl so that signed_commits
	// mode can be checked. Config loading only needs the filesystem and workdir path.
	let config = cursus::model::config::load(&*filesystem, &git_workdir).await?;

	let git_inner = Arc::new(cursus::git::GitWorkdir::new(
		Arc::clone(&runner),
		git_workdir,
	));

	// Build one octocrab client for the whole process; both the Git decorator
	// (signed commits) and the forge client (PRs/releases) share it.
	let octocrab: Option<Arc<octocrab::Octocrab>> = env_first(&["GH_TOKEN", "GITHUB_TOKEN"])
		.and_then(|t| build_octocrab(&t).ok())
		.map(Arc::new);

	let git = build_git(
		git_inner,
		Arc::clone(&filesystem),
		Arc::clone(&runner),
		&config,
		cli.global.dry_run,
		octocrab.clone(),
	)
	.await?;

	let editor = env_first(&["VISUAL", "EDITOR"]);
	let oidc_environment = env_first(&["ACTIONS_ID_TOKEN_REQUEST_URL", "CI_JOB_JWT_V2"]).is_some();
	let node_auth_token_present = env_first(&["NODE_AUTH_TOKEN"]).is_some();
	let cargo_registry_token_present = env_first(&["CARGO_REGISTRY_TOKEN"]).is_some();
	let locale = detect_locale();

	let env = cursus::Env::new(runner, filesystem, git)
		.with_editor_opt(editor)
		.with_oidc_environment(oidc_environment)
		.with_node_auth_token_present(node_auth_token_present)
		.with_cargo_registry_token_present(cargo_registry_token_present)
		.with_locale(locale);

	let (forge_client_result, gitlab_uses_job_token_only) =
		resolve_forge_client_for_env(&env, &config, octocrab).await;
	let env = env
		.with_code_forge_client_result(forge_client_result)
		.with_gitlab_uses_job_token_only(gitlab_uses_job_token_only);

	cursus::run(cli, env, config).await
}
