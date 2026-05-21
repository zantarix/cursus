use log::Log as _;

use crate::test_logging::*;

#[test]
fn test_logger_enabled_returns_true_for_any_metadata() {
	let meta = log::Metadata::builder()
		.level(log::Level::Info)
		.target("test")
		.build();
	assert!(TestLogger.enabled(&meta));
}
