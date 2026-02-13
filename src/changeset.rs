//! Changeset file generation for recording semantic version changes.
//!
//! This module handles creating changeset files with TOML frontmatter
//! that record which projects are affected and the type of change.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::tui::change::ChangeType;

/// A changeset recording project changes and an optional description message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changeset {
	/// Map of project name to change type.
	pub packages: BTreeMap<String, ChangeType>,
	/// Optional description message for the changeset.
	pub message: Option<String>,
}

/// Generates a random filename for a changeset using petname.
///
/// Returns a filename like `evidently-uptown-primate.md`.
pub fn generate_filename() -> String {
	let name = petname::petname(3, "-").unwrap_or_else(|| "unnamed-changeset".to_string());
	format!("{name}.md")
}

/// Formats a changeset as a string with Hugo-style `+++` TOML frontmatter.
///
/// The output format is:
/// ```text
/// +++
/// my-app = "minor"
/// +++
///
/// Description message here
/// ```
pub fn format_changeset(changeset: &Changeset) -> String {
	let toml_str = toml::to_string(&changeset.packages).unwrap_or_default();
	let body = changeset.message.as_deref().unwrap_or_default();
	format!("+++\n{toml_str}+++\n\n{body}\n")
}

/// Parses a changeset from a string with Hugo-style `+++` TOML frontmatter.
///
/// Expected format:
/// ```text
/// +++
/// my-app = "minor"
/// +++
///
/// Description message here
/// ```
///
/// # Errors
///
/// Returns an error if the delimiters are missing or the TOML frontmatter is invalid.
pub fn parse_changeset(input: &str) -> anyhow::Result<Changeset> {
	let rest = input
		.strip_prefix("+++\n")
		.context("Missing opening +++ delimiter")?;
	let (toml_section, body) = rest
		.split_once("+++\n")
		.context("Missing closing +++ delimiter")?;
	let packages: BTreeMap<String, ChangeType> =
		toml::from_str(toml_section).context("Invalid TOML frontmatter")?;
	let trimmed = body.trim();
	let message = if trimmed.is_empty() {
		None
	} else {
		Some(trimmed.to_string())
	};
	Ok(Changeset { packages, message })
}

/// Writes a changeset file to `{git_root}/.chronicle/{name}.md`.
///
/// Creates the `.chronicle` directory if it doesn't exist. Returns the
/// path to the written file.
///
/// # Errors
///
/// Returns an error if the directory cannot be created or the file cannot be written.
pub fn write_changeset(git_root: &Path, changeset: &Changeset) -> anyhow::Result<PathBuf> {
	let chronicle_dir = git_root.join(".chronicle");
	std::fs::create_dir_all(&chronicle_dir)
		.with_context(|| format!("Failed to create directory: {}", chronicle_dir.display()))?;

	let filename = generate_filename();
	let path = chronicle_dir.join(filename);
	let content = format_changeset(changeset);
	std::fs::write(&path, &content)
		.with_context(|| format!("Failed to write changeset: {}", path.display()))?;
	Ok(path)
}

/// Opens the user's editor to edit the changeset file.
///
/// Uses the `EDITOR` environment variable, falling back to `nano`.
///
/// # Errors
///
/// Returns an error if the editor process cannot be spawned or exits with an error.
pub fn open_editor(path: &Path) -> anyhow::Result<()> {
	let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
	let status = std::process::Command::new(&editor)
		.arg(path)
		.status()
		.with_context(|| format!("Failed to open editor: {editor}"))?;
	if !status.success() {
		anyhow::bail!("Editor exited with status: {status}");
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn single_package_changeset() -> Changeset {
		let mut packages = BTreeMap::new();
		packages.insert("my-app".to_string(), ChangeType::Minor);
		Changeset {
			packages,
			message: None,
		}
	}

	fn multi_package_changeset() -> Changeset {
		let mut packages = BTreeMap::new();
		packages.insert("@my-org/my-app".to_string(), ChangeType::Minor);
		packages.insert("@my-org/my-lib".to_string(), ChangeType::Patch);
		Changeset {
			packages,
			message: None,
		}
	}

	#[test]
	fn generate_filename_ends_with_md() {
		let filename = generate_filename();
		assert!(
			filename.ends_with(".md"),
			"Expected .md extension, got: {filename}"
		);
	}

	#[test]
	fn generate_filename_has_exactly_two_hyphens() {
		let filename = generate_filename();
		let stem = filename.trim_end_matches(".md");
		let hyphen_count = stem.chars().filter(|&c| c == '-').count();
		assert_eq!(
			hyphen_count, 2,
			"Expected exactly 2 hyphens (3 words), got {hyphen_count} in: {stem}"
		);
	}

	#[test]
	fn generate_filename_is_not_empty() {
		let filename = generate_filename();
		let stem = filename.trim_end_matches(".md");
		assert!(!stem.is_empty(), "Filename stem should not be empty");
	}

	#[test]
	fn format_changeset_single_package() {
		let changeset = single_package_changeset();
		let output = format_changeset(&changeset);
		assert!(output.starts_with("+++\n"), "Should start with +++");
		assert!(
			output.contains("my-app = \"minor\""),
			"Should contain package entry, got: {output}"
		);
		assert!(output.contains("+++\n\n\n")); // empty body
	}

	#[test]
	fn format_changeset_multiple_packages() {
		let changeset = multi_package_changeset();
		let output = format_changeset(&changeset);
		assert!(
			output.contains("\"@my-org/my-app\" = \"minor\""),
			"Should contain @my-org/my-app, got: {output}"
		);
		assert!(
			output.contains("\"@my-org/my-lib\" = \"patch\""),
			"Should contain @my-org/my-lib, got: {output}"
		);
	}

	#[test]
	fn format_changeset_with_message() {
		let mut changeset = single_package_changeset();
		changeset.message = Some("Added a new feature".to_string());
		let output = format_changeset(&changeset);
		assert!(output.contains("Added a new feature"));
		assert!(output.ends_with("Added a new feature\n"));
	}

	#[test]
	fn format_changeset_without_message() {
		let changeset = single_package_changeset();
		let output = format_changeset(&changeset);
		let after_frontmatter = output.rsplit_once("+++").unwrap().1;
		assert_eq!(after_frontmatter.trim(), "");
	}

	#[test]
	fn format_changeset_major_type() {
		let mut packages = BTreeMap::new();
		packages.insert("pkg".to_string(), ChangeType::Major);
		let changeset = Changeset {
			packages,
			message: None,
		};
		let output = format_changeset(&changeset);
		assert!(
			output.contains("pkg = \"major\""),
			"Should contain major type, got: {output}"
		);
	}

	#[test]
	fn write_changeset_creates_file() {
		let dir = tempfile::tempdir().unwrap();
		let changeset = single_package_changeset();
		let path = write_changeset(dir.path(), &changeset).unwrap();
		assert!(path.exists(), "Changeset file should exist");
		assert!(path.starts_with(dir.path().join(".chronicle")));
		assert!(path.extension().is_some_and(|ext| ext == "md"));
	}

	#[test]
	fn write_changeset_creates_directory() {
		let dir = tempfile::tempdir().unwrap();
		let changeset = single_package_changeset();
		write_changeset(dir.path(), &changeset).unwrap();
		assert!(
			dir.path().join(".chronicle").is_dir(),
			".chronicle directory should exist"
		);
	}

	#[test]
	fn write_changeset_file_has_correct_content() {
		let dir = tempfile::tempdir().unwrap();
		let mut changeset = single_package_changeset();
		changeset.message = Some("Test message".to_string());
		let path = write_changeset(dir.path(), &changeset).unwrap();
		let content = std::fs::read_to_string(path).unwrap();
		assert!(content.starts_with("+++\n"));
		assert!(
			content.contains("my-app = \"minor\""),
			"Should contain package entry, got: {content}"
		);
		assert!(content.contains("Test message"));
	}

	#[test]
	fn parse_changeset_round_trip_without_message() {
		let changeset = single_package_changeset();
		let formatted = format_changeset(&changeset);
		let parsed = parse_changeset(&formatted).unwrap();
		assert_eq!(parsed, changeset);
	}

	#[test]
	fn parse_changeset_round_trip_with_message() {
		let mut changeset = single_package_changeset();
		changeset.message = Some("Added a new feature".to_string());
		let formatted = format_changeset(&changeset);
		let parsed = parse_changeset(&formatted).unwrap();
		assert_eq!(parsed, changeset);
	}

	#[test]
	fn parse_changeset_single_package() {
		let input = "+++\nmy-app = \"minor\"\n+++\n\n";
		let parsed = parse_changeset(input).unwrap();
		assert_eq!(parsed.packages.len(), 1);
		assert_eq!(parsed.packages["my-app"], ChangeType::Minor);
		assert_eq!(parsed.message, None);
	}

	#[test]
	fn parse_changeset_multiple_packages() {
		let input = "+++\nmy-app = \"minor\"\nmy-lib = \"patch\"\n+++\n\n";
		let parsed = parse_changeset(input).unwrap();
		assert_eq!(parsed.packages.len(), 2);
		assert_eq!(parsed.packages["my-app"], ChangeType::Minor);
		assert_eq!(parsed.packages["my-lib"], ChangeType::Patch);
	}

	#[test]
	fn parse_changeset_with_message() {
		let input = "+++\npkg = \"major\"\n+++\n\nSome description\n";
		let parsed = parse_changeset(input).unwrap();
		assert_eq!(parsed.message, Some("Some description".to_string()));
	}

	#[test]
	fn parse_changeset_empty_body_is_none() {
		let input = "+++\npkg = \"patch\"\n+++\n\n\n";
		let parsed = parse_changeset(input).unwrap();
		assert_eq!(parsed.message, None);
	}

	#[test]
	fn parse_changeset_missing_delimiters_is_error() {
		let input = "pkg = \"minor\"\n";
		assert!(parse_changeset(input).is_err());
	}

	#[test]
	fn parse_changeset_missing_closing_delimiter_is_error() {
		let input = "+++\npkg = \"minor\"\n";
		assert!(parse_changeset(input).is_err());
	}

	#[test]
	fn parse_changeset_invalid_toml_is_error() {
		let input = "+++\nnot valid toml {{{\n+++\n\n";
		assert!(parse_changeset(input).is_err());
	}

	#[test]
	fn parse_changeset_invalid_change_type_is_error() {
		let input = "+++\npkg = \"breaking\"\n+++\n\n";
		assert!(parse_changeset(input).is_err());
	}

	#[test]
	fn change_type_serializes_lowercase() {
		let mut map = BTreeMap::new();
		map.insert("a".to_string(), ChangeType::Major);
		map.insert("b".to_string(), ChangeType::Minor);
		map.insert("c".to_string(), ChangeType::Patch);
		let toml_str = toml::to_string(&map).unwrap();
		assert!(toml_str.contains("\"major\""));
		assert!(toml_str.contains("\"minor\""));
		assert!(toml_str.contains("\"patch\""));
	}
}
