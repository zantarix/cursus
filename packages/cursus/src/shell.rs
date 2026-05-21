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
