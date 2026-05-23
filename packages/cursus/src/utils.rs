//! General-purpose utility helpers.

/// Returns today's date as an ISO 8601 string (`YYYY-MM-DD`) in UTC.
pub fn today_iso_date() -> String {
	chrono::Utc::now().format("%Y-%m-%d").to_string()
}
