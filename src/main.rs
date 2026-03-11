#![feature(coverage_attribute)]

use std::process::ExitCode;
use std::sync::Arc;

use chronicle::command::{RealCommandRunner, VerboseCommandRunner};
use clap::Parser as _;

/// A minimal `log::Log` implementation that splits output by level.
///
/// Info/Debug/Trace go to stdout; Warn/Error go to stderr.
/// Formatting mirrors the previous fern configuration.
struct CliLogger;

#[coverage(off)]
#[mutants::skip]
impl log::Log for CliLogger {
	/// Always returns `true`; actual level filtering is handled by
	/// [`log::set_max_level`] in [`init_logging`].
	fn enabled(&self, _: &log::Metadata) -> bool {
		true
	}

	fn log(&self, record: &log::Record) {
		use std::io::Write as _;
		let target = record.target();
		let args = record.args();
		match record.level() {
			log::Level::Info => {
				let _ = writeln!(std::io::stdout().lock(), "{args}");
			}
			log::Level::Warn => {
				let _ = writeln!(std::io::stderr().lock(), "[warning] {args}");
			}
			log::Level::Error => {
				let _ = writeln!(std::io::stderr().lock(), "[error] {args}");
			}
			log::Level::Debug => {
				let _ = writeln!(std::io::stdout().lock(), "debug: {target}: {args}");
			}
			log::Level::Trace => {
				let _ = writeln!(std::io::stdout().lock(), "trace: {target}: {args}");
			}
		}
	}

	fn flush(&self) {
		use std::io::Write as _;
		let _ = std::io::stdout().flush();
		let _ = std::io::stderr().flush();
	}
}

static LOGGER: CliLogger = CliLogger;

#[coverage(off)]
#[mutants::skip]
fn init_logging(level: log::LevelFilter) {
	if let Err(e) = log::set_logger(&LOGGER) {
		eprintln!("warning: failed to initialize logging: {e}");
	}
	log::set_max_level(level);
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

	let runner: Arc<dyn chronicle::command::CommandRunner> =
		Arc::new(VerboseCommandRunner::new(RealCommandRunner));
	let editor = std::env::var("VISUAL")
		.ok()
		.filter(|s| !s.is_empty())
		.or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.is_empty()));
	let github_client = std::env::var("GH_TOKEN")
		.ok()
		.or_else(|| std::env::var("GITHUB_TOKEN").ok())
		.map(|token| {
			Arc::new(chronicle::github::RestGitHubClient::new(token))
				as Arc<dyn chronicle::github::client::GitHubClient>
		});
	let env = chronicle::Env::new(runner)
		.with_editor_opt(editor)
		.with_github_client_opt(github_client);
	match chronicle::run_with(cli, &cwd, env) {
		Ok(code) => code,
		Err(e) => {
			log::error!("{e:#}");
			ExitCode::FAILURE
		}
	}
}
