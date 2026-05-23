//! Process-environment helpers used by the cursus binary.
//!
//! The library boundary never reads environment variables directly (ADR-030);
//! these helpers live in the binary and feed the relevant values into
//! [`cursus::Env`] / forge-client constructors.

/// Returns the first non-empty value from the given environment variables,
/// or `None` if none are set or all are empty.
#[coverage(off)]
#[mutants::skip]
pub(crate) fn env_first(vars: &[&str]) -> Option<String> {
	vars.iter()
		.find_map(|name| std::env::var(name).ok().filter(|s| !s.is_empty()))
}

/// Resolves the BCP 47 locale tag for user-visible messages.
///
/// Priority order:
/// 1. `CURSUS_LOCALE` environment variable (explicit override)
/// 2. System locale via `sys_locale::get_locale()` (cross-platform)
/// 3. `"en"` fallback
#[coverage(off)]
#[mutants::skip]
pub(crate) fn detect_locale() -> String {
	env_first(&["CURSUS_LOCALE"])
		.or_else(sys_locale::get_locale)
		.unwrap_or_else(|| cursus::locale::DEFAULT_LOCALE.to_string())
}
