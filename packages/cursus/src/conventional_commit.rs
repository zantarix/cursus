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
pub struct ConventionalCommit {
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
	pub fn change_type(&self) -> Option<ChangeType> {
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

/// Returns `true` if `line` matches the RFC 822-style git trailer format:
///
/// - `BREAKING CHANGE: <value>` or `BREAKING-CHANGE: <value>` (multi-word tokens from the
///   Conventional Commits spec)
/// - `<token>: <value>` where token is `[a-zA-Z0-9-]+`
/// - `<token> #<value>` where token is `[a-zA-Z0-9-]+` (e.g. `Fixes #123`)
fn is_trailer_line(line: &str) -> bool {
	let line = line.trim_end();
	if line.is_empty() {
		return false;
	}
	if line.starts_with("BREAKING CHANGE: ") || line.starts_with("BREAKING-CHANGE: ") {
		return true;
	}
	let token_end = line
		.bytes()
		.position(|b| !b.is_ascii_alphanumeric() && b != b'-')
		.unwrap_or(line.len());
	if token_end == 0 {
		return false;
	}
	let after_token = &line[token_end..];
	after_token.starts_with(": ") || after_token == ":" || after_token.starts_with(" #")
}

/// Strips RFC 822-style git trailers from the tail of `rest` (everything after the commit
/// header's `\n\n`).
///
/// The trailer block is the maximal contiguous run of non-empty lines at the tail where every
/// line matches the git trailer format. Blank lines between the prose body and the trailer block
/// are trimmed. Returns `None` when the entire `rest` consists only of trailers (or is empty).
pub(crate) fn strip_trailers(rest: &str) -> Option<String> {
	let lines: Vec<&str> = rest.lines().collect();

	// Skip trailing blank lines.
	let mut end = lines.len();
	while end > 0 && lines[end - 1].trim().is_empty() {
		end -= 1;
	}
	if end == 0 {
		return None;
	}

	// Walk backwards from the tail consuming trailer lines (stop at blank or non-trailer).
	let mut trailer_start = end;
	while trailer_start > 0 {
		let line = lines[trailer_start - 1];
		if line.trim().is_empty() || !is_trailer_line(line) {
			break;
		}
		trailer_start -= 1;
	}

	// No trailers found — return the trimmed body as-is.
	if trailer_start == end {
		let s = lines[..end].join("\n").trim().to_string();
		return if s.is_empty() { None } else { Some(s) };
	}

	// All content was trailers.
	if trailer_start == 0 {
		return None;
	}

	// Prose exists before the trailer block. Trim trailing blank lines from the prose section.
	let prose_end = lines[..trailer_start]
		.iter()
		.rposition(|l| !l.trim().is_empty())
		.map(|i| i + 1)
		.unwrap_or(0);

	if prose_end == 0 {
		return None;
	}

	let prose = lines[..prose_end].join("\n").trim().to_string();
	if prose.is_empty() { None } else { Some(prose) }
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
/// RFC 822-style git trailers (e.g. `Signed-off-by:`, `Co-authored-by:`, `Fixes #123`) are
/// stripped from the tail of the body per ADR-040, so `body` contains only prose text.
///
/// # Errors
///
/// Returns an error if the commit message does not conform to the
/// Conventional Commits specification.
pub fn parse(message: &str) -> anyhow::Result<ConventionalCommit> {
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
	let body = rest.and_then(strip_trailers);

	Ok(ConventionalCommit {
		commit_type,
		scope,
		breaking: breaking_bang || breaking_footer,
		description,
		body,
	})
}
