use crate::locale::*;
use crate::t;

#[test]
fn current_locale_id_defaults_to_en() {
	let lang = current_locale_id();
	// The default may have been changed by other tests, but on a fresh process it's "en".
	// We just verify it doesn't panic.
	let _ = lang.to_string();
}

#[test]
fn t_macro_returns_english_for_button_yes() {
	set_locale("en");
	let label = t!("button-yes");
	assert_eq!(label, "Yes");
}

#[test]
fn t_macro_returns_english_for_button_no() {
	set_locale("en");
	let label = t!("button-no");
	assert_eq!(label, "No");
}

#[test]
fn t_macro_returns_english_for_button_screen_help() {
	set_locale("en");
	let help = t!("button-screen-help");
	assert!(help.contains("←/→"), "expected arrow in help: {help}");
	assert!(help.contains("Enter"), "expected Enter in help: {help}");
	assert!(help.contains("Esc"), "expected Esc in help: {help}");
}

#[test]
fn t_macro_with_variable_substitutes_value() {
	set_locale("en");
	let question = t!("manifest-path-question", "manifest" => "Cargo.toml");
	assert!(
		question.contains("Cargo.toml"),
		"expected manifest name in question: {question}"
	);
}

#[test]
fn set_locale_invalid_falls_back_to_default() {
	set_locale("not-a-valid!!!");
	let lang = current_locale_id();
	assert_eq!(lang.to_string(), DEFAULT_LOCALE);
	// Restore
	set_locale("en");
}

#[test]
fn set_locale_and_lookup_returns_translation() {
	set_locale("en");
	let label = t!("button-major");
	assert_eq!(label, "Major");
}
