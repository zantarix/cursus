//! General-purpose utility helpers.

/// Returns today's date as an ISO 8601 string (`YYYY-MM-DD`) in UTC.
pub fn today_iso_date() -> String {
	let now = time::OffsetDateTime::now_utc();
	format!(
		"{:04}-{:02}-{:02}",
		now.year(),
		now.month() as u8,
		now.day()
	)
}
