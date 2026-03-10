#![feature(coverage_attribute)]

use std::process::ExitCode;
use std::sync::Arc;

use chronicle::command::{RealCommandRunner, VerboseCommandRunner};
use clap::Parser as _;

#[coverage(off)]
#[mutants::skip]
fn init_logging(level: log::LevelFilter) {
	if let Err(e) = fern::Dispatch::new()
		.format(|out, message, record| match record.level() {
			log::Level::Info => out.finish(format_args!("{message}")),
			log::Level::Warn => out.finish(format_args!("[warning] {message}")),
			log::Level::Error => out.finish(format_args!("[error] {message}")),
			log::Level::Debug => out.finish(format_args!("debug: {}: {message}", record.target())),
			log::Level::Trace => out.finish(format_args!("trace: {}: {message}", record.target())),
		})
		.level(level)
		.chain(
			fern::Dispatch::new()
				.filter(|meta| meta.level() >= log::Level::Info)
				.chain(std::io::stdout()),
		)
		.chain(
			fern::Dispatch::new()
				.filter(|meta| meta.level() < log::Level::Info)
				.chain(std::io::stderr()),
		)
		.apply()
	{
		eprintln!("warning: failed to initialize logging: {e}");
	}
}

/// Maps parsed global flags to the corresponding [`log::LevelFilter`].
///
/// `-s` / `--silent` → `Error`, default → `Info`, `-v` → `Debug`, `-vv+` → `Trace`.
#[coverage(off)]
#[mutants::skip]
fn determine_log_level(global: &chronicle::cli::GlobalArgs) -> log::LevelFilter {
	if global.silent {
		log::LevelFilter::Error
	} else {
		match global.verbose {
			0 => log::LevelFilter::Info,
			1 => log::LevelFilter::Debug,
			_ => log::LevelFilter::Trace,
		}
	}
}

#[coverage(off)]
#[mutants::skip]
fn main() -> ExitCode {
	// Parse args exactly once. Logging is initialised immediately after so
	// that every subsequent operation benefits from the user-requested level.
	let cli = match chronicle::cli::Cli::try_parse() {
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

	let cwd = match std::env::current_dir() {
		Ok(cwd) => cwd,
		Err(e) => {
			log::error!("{e:#}");
			return ExitCode::FAILURE;
		}
	};

	let env = chronicle::Env {
		visual: std::env::var("VISUAL").ok(),
		editor: std::env::var("EDITOR").ok(),
	};
	let github_client: Option<Arc<dyn chronicle::github::client::GitHubClient>> =
		std::env::var("GH_TOKEN")
			.ok()
			.or_else(|| std::env::var("GITHUB_TOKEN").ok())
			.map(|token| {
				Arc::new(chronicle::github::RestGitHubClient::new(token))
					as Arc<dyn chronicle::github::client::GitHubClient>
			});
	let runner: Arc<dyn chronicle::command::CommandRunner> =
		Arc::new(VerboseCommandRunner::new(RealCommandRunner));
	match chronicle::run_with(cli, &cwd, env, runner, github_client) {
		Ok(code) => code,
		Err(e) => {
			log::error!("{e:#}");
			ExitCode::FAILURE
		}
	}
}
