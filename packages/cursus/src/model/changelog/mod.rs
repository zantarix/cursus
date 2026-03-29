//! Changelog generation and management.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context;

use crate::model::changeset::ChangeType;
use crate::path::AbsolutePath;

/// A reference to the git commit that introduced a changeset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReference {
	/// The first 7 characters of the full commit SHA.
	pub short_hash: String,
	/// The PR number, if one could be extracted from the commit subject.
	pub pr_number: Option<u64>,
}

impl CommitReference {
	/// Creates a new `CommitReference` from a full SHA and the commit subject line.
	pub fn new(full_sha: &str, subject: &str) -> Self {
		Self {
			short_hash: full_sha.chars().take(7).collect(),
			pr_number: extract_pr_number(subject),
		}
	}

	/// Formats the commit reference as a suffix string.
	///
	/// Returns ` [abc1234] via #123` when a PR number is present,
	/// or ` [abc1234]` when no PR number is known.
	pub fn format_suffix(&self) -> String {
		if let Some(pr) = self.pr_number {
			format!(" [{}] via #{}", self.short_hash, pr)
		} else {
			format!(" [{}]", self.short_hash)
		}
	}
}

/// Extracts a PR number from a git commit subject line.
///
/// Recognises two patterns:
/// - Squash-merge: subject contains `(#NNN)` (e.g. `feat: add thing (#42)`)
/// - Merge commit: subject starts with `Merge pull request #NNN`
fn extract_pr_number(subject: &str) -> Option<u64> {
	// Check squash-merge pattern: (#NNN) anywhere in the subject
	if let Some(pos) = subject.rfind("(#") {
		let rest = &subject[pos + 2..];
		if let Some(end) = rest.find(')') {
			let num_str = &rest[..end];
			if let Ok(n) = num_str.parse::<u64>() {
				return Some(n);
			}
		}
	}
	// Check merge-commit pattern: "Merge pull request #NNN ..."
	if let Some(rest) = subject.strip_prefix("Merge pull request #") {
		let num_str = rest.split_whitespace().next().unwrap_or("");
		if let Ok(n) = num_str.parse::<u64>() {
			return Some(n);
		}
	}
	None
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
