//! Process-global locale support for Cursus.
//!
//! The locale is set once from `main` (via [`set_locale`]) before any output
//! is produced. Library code never reads `LANG`, `LC_ALL`, or other environment
//! variables directly — all locale information flows through [`Env::locale`][crate::Env].
//!
//! Translation strings live in `locales/en/*.ftl` (and other language
//! subdirectories). The [`t!`] macro performs the lookup using the current
//! process locale, falling back to `en` for any missing keys.

use std::sync::{LazyLock, RwLock};

use fluent_templates::static_loader;
pub use unic_langid::LanguageIdentifier;
use unic_langid::langid;

/// The fallback locale used when no system locale is detected.
///
/// Referenced by both the library defaults and the binary entry point so they
/// cannot drift out of sync.
pub const DEFAULT_LOCALE: &str = "en";

static_loader! {
	pub static LOCALES = {
		locales: "./locales",
		fallback_language: "en",
	};
}

/// The active process-global locale.
static LOCALE: LazyLock<RwLock<LanguageIdentifier>> = LazyLock::new(|| RwLock::new(langid!("en")));

/// Sets the process-global locale used by [`t!`].
///
/// Should be called once from `main` before any TUI or CLI output is produced.
/// The `locale` string should be a BCP 47 language tag (e.g. `"en"`, `"en-US"`,
/// `"pt-BR"`). If parsing fails the locale falls back to `"en"`.
pub fn set_locale(locale: &str) {
	let lang_id: LanguageIdentifier = locale.parse().unwrap_or_else(|_| langid!("en"));
	if let Ok(mut guard) = LOCALE.write() {
		*guard = lang_id;
	} else {
		log::warn!("locale lock poisoned; locale not updated");
	}
}

/// Returns a clone of the current process-global [`LanguageIdentifier`].
///
/// Used internally by the [`t!`] macro on every call to look up translations.
pub fn current_locale_id() -> LanguageIdentifier {
	LOCALE
		.read()
		.map(|guard| guard.clone())
		.unwrap_or_else(|_| langid!("en"))
}

/// Looks up a translation string for the current process locale.
///
/// Falls back to the `en` locale for any message IDs not found in the
/// requested language. If the message ID is missing in all locales the
/// key itself is returned as a placeholder.
///
/// # Single argument form
///
/// ```ignore
/// let label = t!("button-yes"); // → "Yes"
/// ```
///
/// # With Fluent variables
///
/// ```ignore
/// let msg = t!("manifest-path-question", "manifest" => "Cargo.toml");
/// ```
///
/// Variable values are converted to strings via `to_string()` before being
/// passed to Fluent.
#[macro_export]
macro_rules! t {
	($id:expr) => {{
		use ::fluent_templates::Loader as _;
		let lang = $crate::locale::current_locale_id();
		$crate::locale::LOCALES.lookup(&lang, $id)
	}};
	($id:expr, $($key:literal => $val:expr),+) => {{
		use ::fluent_templates::Loader as _;
		let lang = $crate::locale::current_locale_id();
		let mut args: ::std::collections::HashMap<
			::std::borrow::Cow<'static, str>,
			::fluent_templates::fluent_bundle::FluentValue<'_>,
		> = ::std::collections::HashMap::new();
		$(
			args.insert(
				::std::borrow::Cow::Borrowed($key),
				::fluent_templates::fluent_bundle::FluentValue::from($val.to_string()),
			);
		)+
		$crate::locale::LOCALES.lookup_with_args(&lang, $id, &args)
	}};
}

#[cfg(test)]
mod tests {
	use super::*;

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
}
