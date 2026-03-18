//! Shell quoting utilities.
//!
//! Each function in this module has a platform-specific behaviour and should be
//! updated (or swapped for a conditional implementation) when Windows support
//! is added.

/// Wraps a string in POSIX single quotes, escaping any embedded single quotes.
///
/// The resulting value is safe to embed in a `/bin/sh -c` command string: the
/// shell will treat the entire quoted span as a single token regardless of
/// spaces, glob characters, or other metacharacters.
///
/// Existing single quotes are replaced using the `'\''` idiom: the surrounding
/// quotes are closed, the literal quote is added unquoted, then quoting resumes.
///
/// # Example
///
/// ```
/// # use cursus::shell::shell_quote;
/// assert_eq!(shell_quote("hello world"), "'hello world'");
/// assert_eq!(shell_quote("it's"), "'it'\\''s'");
/// ```
pub fn shell_quote(s: &str) -> String {
	format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn shell_quote_wraps_in_single_quotes() {
		assert_eq!(shell_quote("hello"), "'hello'");
	}

	#[test]
	fn shell_quote_handles_spaces() {
		assert_eq!(shell_quote("hello world"), "'hello world'");
	}

	#[test]
	fn shell_quote_escapes_single_quote() {
		assert_eq!(shell_quote("it's"), "'it'\\''s'");
	}

	#[test]
	fn shell_quote_escapes_multiple_single_quotes() {
		assert_eq!(shell_quote("it's a l'il"), "'it'\\''s a l'\\''il'");
	}

	#[test]
	fn shell_quote_handles_empty_string() {
		assert_eq!(shell_quote(""), "''");
	}
}
