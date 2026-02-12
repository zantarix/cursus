use std::process::ExitCode;

fn main() -> ExitCode {
	let cwd = match std::env::current_dir() {
		Ok(cwd) => cwd,
		Err(e) => {
			eprintln!("Error: {e:#}");
			return ExitCode::FAILURE;
		}
	};

	match chronicle::run(std::env::args_os(), &cwd) {
		Ok(code) => code,
		Err(e) => {
			eprintln!("Error: {e:#}");
			ExitCode::FAILURE
		}
	}
}
