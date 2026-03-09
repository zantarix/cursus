#![feature(coverage_attribute)]

use std::process::ExitCode;
use std::sync::Arc;

use chronicle::command::RealCommandRunner;

#[coverage(off)]
#[mutants::skip]
fn init_logging() {
	if let Err(e) = fern::Dispatch::new()
		.format(|out, message, record| match record.level() {
			log::Level::Info => out.finish(format_args!("{message}")),
			log::Level::Warn => out.finish(format_args!("[warning] {message}")),
			log::Level::Error => out.finish(format_args!("[error] {message}")),
			log::Level::Debug => out.finish(format_args!("debug: {}: {message}", record.target())),
			log::Level::Trace => out.finish(format_args!("trace: {}: {message}", record.target())),
		})
		.level(log::LevelFilter::Info)
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

#[coverage(off)]
#[mutants::skip]
fn main() -> ExitCode {
	init_logging();

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
		github_token: std::env::var("GH_TOKEN")
			.ok()
			.or_else(|| std::env::var("GITHUB_TOKEN").ok()),
	};
	let runner = Arc::new(RealCommandRunner);
	match chronicle::run(std::env::args_os(), &cwd, env, runner) {
		Ok(code) => code,
		Err(e) => {
			log::error!("{e:#}");
			ExitCode::FAILURE
		}
	}
}
