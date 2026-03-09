//! Changelog generation and management.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::model::changeset::ChangeType;

/// A changelog entry for a specific version.
pub struct Changelog {
	version: semver::Version,
	date: String,
	changes: Vec<(ChangeType, Option<String>)>,
	project_path: PathBuf,
}

impl Changelog {
	/// Creates a new changelog entry.
	pub fn new(
		version: semver::Version,
		date: String,
		changes: Vec<(ChangeType, Option<String>)>,
		project_path: PathBuf,
	) -> Self {
		Self {
			version,
			date,
			changes,
			project_path,
		}
	}

	/// Formats this changelog entry as markdown.
	///
	/// Groups changeset messages by change type (Major → Breaking Changes,
	/// Minor → Features, Patch → Bug Fixes) and formats them as a markdown section.
	pub fn format_entry(&self) -> String {
		let mut sections: BTreeMap<ChangeType, Vec<&str>> = BTreeMap::new();
		for (ct, msg) in &self.changes {
			if let Some(text) = msg.as_deref() {
				sections.entry(*ct).or_default().push(text);
			}
		}

		let mut output = format!("## {} - {}\n", self.version, self.date);

		// Iterate in reverse order (Major first, then Minor, then Patch)
		for ct in [ChangeType::Major, ChangeType::Minor, ChangeType::Patch] {
			if let Some(messages) = sections.get(&ct) {
				let heading = match ct {
					ChangeType::Major => "Breaking Changes",
					ChangeType::Minor => "Features",
					ChangeType::Patch => "Bug Fixes",
				};
				let _ = writeln!(output, "\n### {heading}\n");
				for msg in messages {
					let _ = writeln!(output, "- {}", indent_continuation_lines(msg));
				}
			}
		}

		output
	}

	/// Writes or prepends this changelog entry to the project's CHANGELOG.md.
	///
	/// If the CHANGELOG.md file exists, the new entry is inserted before the first
	/// second-level markdown heading (`## `), preserving any title or introductory
	/// text above it. If no such heading exists, the entry is appended to the file.
	/// If the file does not exist, a new file is created with a `# Changelog` header.
	///
	/// # Errors
	///
	/// Returns an error if the file cannot be read or written.
	pub fn update(&self, git_workdir: &Path) -> anyhow::Result<()> {
		let changelog_path = git_workdir.join(&self.project_path).join("CHANGELOG.md");
		let entry = self.format_entry();
		let content = if changelog_path.exists() {
			let existing = std::fs::read_to_string(&changelog_path)
				.with_context(|| format!("Failed to read {}", changelog_path.display()))?;
			let (preamble, rest) = split_at_first_h2(&existing);
			format!("{preamble}{entry}\n{rest}")
		} else {
			format!("# Changelog\n\n{entry}\n")
		};
		std::fs::write(&changelog_path, content)
			.with_context(|| format!("Failed to write {}", changelog_path.display()))?;
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
pub fn extract_version_body(
	changelog_path: &Path,
	version: &semver::Version,
) -> anyhow::Result<String> {
	let content = std::fs::read_to_string(changelog_path)
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
mod tests {
	use super::*;

	#[test]
	fn split_at_first_h2_with_preamble() {
		let content = "# Changelog\n\nIntro paragraph.\n\n## 1.0.0\n\nOld\n";
		let (preamble, rest) = split_at_first_h2(content);
		assert_eq!(preamble, "# Changelog\n\nIntro paragraph.\n\n");
		assert_eq!(rest, "## 1.0.0\n\nOld\n");
	}

	#[test]
	fn split_at_first_h2_starts_with_h2() {
		let content = "## 1.0.0\n\nOld\n";
		let (preamble, rest) = split_at_first_h2(content);
		assert_eq!(preamble, "");
		assert_eq!(rest, "## 1.0.0\n\nOld\n");
	}

	#[test]
	fn split_at_first_h2_no_h2() {
		let content = "# Changelog\n\nNo versions yet.\n";
		let (preamble, rest) = split_at_first_h2(content);
		assert_eq!(preamble, "# Changelog\n\nNo versions yet.\n");
		assert_eq!(rest, "");
	}

	#[test]
	fn split_at_first_h2_empty() {
		let (preamble, rest) = split_at_first_h2("");
		assert_eq!(preamble, "");
		assert_eq!(rest, "");
	}

	#[test]
	fn update_changelog_preserves_custom_preamble() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("CHANGELOG.md"),
			"# My Custom Title\n\nAn intro paragraph.\n\n## 0.1.0\n\nOld entry\n",
		)
		.unwrap();
		let changes = vec![(ChangeType::Minor, Some("New thing".to_string()))];
		let changelog = Changelog::new(
			"0.2.0".parse().unwrap(),
			"2024-06-01".to_string(),
			changes,
			PathBuf::new(),
		);
		changelog.update(dir.path()).unwrap();

		let content = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
		insta::assert_snapshot!(content);
	}

	#[test]
	fn format_changelog_entry_with_messages() {
		let changes = vec![
			(ChangeType::Minor, Some("Added feature X".to_string())),
			(ChangeType::Patch, Some("Fixed bug Y".to_string())),
		];
		let changelog = Changelog::new(
			"1.1.0".parse().unwrap(),
			"2024-01-15".to_string(),
			changes,
			PathBuf::new(),
		);
		let entry = changelog.format_entry();
		assert!(entry.contains("## 1.1.0 - 2024-01-15"));
		assert!(entry.contains("### Features"));
		assert!(entry.contains("- Added feature X"));
		assert!(entry.contains("### Bug Fixes"));
		assert!(entry.contains("- Fixed bug Y"));
	}

	#[test]
	fn format_changelog_entry_no_messages() {
		let changes: Vec<(ChangeType, Option<String>)> = vec![(ChangeType::Minor, None)];
		let changelog = Changelog::new(
			"1.1.0".parse().unwrap(),
			"2024-01-15".to_string(),
			changes,
			PathBuf::new(),
		);
		let entry = changelog.format_entry();
		assert!(entry.contains("## 1.1.0 - 2024-01-15"));
		assert!(!entry.contains("###"));
	}

	#[test]
	fn format_changelog_entry_multiline_message() {
		let changes = vec![(
			ChangeType::Minor,
			Some("First line\nSecond line\nThird line".to_string()),
		)];
		let changelog = Changelog::new(
			"1.1.0".parse().unwrap(),
			"2024-01-15".to_string(),
			changes,
			PathBuf::new(),
		);
		let entry = changelog.format_entry();
		// Continuation lines must be indented so the list item renders correctly
		assert!(entry.contains("- First line\n  Second line\n  Third line"));
	}

	#[test]
	fn format_changelog_entry_multiline_message_blank_lines_not_indented() {
		let changes = vec![(
			ChangeType::Minor,
			Some("First line\n\nSecond paragraph".to_string()),
		)];
		let changelog = Changelog::new(
			"1.1.0".parse().unwrap(),
			"2024-01-15".to_string(),
			changes,
			PathBuf::new(),
		);
		let entry = changelog.format_entry();
		// Blank lines must not be indented
		assert!(entry.contains("- First line\n\n  Second paragraph"));
	}

	#[test]
	fn format_changelog_entry_major_section() {
		let changes = vec![(ChangeType::Major, Some("Breaking API change".to_string()))];
		let changelog = Changelog::new(
			"2.0.0".parse().unwrap(),
			"2024-01-15".to_string(),
			changes,
			PathBuf::new(),
		);
		let entry = changelog.format_entry();
		assert!(entry.contains("### Breaking Changes"));
		assert!(entry.contains("- Breaking API change"));
	}

	#[test]
	fn update_changelog_creates_new_file() {
		let dir = tempfile::tempdir().unwrap();
		let changes = vec![(ChangeType::Minor, Some("Something new".to_string()))];
		let changelog = Changelog::new(
			"1.0.0".parse().unwrap(),
			"2024-01-15".to_string(),
			changes,
			PathBuf::new(),
		);
		changelog.update(dir.path()).unwrap();

		let content = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
		assert!(content.contains("# Changelog"));
		assert!(content.contains("## 1.0.0 - 2024-01-15"));
	}

	#[test]
	fn update_changelog_prepends_to_existing() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("CHANGELOG.md"),
			"# Changelog\n\n## 0.1.0\n\nOld entry\n",
		)
		.unwrap();
		let changes = vec![(ChangeType::Minor, Some("New thing".to_string()))];
		let changelog = Changelog::new(
			"0.2.0".parse().unwrap(),
			"2024-06-01".to_string(),
			changes,
			PathBuf::new(),
		);
		changelog.update(dir.path()).unwrap();

		let content = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
		assert!(content.contains("## 0.2.0 - 2024-06-01"));
		assert!(content.contains("## 0.1.0"));
		// New entry should come first
		let pos_new = content.find("## 0.2.0").unwrap();
		let pos_old = content.find("## 0.1.0").unwrap();
		assert!(pos_new < pos_old);
		// Header must appear exactly once
		assert_eq!(content.matches("# Changelog").count(), 1);
	}

	#[test]
	fn update_changelog_successive_releases_snapshot() {
		let dir = tempfile::tempdir().unwrap();

		let make = |version: &str, msg: &str| {
			Changelog::new(
				version.parse().unwrap(),
				"2024-01-01".to_string(),
				vec![(ChangeType::Patch, Some(msg.to_string()))],
				PathBuf::new(),
			)
		};

		make("1.0.0", "Initial release").update(dir.path()).unwrap();
		make("1.0.1", "Second release").update(dir.path()).unwrap();
		make("1.0.2", "Third release").update(dir.path()).unwrap();

		let content = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
		insta::assert_snapshot!(content);
	}

	#[test]
	fn update_changelog_no_duplicate_header_on_successive_releases() {
		let dir = tempfile::tempdir().unwrap();

		let make = |version: &str, msg: &str| {
			Changelog::new(
				version.parse().unwrap(),
				"2024-01-01".to_string(),
				vec![(ChangeType::Patch, Some(msg.to_string()))],
				PathBuf::new(),
			)
		};

		make("1.0.0", "Initial release").update(dir.path()).unwrap();
		make("1.0.1", "Second release").update(dir.path()).unwrap();
		make("1.0.2", "Third release").update(dir.path()).unwrap();

		let content = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
		assert_eq!(content.matches("# Changelog").count(), 1);
		// All three versions present
		assert!(content.contains("## 1.0.2"));
		assert!(content.contains("## 1.0.1"));
		assert!(content.contains("## 1.0.0"));
		// Newest first
		let p2 = content.find("## 1.0.2").unwrap();
		let p1 = content.find("## 1.0.1").unwrap();
		let p0 = content.find("## 1.0.0").unwrap();
		assert!(p2 < p1 && p1 < p0);
	}

	#[test]
	fn update_changelog_in_subdir() {
		let dir = tempfile::tempdir().unwrap();
		let sub = dir.path().join("packages/my-pkg");
		std::fs::create_dir_all(&sub).unwrap();
		let changes = vec![(ChangeType::Patch, Some("Release".to_string()))];
		let changelog = Changelog::new(
			"1.0.0".parse().unwrap(),
			"2024-01-15".to_string(),
			changes,
			PathBuf::from("packages/my-pkg"),
		);
		changelog.update(dir.path()).unwrap();

		let content = std::fs::read_to_string(sub.join("CHANGELOG.md")).unwrap();
		assert!(content.contains("## 1.0.0 - 2024-01-15"));
	}

	#[test]
	fn update_changelog_fails_when_cannot_read_existing() {
		let dir = tempfile::tempdir().unwrap();
		let changelog_path = dir.path().join("CHANGELOG.md");
		// Create a directory with the same name as the file we want to read
		std::fs::create_dir(&changelog_path).unwrap();

		let changes = vec![(ChangeType::Minor, Some("New".to_string()))];
		let changelog = Changelog::new(
			"1.0.0".parse().unwrap(),
			"2024-01-15".to_string(),
			changes,
			PathBuf::new(),
		);
		let result = changelog.update(dir.path());

		// Should fail because CHANGELOG.md is a directory, not a file
		assert!(result.is_err());
	}

	#[test]
	fn update_changelog_fails_when_cannot_write() {
		use std::os::unix::fs::PermissionsExt;
		let dir = tempfile::tempdir().unwrap();
		// Make directory read-only
		let mut perms = std::fs::metadata(dir.path()).unwrap().permissions();
		perms.set_mode(0o444);
		std::fs::set_permissions(dir.path(), perms).unwrap();

		let changes = vec![(ChangeType::Patch, Some("Fix".to_string()))];
		let changelog = Changelog::new(
			"1.0.0".parse().unwrap(),
			"2024-01-15".to_string(),
			changes,
			PathBuf::new(),
		);
		let result = changelog.update(dir.path());

		// Restore permissions before assertions for cleanup
		let mut perms = std::fs::metadata(dir.path()).unwrap().permissions();
		perms.set_mode(0o755);
		std::fs::set_permissions(dir.path(), perms).unwrap();

		// Should fail because directory is read-only
		assert!(result.is_err());
	}

	// --- extract_version_body ---

	const MULTI_VERSION_CHANGELOG: &str = "\
# Changelog

## 1.2.0 - 2024-06-01

### Features

- Added widget

## 1.1.0 - 2024-03-01

### Bug Fixes

- Fixed thing

## 1.0.0

Initial release
";

	#[test]
	fn extract_version_body_finds_middle_version() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("CHANGELOG.md");
		std::fs::write(&path, MULTI_VERSION_CHANGELOG).unwrap();

		let body = extract_version_body(&path, &"1.1.0".parse().unwrap()).unwrap();
		assert!(body.contains("### Bug Fixes"));
		assert!(body.contains("- Fixed thing"));
		assert!(!body.contains("### Features"));
	}

	#[test]
	fn extract_version_body_finds_first_version() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("CHANGELOG.md");
		std::fs::write(&path, MULTI_VERSION_CHANGELOG).unwrap();

		let body = extract_version_body(&path, &"1.2.0".parse().unwrap()).unwrap();
		assert!(body.contains("### Features"));
		assert!(body.contains("- Added widget"));
	}

	#[test]
	fn extract_version_body_finds_version_at_eof() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("CHANGELOG.md");
		std::fs::write(&path, MULTI_VERSION_CHANGELOG).unwrap();

		let body = extract_version_body(&path, &"1.0.0".parse().unwrap()).unwrap();
		assert_eq!(body.trim(), "Initial release");
	}

	#[test]
	fn extract_version_body_returns_empty_for_missing_version() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("CHANGELOG.md");
		std::fs::write(&path, MULTI_VERSION_CHANGELOG).unwrap();

		let body = extract_version_body(&path, &"9.9.9".parse().unwrap()).unwrap();
		assert!(body.is_empty());
	}

	#[test]
	fn extract_version_body_returns_error_for_missing_file() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("CHANGELOG.md");

		let result = extract_version_body(&path, &"1.0.0".parse().unwrap());
		assert!(result.is_err());
	}

	#[test]
	fn extract_version_body_does_not_match_version_prefix() {
		let changelog = "# Changelog\n\n## 1.2.0-beta - 2024-01-01\n\nbeta content\n\n## 1.2.0 - 2024-02-01\n\nstable content\n";
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("CHANGELOG.md");
		std::fs::write(&path, changelog).unwrap();

		let body = extract_version_body(&path, &"1.2.0".parse().unwrap()).unwrap();
		assert!(body.contains("stable content"));
		assert!(!body.contains("beta content"));
	}

	#[test]
	fn extract_version_body_with_date_suffix() {
		let changelog = "# Changelog\n\n## 2.0.0 - 2025-01-01\n\nMajor release\n";
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("CHANGELOG.md");
		std::fs::write(&path, changelog).unwrap();

		let body = extract_version_body(&path, &"2.0.0".parse().unwrap()).unwrap();
		assert!(body.contains("Major release"));
	}

	#[test]
	fn extract_version_body_empty_body() {
		let changelog = "# Changelog\n\n## 1.0.0\n\n## 0.9.0\n\nPrevious\n";
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("CHANGELOG.md");
		std::fs::write(&path, changelog).unwrap();

		let body = extract_version_body(&path, &"1.0.0".parse().unwrap()).unwrap();
		assert!(body.is_empty());
	}
}
