//! Shared test-logging infrastructure for capturing `log` output in unit tests.
//!
//! The `log` crate only allows one global logger to be registered per process,
//! so all test modules must share the same instance.  This module provides a
//! single [`TestLogger`] static backed by thread-local storage, ensuring
//! parallel tests in different threads do not see each other's messages.

use std::cell::RefCell;

thread_local! {
	/// Per-thread log capture buffer, populated by [`TestLogger`].
	pub static CAPTURED_LOGS: RefCell<Vec<(log::Level, String)>> = const { RefCell::new(Vec::new()) };
}

/// A `log::Log` implementation that appends every record to [`CAPTURED_LOGS`].
pub struct TestLogger;

impl log::Log for TestLogger {
	fn enabled(&self, _: &log::Metadata) -> bool {
		true
	}

	fn log(&self, record: &log::Record) {
		CAPTURED_LOGS.with(|logs| {
			logs.borrow_mut()
				.push((record.level(), format!("{}", record.args())));
		});
	}

	fn flush(&self) {}
}

/// The single global test logger instance.
pub static TEST_LOGGER: TestLogger = TestLogger;

/// Registers [`TEST_LOGGER`] as the global logger and sets the max level to
/// `Trace`.  Safe to call from multiple tests — subsequent calls are no-ops.
pub fn init_test_logger() {
	let _ = log::set_logger(&TEST_LOGGER);
	log::set_max_level(log::LevelFilter::Trace);
}

/// Drains and returns all log records captured on the current thread since the
/// last call to this function (or since the thread started).
pub fn take_logs() -> Vec<(log::Level, String)> {
	CAPTURED_LOGS.with(|logs| std::mem::take(&mut *logs.borrow_mut()))
}
