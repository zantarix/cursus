use std::process::ExitCode;

fn main() -> ExitCode {
	match chronicle::run(std::env::args_os()) {
		Ok(code) => code,
		Err(e) => {
			eprintln!("Error: {e:#}");
			ExitCode::FAILURE
		}
	}
}
