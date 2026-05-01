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

#[cfg(test)]
mod tests {
	use super::redact_credentials;

	#[test]
	fn no_url_is_borrowed_unchanged() {
		let s = "git add failed: no such file or directory";
		assert!(matches!(
			redact_credentials(s),
			std::borrow::Cow::Borrowed(_)
		));
		assert_eq!(redact_credentials(s), s);
	}

	#[test]
	fn url_without_credentials_is_borrowed_unchanged() {
		let s = "git push failed: https://github.com/owner/repo.git";
		assert!(matches!(
			redact_credentials(s),
			std::borrow::Cow::Borrowed(_)
		));
		assert_eq!(redact_credentials(s), s);
	}

	#[test]
	fn access_token_url_is_redacted() {
		let input = "git push failed: https://x-access-token:ghs_TOKEN@github.com/owner/repo.git";
		let want = "git push failed: https://[REDACTED]@github.com/owner/repo.git";
		assert_eq!(redact_credentials(input), want);
	}

	#[test]
	fn user_password_url_is_redacted() {
		let input = "error: https://user:secret@registry.example.com/npm/package";
		let want = "error: https://[REDACTED]@registry.example.com/npm/package";
		assert_eq!(redact_credentials(input), want);
	}

	#[test]
	fn token_only_url_is_redacted() {
		let input = "npm publish failed: https://TOKEN@registry.npmjs.org/";
		let want = "npm publish failed: https://[REDACTED]@registry.npmjs.org/";
		assert_eq!(redact_credentials(input), want);
	}

	#[test]
	fn multiple_urls_in_one_string_all_redacted() {
		let input = "https://a:b@x.com/path\nhttps://c:d@y.com/path\nhttps://z.com/no-creds";
		let want =
			"https://[REDACTED]@x.com/path\nhttps://[REDACTED]@y.com/path\nhttps://z.com/no-creds";
		assert_eq!(redact_credentials(input), want);
	}

	#[test]
	fn url_with_path_only_no_credentials_preserved() {
		let s = "clone from https://github.com/org/repo.git succeeded";
		assert_eq!(redact_credentials(s), s);
	}

	// Authority-termination: first URL has no trailing slash; second URL must
	// still be redacted independently.
	#[test]
	fn second_url_on_next_line_redacted_when_first_has_no_path() {
		let input = "auth failed: https://x-access-token:FIRST@github.com\nfallback: https://user:SECOND@registry.example.com/path";
		let want = "auth failed: https://[REDACTED]@github.com\nfallback: https://[REDACTED]@registry.example.com/path";
		assert_eq!(redact_credentials(input), want);
	}

	// RFC 3986: userinfo split on *last* '@'; password containing '@' is fully
	// redacted.
	#[test]
	fn password_containing_at_sign_is_fully_redacted() {
		let input = "https://user:p@ss@host.example.com/path";
		let want = "https://[REDACTED]@host.example.com/path";
		assert_eq!(redact_credentials(input), want);
	}

	#[test]
	fn url_with_port_redacted() {
		let input = "error: https://user:tok@host.com:8080/path";
		let want = "error: https://[REDACTED]@host.com:8080/path";
		assert_eq!(redact_credentials(input), want);
	}

	#[test]
	fn url_with_no_path_component_redacted() {
		let input = "push failed: https://x-access-token:ghs_ABC@github.com";
		let want = "push failed: https://[REDACTED]@github.com";
		assert_eq!(redact_credentials(input), want);
	}
}
