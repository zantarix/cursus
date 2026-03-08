#![feature(coverage_attribute)]

use std::process::ExitCode;
use std::sync::Arc;

use chronicle::command::RealCommandRunner;

#[coverage(off)]
#[mutants::skip]
fn main() -> ExitCode {
	let cwd = match std::env::current_dir() {
		Ok(cwd) => cwd,
		Err(e) => {
			eprintln!("Error: {e:#}");
			return ExitCode::FAILURE;
		}
	};

	let env = chronicle::Env {
		visual: std::env::var("VISUAL").ok(),
		editor: std::env::var("EDITOR").ok(),
	};
	let runner = Arc::new(RealCommandRunner);
	match chronicle::run(std::env::args_os(), &cwd, env, runner) {
		Ok(code) => code,
		Err(e) => {
			eprintln!("Error: {e:#}");
			ExitCode::FAILURE
		}
	}
}
