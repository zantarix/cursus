//! Changelog generation and management.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context;

use crate::model::changeset::ChangeType;
use crate::path::AbsolutePath;

/// A forge-specific reference to the pull/merge request that introduced a change.
///
/// References are self-describing: a `#123` token is inherently a GitHub pull request
/// and a `!123` token a GitLab merge request, so the variant captures both the number
/// and the forge it belongs to without consulting any configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeReference {
	/// A GitHub pull request, e.g. `#123`.
	GitHub {
		/// The pull request number.
		number: u64,
	},
	/// A GitLab merge request, e.g. `!123` or, cross-project, `group/proj!123`.
	GitLab {
		/// The project path (`group/proj`) when the reference is cross-project, else `None`.
		///
		/// Preserving the prefix is required for correctness: rendering a cross-project
		/// `group/proj!71` as a bare `!71` would resolve against the current project and
		/// link to a different merge request.
		project: Option<String>,
		/// The merge request number.
		number: u64,
	},
}

impl ForgeReference {
	/// Formats this reference as the `via …` portion of a changelog suffix.
	///
	/// GitHub renders as `#123`; GitLab renders as `!123+` (the trailing `+` makes GitLab
	/// inline the merge request title), preserving any cross-project `group/proj` prefix.
	fn format_token(&self) -> String {
		match self {
			Self::GitHub { number } => format!("#{number}"),
			Self::GitLab {
				project: Some(project),
				number,
			} => format!("{project}!{number}+"),
			Self::GitLab {
				project: None,
				number,
			} => format!("!{number}+"),
		}
	}
}

/// A reference to the git commit that introduced a changeset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReference {
	/// The first 7 characters of the full commit SHA.
	pub short_hash: String,
	/// The pull/merge request reference, if one could be extracted from the commit message.
	pub reference: Option<ForgeReference>,
}

impl CommitReference {
	/// Creates a new `CommitReference` from a full SHA and the full commit message.
	///
	/// The whole message is inspected (not just the subject) so that GitLab's
	/// `See merge request …!NN` line, which lives in the body of a default merge commit,
	/// is detected.
	pub fn new(full_sha: &str, message: &str) -> Self {
		Self {
			short_hash: full_sha.chars().take(7).collect(),
			reference: extract_reference(message),
		}
	}

	/// Returns whether a forge reference was extracted from the commit.
	pub fn has_reference(&self) -> bool {
		self.reference.is_some()
	}

	/// Formats the commit reference as a suffix string.
	///
	/// Returns ` [abc1234] via #123` (GitHub) or ` [abc1234] via !123+` (GitLab) when a
	/// reference is present, or ` [abc1234]` when none is known.
	pub fn format_suffix(&self) -> String {
		match &self.reference {
			Some(reference) => {
				format!(" [{}] via {}", self.short_hash, reference.format_token())
			}
			None => format!(" [{}]", self.short_hash),
		}
	}
}

/// Extracts a forge reference from a git commit message.
///
/// References are self-describing, so both GitHub and GitLab conventions are checked and
/// the first match wins (in this order):
/// 1. GitHub merge commit: first line starts with `Merge pull request #NNN`.
/// 2. GitLab merge commit: the message contains `See merge request [<path>]!NNN`.
/// 3. GitHub squash merge: `(#NNN)` in the first line.
/// 4. GitLab squash merge: `(!NNN)` in the first line.
///
/// The squash patterns are anchored to the first line so that `#`/`!` mentions in the
/// commit body do not produce false positives.
fn extract_reference(message: &str) -> Option<ForgeReference> {
	let first_line = message.lines().next().unwrap_or("");

	// 1. GitHub merge-commit pattern: "Merge pull request #NNN ..."
	if let Some(rest) = first_line.strip_prefix("Merge pull request #") {
		let num_str = rest.split_whitespace().next().unwrap_or("");
		if let Ok(number) = num_str.parse::<u64>() {
			return Some(ForgeReference::GitHub { number });
		}
	}

	// 2. GitLab merge-commit pattern: "See merge request [<path>]!NNN" (anywhere).
	if let Some(reference) = extract_gitlab_see_merge_request(message) {
		return Some(reference);
	}

	// 3. GitHub squash pattern: "(#NNN)" in the first line.
	if let Some(number) = extract_parenthesised_number(first_line, "(#") {
		return Some(ForgeReference::GitHub { number });
	}

	// 4. GitLab squash pattern: "(!NNN)" in the first line.
	if let Some(number) = extract_parenthesised_number(first_line, "(!") {
		return Some(ForgeReference::GitLab {
			project: None,
			number,
		});
	}

	None
}

/// Parses the number out of a `<open>NNN)` token (e.g. `(#42)` or `(!42)`) in `text`.
///
/// `open` is the opening delimiter including the sigil, e.g. `"(#"` or `"(!"`.
fn extract_parenthesised_number(text: &str, open: &str) -> Option<u64> {
	let pos = text.rfind(open)?;
	let rest = &text[pos + open.len()..];
	let end = rest.find(')')?;
	rest[..end].parse::<u64>().ok()
}

/// Extracts a GitLab merge request reference from a `See merge request …!NNN` line.
///
/// The marker is anchored to the **start of a line** (after trimming leading whitespace),
/// matching the line GitLab generates in a default merge commit. This avoids treating an
/// attacker-influenced `See merge request …` mention buried in prose (e.g. a GitHub PR body)
/// as a GitLab reference.
///
/// The text between `See merge request ` and the `!` is an optional project path
/// (`group/proj`); when present it is validated against GitLab's path grammar and carried
/// through so cross-project references link correctly. An invalid path causes the pattern
/// to be treated as unmatched rather than emitting an unsafe/wrong link.
fn extract_gitlab_see_merge_request(message: &str) -> Option<ForgeReference> {
	const MARKER: &str = "See merge request ";
	let after = message
		.lines()
		.find_map(|line| line.trim_start().strip_prefix(MARKER))?;
	// The reference token runs until the first whitespace.
	let token = after.split_whitespace().next().unwrap_or("");
	let bang = token.find('!')?;
	let project_part = &token[..bang];
	let number_part = &token[bang + 1..];

	let number = number_part.parse::<u64>().ok()?;

	let project = if project_part.is_empty() {
		None
	} else if is_valid_gitlab_project_path(project_part) {
		Some(project_part.to_string())
	} else {
		// Reject anything that is not a plausible project path rather than render it.
		return None;
	};

	Some(ForgeReference::GitLab { project, number })
}

/// Returns whether `path` matches GitLab's project path grammar.
///
/// Must be non-empty, start with an alphanumeric, `_` or `.`, and otherwise contain only
/// those characters plus `-` and `/`.
fn is_valid_gitlab_project_path(path: &str) -> bool {
	let mut chars = path.chars();
	let Some(first) = chars.next() else {
		return false;
	};
	if !(first.is_ascii_alphanumeric() || first == '_' || first == '.') {
		return false;
	}
	path.chars()
		.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '/'))
}

/// A changelog entry for a specific version.
pub struct Changelog {
	version: semver::Version,
	date: String,
	changes: Vec<(ChangeType, Option<String>, Option<CommitReference>)>,
	/// Dependency update entries rendered under a `### Dependencies` section.
	dependency_entries: Vec<String>,
	project_path: AbsolutePath,
}

impl Changelog {
	/// Creates a new changelog entry.
	pub fn new(
		version: semver::Version,
		date: String,
		changes: Vec<(ChangeType, Option<String>, Option<CommitReference>)>,
		project_path: AbsolutePath,
	) -> Self {
		Self {
			version,
			date,
			changes,
			dependency_entries: Vec::new(),
			project_path,
		}
	}

	/// Attaches dependency update entries to this changelog entry (builder pattern).
	///
	/// Each entry is a human-readable string describing a dependency update, e.g.
	/// `` "`pkg-a` bumped to 2.0.0" ``. These are rendered under a `### Dependencies`
	/// section, separate from the regular change-type sections.
	pub fn with_dependency_entries(mut self, entries: Vec<String>) -> Self {
		self.dependency_entries = entries;
		self
	}

	/// Formats just the categorised change sections (### headings + bullet items),
	/// without the `## version - date` heading line.
	///
	/// Returns an empty string when no changeset has a message.
	pub fn format_sections(&self) -> String {
		let mut sections: BTreeMap<ChangeType, Vec<(&str, Option<&CommitReference>)>> =
			BTreeMap::new();
		for (ct, msg, commit_ref) in &self.changes {
			if let Some(text) = msg.as_deref() {
				sections
					.entry(*ct)
					.or_default()
					.push((text, commit_ref.as_ref()));
			}
		}

		let mut output = String::new();

		// Iterate in order: Major (Breaking Changes) first, then Minor, then Patch
		for ct in [ChangeType::Major, ChangeType::Minor, ChangeType::Patch] {
			if let Some(messages) = sections.get(&ct) {
				let heading = match ct {
					ChangeType::Major => "Breaking Changes",
					ChangeType::Minor => "Features",
					ChangeType::Patch => "Bug Fixes",
				};
				output.push_str(&format_change_section(
					heading,
					messages,
					!output.is_empty(),
				));
			}
		}

		// Dependencies section: auto-generated entries for propagated version bumps.
		if !self.dependency_entries.is_empty() {
			let dep_messages: Vec<(&str, Option<&CommitReference>)> = self
				.dependency_entries
				.iter()
				.map(|e| (e.as_str(), None))
				.collect();
			output.push_str(&format_change_section(
				"Dependencies",
				&dep_messages,
				!output.is_empty(),
			));
		}

		output
	}

	/// Formats this changelog entry as markdown.
	///
	/// Groups changeset messages by change type (Major → Breaking Changes,
	/// Minor → Features, Patch → Bug Fixes) and formats them as a markdown section
	/// under a `## version - date` heading.
	pub fn format_entry(&self) -> String {
		let sections = self.format_sections();
		if sections.is_empty() {
			format!("## {} - {}\n", self.version, self.date)
		} else {
			format!("## {} - {}\n\n{}", self.version, self.date, sections)
		}
	}

	/// Writes or prepends this changelog entry to the project's CHANGELOG.md.
	///
	/// If the CHANGELOG.md file exists, the new entry is inserted before the first
	/// second-level markdown heading (`## `), preserving any title or introductory
	/// text above it. If no such heading exists, the entry is appended to the file.
	/// If the file does not exist, a new file is created with a `# Changelog` header.
	///
	/// When `dry_run` is `true` the file is not written.
	///
	/// # Errors
	///
	/// Returns an error if the file cannot be read or written.
	pub async fn update(
		&self,
		dry_run: bool,
		fs: &dyn crate::filesystem::Filesystem,
	) -> anyhow::Result<()> {
		let changelog_path = self.project_path.child("CHANGELOG.md");
		let entry = self.format_entry();
		let content = if fs.exists(&changelog_path).await? {
			let existing = fs
				.read_to_string(&changelog_path)
				.await
				.with_context(|| format!("Failed to read {}", changelog_path.display()))?;
			let (preamble, rest) = split_at_first_h2(&existing);
			format!("{preamble}{entry}\n{rest}")
		} else {
			format!("# Changelog\n\n{entry}\n")
		};
		if !dry_run {
			fs.write(&changelog_path, content.as_bytes())
				.await
				.with_context(|| format!("Failed to write {}", changelog_path.display()))?;
		}
		Ok(())
	}
}

/// Extracts the body of a specific version's section from a CHANGELOG.md file.
///
/// Finds the `## {version}` heading (with optional ` - date` suffix) and returns
/// the lines until the next `## ` heading or end of file, with leading and
/// trailing blank lines trimmed.
///
/// Returns an empty string if the version is not found. Does not match version
/// prefixes — searching for `1.2.0` will not match `## 1.2.0-beta`.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub async fn extract_version_body(
	changelog_path: &Path,
	version: &semver::Version,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<String> {
	let abs_path = crate::path::AbsolutePath::new(changelog_path).with_context(|| {
		format!(
			"changelog path is not absolute: {}",
			changelog_path.display()
		)
	})?;
	let content = fs
		.read_to_string(&abs_path)
		.await
		.with_context(|| format!("Failed to read {}", changelog_path.display()))?;

	let version_str = version.to_string();
	let mut in_section = false;
	let mut body_lines: Vec<&str> = Vec::new();

	for line in content.lines() {
		if let Some(rest) = line.strip_prefix("## ") {
			if in_section {
				break;
			}
			// Match "1.2.0" exactly or "1.2.0 - date" (space after version prevents
			// matching prefixes like "1.2.0-beta").
			if rest == version_str || rest.starts_with(&format!("{version_str} ")) {
				in_section = true;
			}
		} else if in_section {
			body_lines.push(line);
		}
	}

	if !in_section {
		return Ok(String::new());
	}

	let start = body_lines
		.iter()
		.position(|l| !l.is_empty())
		.unwrap_or(body_lines.len());
	let end = body_lines
		.iter()
		.rposition(|l| !l.is_empty())
		.map_or(start, |i| i + 1);

	Ok(body_lines[start..end].join("\n"))
}

/// Formats a single change-type section with a heading and bullet items.
///
/// Returns a string containing the `### heading` line followed by bullet items.
/// Prepends a blank separator line when `needs_separator` is true.
///
/// Each entry is a `(message, commit_reference)` pair. When a commit reference is
/// present, its suffix is appended to the **first line** of the message so that
/// multiline entries render as:
/// ```text
/// - Added widget [abc1234] via #123
///   with additional details
/// ```
fn format_change_section(
	heading: &str,
	messages: &[(&str, Option<&CommitReference>)],
	needs_separator: bool,
) -> String {
	let mut section = String::new();
	if needs_separator {
		section.push('\n');
	}
	let _ = writeln!(section, "### {heading}\n");
	for (msg, commit_ref) in messages {
		let suffix = commit_ref.map_or_else(String::new, CommitReference::format_suffix);
		// Apply suffix to first line, then indent continuation lines.
		let text_with_suffix = if suffix.is_empty() {
			(*msg).to_string()
		} else {
			let mut lines = msg.splitn(2, '\n');
			let first = lines.next().unwrap_or("");
			let rest = lines.next().unwrap_or("");
			if rest.is_empty() {
				format!("{first}{suffix}")
			} else {
				format!("{first}{suffix}\n{rest}")
			}
		};
		let _ = writeln!(
			section,
			"- {}",
			indent_continuation_lines(&text_with_suffix)
		);
	}
	section
}

/// Indents continuation lines of a multiline string for use in a Markdown list item.
///
/// The first line is returned as-is. Subsequent non-empty lines are prefixed with
/// two spaces to align under the `- ` bullet. Blank lines are left unindented so
/// they do not produce lines of trailing whitespace.
fn indent_continuation_lines(text: &str) -> String {
	text.split('\n')
		.enumerate()
		.map(|(i, line)| {
			if i == 0 || line.is_empty() {
				line.to_string()
			} else {
				format!("  {line}")
			}
		})
		.collect::<Vec<_>>()
		.join("\n")
}

/// Splits `content` at the first second-level markdown heading (`## `).
///
/// Returns `(preamble, rest)` where `preamble` is everything up to and including
/// the newline before the first `## ` line, and `rest` starts at that `## ` line.
/// If no `## ` heading is found, returns `(content, "")`.
fn split_at_first_h2(content: &str) -> (&str, &str) {
	if content.starts_with("## ") {
		return ("", content);
	}
	if let Some(pos) = content.find("\n## ") {
		(&content[..pos + 1], &content[pos + 1..])
	} else {
		(content, "")
	}
}

#[cfg(test)]
mod tests;
