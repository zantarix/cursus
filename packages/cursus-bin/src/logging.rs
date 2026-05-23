//! Process-wide logging setup for the cursus binary.
//!
//! Provides a minimal [`log::Log`] implementation that splits output by
//! level (Info/Debug/Trace → stdout, Warn/Error → stderr) and helpers to
//! resolve the user-requested log level from parsed CLI flags.

/// A minimal `log::Log` implementation that splits output by level.
///
/// Info/Debug/Trace go to stdout; Warn/Error go to stderr.
/// Formatting mirrors the previous fern configuration.
struct CliLogger {
	stderr_is_terminal: bool,
}

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
				let stderr = std::io::stderr();
				if self.stderr_is_terminal {
					let _ = writeln!(stderr.lock(), "\x1b[33m[warning] {args}\x1b[0m");
				} else {
					let _ = writeln!(stderr.lock(), "[warning] {args}");
				}
			}
			log::Level::Error => {
				let stderr = std::io::stderr();
				if self.stderr_is_terminal {
					let _ = writeln!(stderr.lock(), "\x1b[91m[error] {args}\x1b[0m");
				} else {
					let _ = writeln!(stderr.lock(), "[error] {args}");
				}
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

static LOGGER: std::sync::OnceLock<CliLogger> = std::sync::OnceLock::new();

/// Installs the process-global [`log`] logger at the given maximum level.
///
/// Safe to call more than once; the underlying logger is initialised exactly
/// once via [`std::sync::OnceLock`], and subsequent calls just update the
/// max-level filter.
#[coverage(off)]
#[mutants::skip]
pub(crate) fn init_logging(level: log::LevelFilter) {
	use std::io::IsTerminal as _;
	let logger = LOGGER.get_or_init(|| CliLogger {
		stderr_is_terminal: std::io::stderr().is_terminal(),
	});
	if let Err(e) = log::set_logger(logger) {
		eprintln!("warning: failed to initialize logging: {e}");
	}
	log::set_max_level(level);
}

/// Maps parsed global flags to the corresponding [`log::LevelFilter`].
///
/// `-s` / `--silent` → `Error`, default → `Info`, `-v` → `Debug`, `-vv+` → `Trace`.
#[coverage(off)]
#[mutants::skip]
pub(crate) fn determine_log_level(global: &cursus::cli::GlobalArgs) -> log::LevelFilter {
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
