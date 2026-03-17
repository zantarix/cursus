//! Parser for Conventional Commits (https://www.conventionalcommits.org/).
//!
//! Parses commit messages of the form:
//! `<type>(<scope>)?!?: <description>`
//!
//! with an optional body and footer separated from the header by a blank line.

use anyhow::bail;

use crate::model::changeset::ChangeType;

/// A parsed Conventional Commit message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConventionalCommit {
	/// The commit type (e.g., `feat`, `fix`, `chore`).
	pub commit_type: String,
	/// The optional scope (e.g., `auth`, `api`).
	pub scope: Option<String>,
	/// Whether this is a breaking change (via `!` or `BREAKING CHANGE:` footer).
	pub breaking: bool,
	/// The short description following `: `.
	pub description: String,
	/// The optional body text (everything after the first blank line).
	pub body: Option<String>,
}

impl ConventionalCommit {
	/// Maps this commit to a semantic version [`ChangeType`], if applicable.
	///
	/// - Breaking change → [`ChangeType::Major`]
	/// - `feat` → [`ChangeType::Minor`]
	/// - `fix` → [`ChangeType::Patch`]
	/// - Anything else → `None`
	pub(crate) fn change_type(&self) -> Option<ChangeType> {
		if self.breaking {
			return Some(ChangeType::Major);
		}
		match self.commit_type.as_str() {
			"feat" => Some(ChangeType::Minor),
			"fix" => Some(ChangeType::Patch),
			_ => None,
		}
	}
}

/// Parses the header line of a Conventional Commit.
///
/// Returns `(commit_type, scope, breaking_bang, description)`.
///
/// # Errors
///
/// Returns an error if the header does not conform to the spec.
fn parse_header(header: &str) -> anyhow::Result<(String, Option<String>, bool, String)> {
	let mut iter = header.char_indices().peekable();
	let mut commit_type = String::new();
	loop {
		match iter.peek() {
			Some((_, '(')) | Some((_, '!')) | Some((_, ':')) => break,
			Some((_, c)) if c.is_alphanumeric() || *c == '-' => {
				commit_type.push(*c);
				iter.next();
			}
			Some((_, c)) => bail!("Unexpected character '{c}' in commit type in: {header}"),
			None => bail!("Unexpected end of header while parsing type in: {header}"),
		}
	}
	if commit_type.is_empty() {
		bail!("Missing commit type in: {header}");
	}

	let scope = if iter.peek().map(|(_, c)| *c) == Some('(') {
		iter.next();
		let mut scope_str = String::new();
		loop {
			match iter.next() {
				Some((_, ')')) => break,
				Some((_, c)) => scope_str.push(c),
				None => bail!("Unclosed scope parenthesis in: {header}"),
			}
		}
		Some(scope_str)
	} else {
		None
	};

	let breaking_bang = if iter.peek().map(|(_, c)| *c) == Some('!') {
		iter.next();
		true
	} else {
		false
	};

	let remaining: String = iter.map(|(_, c)| c).collect();
	let description = remaining
		.strip_prefix(": ")
		.ok_or_else(|| anyhow::anyhow!("Missing ': ' separator in: {header}"))?
		.trim()
		.to_string();
	if description.is_empty() {
		bail!("Missing description in: {header}");
	}
	Ok((commit_type, scope, breaking_bang, description))
}

/// Parses a commit message string as a Conventional Commit.
///
/// Splits at the first blank line (`\n\n`) to separate the header from the
/// body/footer. The header is expected to match:
/// `<type>(<scope>)?!?: <description>`
///
/// Breaking changes are detected via:
/// - A `!` before the `: ` separator in the header, or
/// - A `BREAKING CHANGE:` or `BREAKING-CHANGE:` token in the footer.
///
/// # Errors
///
/// Returns an error if the commit message does not conform to the
/// Conventional Commits specification.
pub(crate) fn parse(message: &str) -> anyhow::Result<ConventionalCommit> {
	let (header, rest) = match message.split_once("\n\n") {
		Some((h, r)) => (h, Some(r)),
		None => (message, None),
	};

	let (commit_type, scope, breaking_bang, description) = parse_header(header)?;

	let breaking_footer = rest.is_some_and(|r| {
		r.lines().any(|line| {
			line.starts_with("BREAKING CHANGE:") || line.starts_with("BREAKING-CHANGE:")
		})
	});
	let body = rest.map(|r| r.trim().to_string()).filter(|s| !s.is_empty());

	Ok(ConventionalCommit {
		commit_type,
		scope,
		breaking: breaking_bang || breaking_footer,
		description,
		body,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	// --- parse ---

	#[test]
	fn parse_simple_fix() {
		let c = parse("fix: correct off-by-one error").unwrap();
		assert_eq!(c.commit_type, "fix");
		assert_eq!(c.scope, None);
		assert!(!c.breaking);
		assert_eq!(c.description, "correct off-by-one error");
		assert_eq!(c.body, None);
	}

	#[test]
	fn parse_simple_feat() {
		let c = parse("feat: add new widget").unwrap();
		assert_eq!(c.commit_type, "feat");
		assert_eq!(c.scope, None);
		assert!(!c.breaking);
		assert_eq!(c.description, "add new widget");
	}

	#[test]
	fn parse_chore_commit() {
		let c = parse("chore: update dependencies").unwrap();
		assert_eq!(c.commit_type, "chore");
		assert!(!c.breaking);
		assert_eq!(c.description, "update dependencies");
	}

	#[test]
	fn parse_with_scope() {
		let c = parse("feat(auth): add OAuth2 support").unwrap();
		assert_eq!(c.commit_type, "feat");
		assert_eq!(c.scope, Some("auth".to_string()));
		assert!(!c.breaking);
		assert_eq!(c.description, "add OAuth2 support");
	}

	#[test]
	fn parse_breaking_via_bang() {
		let c = parse("feat!: remove deprecated API").unwrap();
		assert_eq!(c.commit_type, "feat");
		assert!(c.breaking);
		assert_eq!(c.description, "remove deprecated API");
	}

	#[test]
	fn parse_breaking_with_scope_and_bang() {
		let c = parse("feat(api)!: redesign authentication").unwrap();
		assert_eq!(c.commit_type, "feat");
		assert_eq!(c.scope, Some("api".to_string()));
		assert!(c.breaking);
		assert_eq!(c.description, "redesign authentication");
	}

	#[test]
	fn parse_breaking_via_footer_breaking_change() {
		let msg = "feat: new login flow\n\nAdds support for SSO.\n\nBREAKING CHANGE: old login endpoint removed";
		let c = parse(msg).unwrap();
		assert_eq!(c.commit_type, "feat");
		assert!(c.breaking);
		assert_eq!(c.description, "new login flow");
	}

	#[test]
	fn parse_breaking_via_footer_breaking_change_hyphen() {
		let msg =
			"refactor: overhaul config\n\nSome details.\n\nBREAKING-CHANGE: config format changed";
		let c = parse(msg).unwrap();
		assert!(c.breaking);
	}

	#[test]
	fn parse_body_extracted() {
		let msg = "fix: resolve race condition\n\nThis was causing crashes under high load.\nSee issue #123.";
		let c = parse(msg).unwrap();
		assert_eq!(c.description, "resolve race condition");
		assert_eq!(
			c.body,
			Some("This was causing crashes under high load.\nSee issue #123.".to_string())
		);
	}

	#[test]
	fn parse_body_none_when_empty_after_blank_line() {
		let c = parse("fix: something\n\n   \n").unwrap();
		assert_eq!(c.body, None);
	}

	#[test]
	fn parse_no_blank_line_means_no_body() {
		let c = parse("fix: quick fix").unwrap();
		assert_eq!(c.body, None);
	}

	#[test]
	fn parse_multiline_header_folds_continuation_into_description() {
		// Git can word-wrap long subjects; the parser treats everything before
		// the first blank line as the header, so the continuation line is
		// folded into the description verbatim.
		let msg = "chore: fixed something\nbut git wrapped this line\n\nBody goes here";
		let c = parse(msg).unwrap();
		assert_eq!(c.commit_type, "chore");
		assert_eq!(c.description, "fixed something\nbut git wrapped this line");
		assert_eq!(c.body, Some("Body goes here".to_string()));
	}

	#[test]
	fn parse_single_trailing_newline_no_body() {
		// A trailing \n without a blank line never triggers the \n\n split.
		// The description is trimmed, so the trailing newline is stripped.
		let c = parse("fix: thing\n").unwrap();
		assert_eq!(c.description, "thing");
		assert_eq!(c.body, None);
	}

	#[test]
	fn parse_single_newline_between_lines_folds_into_description() {
		// Without a blank line, the second line is part of the header, not the body.
		let c = parse("fix: thing\nsecond line").unwrap();
		assert_eq!(c.description, "thing\nsecond line");
		assert_eq!(c.body, None);
	}

	#[test]
	fn parse_missing_separator_is_error() {
		assert!(parse("feat add thing").is_err());
	}

	#[test]
	fn parse_empty_description_is_error() {
		assert!(parse("fix: ").is_err());
	}

	#[test]
	fn parse_missing_type_is_error() {
		assert!(parse(": something").is_err());
	}

	#[test]
	fn parse_unclosed_scope_is_error() {
		assert!(parse("feat(auth: add something").is_err());
	}

	#[test]
	fn parse_hyphenated_type() {
		let c = parse("build-system: update toolchain").unwrap();
		assert_eq!(c.commit_type, "build-system");
	}

	#[test]
	fn parse_invalid_char_in_type_is_error() {
		assert!(parse("feat@scope: desc").is_err());
	}

	// --- change_type ---

	#[test]
	fn change_type_fix_is_patch() {
		let c = parse("fix: correct a bug").unwrap();
		assert_eq!(c.change_type(), Some(ChangeType::Patch));
	}

	#[test]
	fn change_type_feat_is_minor() {
		let c = parse("feat: new feature").unwrap();
		assert_eq!(c.change_type(), Some(ChangeType::Minor));
	}

	#[test]
	fn change_type_breaking_is_major() {
		let c = parse("fix!: breaking bugfix").unwrap();
		assert_eq!(c.change_type(), Some(ChangeType::Major));
	}

	#[test]
	fn change_type_breaking_footer_is_major() {
		let c = parse("feat: new stuff\n\nBREAKING CHANGE: old api gone").unwrap();
		assert_eq!(c.change_type(), Some(ChangeType::Major));
	}

	#[test]
	fn change_type_chore_is_none() {
		let c = parse("chore: update deps").unwrap();
		assert_eq!(c.change_type(), None);
	}

	#[test]
	fn change_type_refactor_is_none() {
		let c = parse("refactor: tidy up code").unwrap();
		assert_eq!(c.change_type(), None);
	}

	#[test]
	fn change_type_docs_is_none() {
		let c = parse("docs: update readme").unwrap();
		assert_eq!(c.change_type(), None);
	}
}
