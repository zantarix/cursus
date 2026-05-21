use crate::utils::*;

#[test]
fn today_iso_date_has_correct_format() {
	let date = today_iso_date();
	assert_eq!(date.len(), 10, "date should be 10 chars: {date}");
	assert_eq!(&date[4..5], "-", "missing hyphen after year: {date}");
	assert_eq!(&date[7..8], "-", "missing hyphen after month: {date}");
	assert!(
		date[..4].chars().all(|c| c.is_ascii_digit()),
		"year should be digits: {date}"
	);
	assert!(
		date[5..7].chars().all(|c| c.is_ascii_digit()),
		"month should be digits: {date}"
	);
	assert!(
		date[8..].chars().all(|c| c.is_ascii_digit()),
		"day should be digits: {date}"
	);
}
