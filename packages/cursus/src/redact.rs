//! Credential redaction for error messages.
//!
//! Strips userinfo from URLs before they appear in user-visible error strings,
//! preventing tokens like `x-access-token:ghs_...` from leaking into logs.

use std::borrow::Cow;

/// Replaces the userinfo component of any URL in `s` with `[REDACTED]`.
///
/// Matches the pattern `<scheme>://<userinfo>@<host>` and replaces `<userinfo>`
/// with the literal string `[REDACTED]`. Handles multiple URLs in a single
/// string (e.g. multi-line stderr output) and returns the original string
/// unchanged when no credentials are present.
pub fn redact_credentials(s: &str) -> Cow<'_, str> {
	if !s.contains("://") {
		return Cow::Borrowed(s);
	}

	let mut result = String::with_capacity(s.len());
	let mut pos = 0;
	let mut modified = false;

	while pos < s.len() {
		match s[pos..].find("://") {
			None => {
				result.push_str(&s[pos..]);
				break;
			}
			Some(rel) => {
				let after_scheme = pos + rel + 3;
				result.push_str(&s[pos..after_scheme]);
				pos = after_scheme;

				// Authority ends at the first URL-terminating character after
				// "://": path separator, query, fragment, or any whitespace.
				// Stopping at whitespace prevents a URL-without-trailing-slash
				// from "swallowing" subsequent lines when `find('/')` falls back
				// to `s.len()`, which would leave credentials in later URLs
				// un-redacted.
				let auth_end = s[pos..]
					.find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace())
					.map(|r| pos + r)
					.unwrap_or(s.len());
				let authority = &s[pos..auth_end];

				// RFC 3986 splits userinfo on the *last* '@' in the authority.
				if let Some(at_rel) = authority.rfind('@') {
					result.push_str("[REDACTED]@");
					result.push_str(&authority[at_rel + 1..]);
					modified = true;
				} else {
					result.push_str(authority);
				}
				pos = auth_end;
			}
		}
	}

	if modified {
		Cow::Owned(result)
	} else {
		Cow::Borrowed(s)
	}
}
