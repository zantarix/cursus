//! Shell quoting utilities.
//!
//! Delegates to the [`shell_escape`] crate, which handles both Unix (POSIX
//! single-quote wrapping) and Windows (`cmd.exe` double-quote wrapping).

use std::borrow::Cow;

/// Quotes a string for safe embedding in a platform shell command.
///
/// Delegates to [`shell_escape::escape`], which selects the appropriate
/// quoting strategy automatically: POSIX single-quote wrapping on Unix (and
/// on MSYS2/Git Bash via the `MSYSTEM` env var), and `cmd.exe`-compatible
/// double-quote wrapping on Windows.
///
/// The resulting value is safe to embed in the platform shell command string
/// (i.e. the argument passed to `run_shell`): the shell treats the entire
/// quoted span as a single token regardless of spaces, glob characters, or
/// other metacharacters. Strings that contain only shell-safe characters are
/// returned unmodified (no unnecessary quoting).
pub fn shell_quote(s: &str) -> String {
	shell_escape::escape(Cow::Borrowed(s)).into_owned()
}

#[cfg(test)]
mod tests {
	// --- Unix quoting ---

	#[test]
	fn unix_quote_safe_string_needs_no_quotes() {
		let result = shell_escape::unix::escape(std::borrow::Cow::Borrowed("hello"));
		assert_eq!(result, "hello");
	}

	#[test]
	fn unix_quote_handles_spaces() {
		let result = shell_escape::unix::escape(std::borrow::Cow::Borrowed("hello world"));
		assert_eq!(result, "'hello world'");
	}

	#[test]
	fn unix_quote_escapes_single_quote() {
		let result = shell_escape::unix::escape(std::borrow::Cow::Borrowed("it's"));
		assert_eq!(result, "'it'\\''s'");
	}

	#[test]
	fn unix_quote_handles_empty_string() {
		let result = shell_escape::unix::escape(std::borrow::Cow::Borrowed(""));
		assert_eq!(result, "''");
	}

	// --- Windows quoting ---

	#[test]
	fn windows_quote_handles_spaces() {
		let result = shell_escape::windows::escape(std::borrow::Cow::Borrowed("hello world"));
		assert!(result.starts_with('"'), "should be double-quoted: {result}");
		assert!(result.ends_with('"'), "should be double-quoted: {result}");
		assert!(
			result.contains("hello world"),
			"should preserve content: {result}"
		);
	}

	#[test]
	fn windows_quote_handles_empty_string() {
		let result = shell_escape::windows::escape(std::borrow::Cow::Borrowed(""));
		assert_eq!(result, "\"\"");
	}

	// --- Public dispatch ---

	#[cfg(unix)]
	#[test]
	fn shell_quote_safe_string_on_unix() {
		assert_eq!(super::shell_quote("hello"), "hello");
	}

	#[cfg(unix)]
	#[test]
	fn shell_quote_handles_spaces_on_unix() {
		assert_eq!(super::shell_quote("hello world"), "'hello world'");
	}

	#[cfg(unix)]
	#[test]
	fn shell_quote_escapes_single_quote_on_unix() {
		assert_eq!(super::shell_quote("it's"), "'it'\\''s'");
	}

	#[cfg(windows)]
	#[test]
	fn shell_quote_handles_spaces_on_windows() {
		let result = super::shell_quote("hello world");
		assert!(result.starts_with('"'));
		assert!(result.ends_with('"'));
	}
}
