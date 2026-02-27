#![feature(coverage_attribute)]

use std::process::ExitCode;

#[coverage(off)]
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
	match chronicle::run(std::env::args_os(), &cwd, env) {
		Ok(code) => code,
		Err(e) => {
			eprintln!("Error: {e:#}");
			ExitCode::FAILURE
		}
	}
}
