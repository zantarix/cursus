//! Changeset file generation for recording semantic version changes.
//!
//! This module handles creating changeset files with TOML frontmatter
//! that record which projects are affected and the type of change.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// The type of semantic version change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
	/// A breaking change that increments the major version.
	Major,
	/// A backwards-compatible feature that increments the minor version.
	Minor,
	/// A backwards-compatible bug fix that increments the patch version.
	Patch,
}

impl PartialOrd for ChangeType {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for ChangeType {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.rank().cmp(&other.rank())
	}
}

impl std::fmt::Display for ChangeType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Major => write!(f, "major"),
			Self::Minor => write!(f, "minor"),
			Self::Patch => write!(f, "patch"),
		}
	}
}

impl ChangeType {
	/// Returns a numeric rank for ordering: Patch(0) < Minor(1) < Major(2).
	fn rank(self) -> u8 {
		match self {
			Self::Patch => 0,
			Self::Minor => 1,
			Self::Major => 2,
		}
	}
}

/// A changeset recording project changes and an optional description message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changeset {
	/// Map of project name to change type.
	pub packages: BTreeMap<String, ChangeType>,
	/// Optional description message for the changeset.
	pub message: Option<String>,
}

impl Changeset {
	/// Creates a new changeset with the given packages and optional message.
	pub fn new(packages: BTreeMap<String, ChangeType>, message: Option<String>) -> Self {
		Self { packages, message }
	}

	/// Generates a random filename for a changeset using petname.
	///
	/// Returns a filename like `evidently-uptown-primate.md`.
	fn generate_filename() -> String {
		let name = petname::petname(3, "-").unwrap_or_else(|| "unnamed-changeset".to_string());
		format!("{name}.md")
	}

	/// Formats this changeset as a string with Hugo-style `+++` TOML frontmatter.
	///
	/// The output format is:
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
	/// Returns an error if the packages map cannot be serialized to TOML.
	pub fn format(&self) -> anyhow::Result<String> {
		let toml_str = toml::to_string(&self.packages)
			.context("Failed to serialize changeset packages to TOML")?;
		let body = self.message.as_deref().unwrap_or_default();
		Ok(format!("+++\n{toml_str}+++\n\n{body}\n"))
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
	pub fn parse(input: &str) -> anyhow::Result<Self> {
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
		Ok(Self { packages, message })
	}

	/// Writes this changeset to `{git_workdir}/.chronicle/{name}.md`.
	///
	/// Creates the `.chronicle` directory if it doesn't exist. Returns the
	/// path to the written file.
	///
	/// # Errors
	///
	/// Returns an error if the directory cannot be created or the file cannot be written.
	pub fn write(&self, git_workdir: &Path) -> anyhow::Result<PathBuf> {
		let chronicle_dir = git_workdir.join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir)
			.with_context(|| format!("Failed to create directory: {}", chronicle_dir.display()))?;

		let filename = Self::generate_filename();
		let path = chronicle_dir.join(filename);
		let content = self.format()?;
		std::fs::write(&path, &content)
			.with_context(|| format!("Failed to write changeset: {}", path.display()))?;
		Ok(path)
	}

	/// Reads all changeset files from the `.chronicle/` directory.
	///
	/// Returns a list of `(path, changeset)` pairs for each `.md` file found.
	/// Returns an empty vec if no changesets exist.
	///
	/// # Errors
	///
	/// Returns an error if any changeset file cannot be read or parsed.
	pub fn read_all(git_workdir: &Path) -> anyhow::Result<Vec<(PathBuf, Self)>> {
		let chronicle_dir = git_workdir.join(".chronicle");
		if !chronicle_dir.is_dir() {
			return Ok(Vec::new());
		}

		let pattern = chronicle_dir
			.join("*.md")
			.to_str()
			.context("Invalid UTF-8 in .chronicle path")?
			.to_string();

		glob::glob(&pattern)
			.context("Invalid glob pattern")?
			.map(|entry| {
				let path = entry.context("Failed to read glob entry")?;
				let contents = std::fs::read_to_string(&path)
					.with_context(|| format!("Failed to read changeset: {}", path.display()))?;
				let changeset = Self::parse(&contents)
					.with_context(|| format!("Failed to parse changeset: {}", path.display()))?;
				Ok((path, changeset))
			})
			.collect()
	}

	/// Consumes released package entries from a changeset file.
	///
	/// - If all packages in the changeset were released, deletes the file.
	/// - If only some packages were released, rewrites the file with the
	///   released entries removed and the description preserved.
	/// - If no packages match (changeset is unrelated), leaves the file untouched.
	///
	/// # Errors
	///
	/// Returns an error if the file cannot be deleted or rewritten.
	pub fn consume(&self, path: &Path, released_packages: &BTreeSet<String>) -> anyhow::Result<()> {
		let remaining: BTreeMap<String, ChangeType> = self
			.packages
			.iter()
			.filter(|(name, _)| !released_packages.contains(*name))
			.map(|(name, ct)| (name.clone(), *ct))
			.collect();

		if remaining.len() == self.packages.len() {
			// No packages were released from this changeset — leave it untouched.
			return Ok(());
		}

		if remaining.is_empty() {
			// All packages consumed — delete the file.
			std::fs::remove_file(path)
				.with_context(|| format!("Failed to delete changeset: {}", path.display()))?;
		} else {
			// Partially consumed — rewrite with remaining packages only.
			let rewritten = Self::new(remaining, self.message.clone());
			let content = rewritten.format()?;
			std::fs::write(path, content)
				.with_context(|| format!("Failed to rewrite changeset: {}", path.display()))?;
		}

		Ok(())
	}
}

/// Finds a default editor by checking for `nano`, `vim`, then `vi` on the system PATH.
fn find_default_editor() -> Option<String> {
	["nano", "vim", "vi"]
		.into_iter()
		.find(|cmd| {
			std::process::Command::new("which")
				.arg(cmd)
				.stdout(std::process::Stdio::null())
				.stderr(std::process::Stdio::null())
				.status()
				.is_ok_and(|s| s.success())
		})
		.map(String::from)
}

/// Opens the user's editor to edit the changeset file.
///
/// Resolves the editor from `env.visual` first, then `env.editor`,
/// falling back to the first available editor from `nano`, `vim`, `vi`.
///
/// # Errors
///
/// Returns an error if no editor is found or the editor process fails.
pub fn open_editor(path: &Path, env: &crate::Env) -> anyhow::Result<()> {
	let editor = env
		.visual
		.as_deref()
		.filter(|v| !v.is_empty())
		.or_else(|| env.editor.as_deref().filter(|v| !v.is_empty()))
		.map(String::from)
		.or_else(find_default_editor)
		.context("No editor found. Set the VISUAL or EDITOR environment variable.")?;
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
		Changeset::new(packages, None)
	}

	fn multi_package_changeset() -> Changeset {
		let mut packages = BTreeMap::new();
		packages.insert("@my-org/my-app".to_string(), ChangeType::Minor);
		packages.insert("@my-org/my-lib".to_string(), ChangeType::Patch);
		Changeset::new(packages, None)
	}

	#[test]
	fn generate_filename_ends_with_md() {
		let filename = Changeset::generate_filename();
		assert!(
			filename.ends_with(".md"),
			"Expected .md extension, got: {filename}"
		);
	}

	#[test]
	fn generate_filename_has_exactly_two_hyphens() {
		let filename = Changeset::generate_filename();
		let stem = filename.trim_end_matches(".md");
		let hyphen_count = stem.chars().filter(|&c| c == '-').count();
		assert_eq!(
			hyphen_count, 2,
			"Expected exactly 2 hyphens (3 words), got {hyphen_count} in: {stem}"
		);
	}

	#[test]
	fn generate_filename_is_not_empty() {
		let filename = Changeset::generate_filename();
		let stem = filename.trim_end_matches(".md");
		assert!(!stem.is_empty(), "Filename stem should not be empty");
	}

	#[test]
	fn format_changeset_single_package() {
		let changeset = single_package_changeset();
		let output = changeset.format().unwrap();
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
		let output = changeset.format().unwrap();
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
		let output = changeset.format().unwrap();
		assert!(output.contains("Added a new feature"));
		assert!(output.ends_with("Added a new feature\n"));
	}

	#[test]
	fn format_changeset_without_message() {
		let changeset = single_package_changeset();
		let output = changeset.format().unwrap();
		let after_frontmatter = output.rsplit_once("+++").unwrap().1;
		assert_eq!(after_frontmatter.trim(), "");
	}

	#[test]
	fn format_changeset_major_type() {
		let mut packages = BTreeMap::new();
		packages.insert("pkg".to_string(), ChangeType::Major);
		let changeset = Changeset::new(packages, None);
		let output = changeset.format().unwrap();
		assert!(
			output.contains("pkg = \"major\""),
			"Should contain major type, got: {output}"
		);
	}

	#[test]
	fn write_changeset_creates_file() {
		let dir = tempfile::tempdir().unwrap();
		let changeset = single_package_changeset();
		let path = changeset.write(dir.path()).unwrap();
		assert!(path.exists(), "Changeset file should exist");
		assert!(path.starts_with(dir.path().join(".chronicle")));
		assert!(path.extension().is_some_and(|ext| ext == "md"));
	}

	#[test]
	fn write_changeset_creates_directory() {
		let dir = tempfile::tempdir().unwrap();
		let changeset = single_package_changeset();
		changeset.write(dir.path()).unwrap();
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
		let path = changeset.write(dir.path()).unwrap();
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
		let formatted = changeset.format().unwrap();
		let parsed = Changeset::parse(&formatted).unwrap();
		assert_eq!(parsed, changeset);
	}

	#[test]
	fn parse_changeset_round_trip_with_message() {
		let mut changeset = single_package_changeset();
		changeset.message = Some("Added a new feature".to_string());
		let formatted = changeset.format().unwrap();
		let parsed = Changeset::parse(&formatted).unwrap();
		assert_eq!(parsed, changeset);
	}

	#[test]
	fn parse_changeset_single_package() {
		let input = "+++\nmy-app = \"minor\"\n+++\n\n";
		let parsed = Changeset::parse(input).unwrap();
		assert_eq!(parsed.packages.len(), 1);
		assert_eq!(parsed.packages["my-app"], ChangeType::Minor);
		assert_eq!(parsed.message, None);
	}

	#[test]
	fn parse_changeset_multiple_packages() {
		let input = "+++\nmy-app = \"minor\"\nmy-lib = \"patch\"\n+++\n\n";
		let parsed = Changeset::parse(input).unwrap();
		assert_eq!(parsed.packages.len(), 2);
		assert_eq!(parsed.packages["my-app"], ChangeType::Minor);
		assert_eq!(parsed.packages["my-lib"], ChangeType::Patch);
	}

	#[test]
	fn parse_changeset_with_message() {
		let input = "+++\npkg = \"major\"\n+++\n\nSome description\n";
		let parsed = Changeset::parse(input).unwrap();
		assert_eq!(parsed.message, Some("Some description".to_string()));
	}

	#[test]
	fn parse_changeset_empty_body_is_none() {
		let input = "+++\npkg = \"patch\"\n+++\n\n\n";
		let parsed = Changeset::parse(input).unwrap();
		assert_eq!(parsed.message, None);
	}

	#[test]
	fn parse_changeset_missing_delimiters_is_error() {
		let input = "pkg = \"minor\"\n";
		assert!(Changeset::parse(input).is_err());
	}

	#[test]
	fn parse_changeset_missing_closing_delimiter_is_error() {
		let input = "+++\npkg = \"minor\"\n";
		assert!(Changeset::parse(input).is_err());
	}

	#[test]
	fn parse_changeset_invalid_toml_is_error() {
		let input = "+++\nnot valid toml {{{\n+++\n\n";
		assert!(Changeset::parse(input).is_err());
	}

	#[test]
	fn parse_changeset_invalid_change_type_is_error() {
		let input = "+++\npkg = \"breaking\"\n+++\n\n";
		assert!(Changeset::parse(input).is_err());
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

	#[test]
	fn read_all_changesets_empty_when_no_directory() {
		let dir = tempfile::tempdir().unwrap();
		let result = Changeset::read_all(dir.path()).unwrap();
		assert!(result.is_empty());
	}

	#[test]
	fn read_all_changesets_empty_when_no_md_files() {
		let dir = tempfile::tempdir().unwrap();
		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		std::fs::write(chronicle_dir.join("config.toml"), "").unwrap();
		let result = Changeset::read_all(dir.path()).unwrap();
		assert!(result.is_empty());
	}

	#[test]
	fn read_all_changesets_single_file() {
		let dir = tempfile::tempdir().unwrap();
		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		std::fs::write(
			chronicle_dir.join("test.md"),
			"+++\nmy-app = \"minor\"\n+++\n\nA change\n",
		)
		.unwrap();

		let result = Changeset::read_all(dir.path()).unwrap();
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].1.packages["my-app"], ChangeType::Minor);
		assert_eq!(result[0].1.message, Some("A change".to_string()));
	}

	#[test]
	fn read_all_changesets_multiple_files() {
		let dir = tempfile::tempdir().unwrap();
		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		std::fs::write(chronicle_dir.join("a.md"), "+++\napp = \"minor\"\n+++\n\n").unwrap();
		std::fs::write(chronicle_dir.join("b.md"), "+++\napp = \"patch\"\n+++\n\n").unwrap();

		let result = Changeset::read_all(dir.path()).unwrap();
		assert_eq!(result.len(), 2);
	}

	#[test]
	fn read_all_changesets_invalid_file_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		std::fs::write(chronicle_dir.join("bad.md"), "not a valid changeset").unwrap();

		let result = Changeset::read_all(dir.path());
		assert!(result.is_err());
	}

	// ChangeType tests
	#[test]
	fn change_type_ordering() {
		assert!(ChangeType::Major > ChangeType::Minor);
		assert!(ChangeType::Minor > ChangeType::Patch);
		assert!(ChangeType::Major > ChangeType::Patch);
		assert!(ChangeType::Patch < ChangeType::Minor);
		assert!(ChangeType::Minor < ChangeType::Major);
		assert_eq!(
			ChangeType::Major.cmp(&ChangeType::Major),
			std::cmp::Ordering::Equal
		);
	}

	#[test]
	fn change_type_display_major() {
		assert_eq!(format!("{}", ChangeType::Major), "major");
	}

	#[test]
	fn change_type_display_minor() {
		assert_eq!(format!("{}", ChangeType::Minor), "minor");
	}

	#[test]
	fn change_type_display_patch() {
		assert_eq!(format!("{}", ChangeType::Patch), "patch");
	}

	#[test]
	fn partial_ord_for_change_type() {
		assert_eq!(
			ChangeType::Major.partial_cmp(&ChangeType::Minor),
			Some(std::cmp::Ordering::Greater)
		);
		assert_eq!(
			ChangeType::Minor.partial_cmp(&ChangeType::Patch),
			Some(std::cmp::Ordering::Greater)
		);
		assert_eq!(
			ChangeType::Patch.partial_cmp(&ChangeType::Major),
			Some(std::cmp::Ordering::Less)
		);
	}

	#[test]
	fn change_type_rank_values() {
		assert_eq!(ChangeType::Patch.rank(), 0);
		assert_eq!(ChangeType::Minor.rank(), 1);
		assert_eq!(ChangeType::Major.rank(), 2);
	}

	// consume tests

	fn make_path_and_changeset(
		dir: &std::path::Path,
		filename: &str,
		content: &str,
	) -> (std::path::PathBuf, Changeset) {
		let path = dir.join(filename);
		std::fs::write(&path, content).unwrap();
		let changeset = Changeset::parse(content).unwrap();
		(path, changeset)
	}

	#[test]
	fn consume_changeset_fully_consumed_deletes_file() {
		let dir = tempfile::tempdir().unwrap();
		let (path, cs) = make_path_and_changeset(
			dir.path(),
			"change.md",
			"+++\npkg-a = \"patch\"\n+++\n\nSome message\n",
		);
		let released: BTreeSet<String> = ["pkg-a".to_string()].into();
		cs.consume(&path, &released).unwrap();
		assert!(!path.exists(), "File should be deleted when fully consumed");
	}

	#[test]
	fn consume_changeset_partially_consumed_rewrites_file() {
		let dir = tempfile::tempdir().unwrap();
		let (path, cs) = make_path_and_changeset(
			dir.path(),
			"change.md",
			"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome message\n",
		);
		let released: BTreeSet<String> = ["pkg-a".to_string()].into();
		cs.consume(&path, &released).unwrap();

		assert!(
			path.exists(),
			"File should still exist when partially consumed"
		);
		let content = std::fs::read_to_string(&path).unwrap();
		assert!(
			content.contains("pkg-b = \"minor\""),
			"Remaining package should be present, got: {content}"
		);
		assert!(
			!content.contains("pkg-a"),
			"Released package should be removed, got: {content}"
		);
		assert!(
			content.contains("Some message"),
			"Message should be preserved, got: {content}"
		);
	}

	#[test]
	fn consume_changeset_unrelated_leaves_file_untouched() {
		let dir = tempfile::tempdir().unwrap();
		let original = "+++\npkg-b = \"minor\"\n+++\n\nUnrelated change\n";
		let (path, cs) = make_path_and_changeset(dir.path(), "change.md", original);
		let released: BTreeSet<String> = ["pkg-a".to_string()].into();
		cs.consume(&path, &released).unwrap();

		assert!(path.exists(), "File should be untouched");
		let content = std::fs::read_to_string(&path).unwrap();
		assert_eq!(content, original, "File contents should be unchanged");
	}

	#[test]
	fn consume_changeset_partial_rewrite_round_trips() {
		let dir = tempfile::tempdir().unwrap();
		let (path, cs) = make_path_and_changeset(
			dir.path(),
			"change.md",
			"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\npkg-c = \"major\"\n+++\n\nMulti-package change\n",
		);
		let released: BTreeSet<String> = ["pkg-a".to_string(), "pkg-c".to_string()].into();
		cs.consume(&path, &released).unwrap();

		let content = std::fs::read_to_string(&path).unwrap();
		let reparsed = Changeset::parse(&content).unwrap();
		assert_eq!(reparsed.packages.len(), 1);
		assert_eq!(reparsed.packages["pkg-b"], ChangeType::Minor);
		assert_eq!(reparsed.message, Some("Multi-package change".to_string()));
	}

	#[test]
	fn consume_changeset_delete_fails_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("nonexistent.md");
		let mut packages = BTreeMap::new();
		packages.insert("pkg-a".to_string(), ChangeType::Patch);
		let cs = Changeset::new(packages, None);
		let released: BTreeSet<String> = ["pkg-a".to_string()].into();
		// File doesn't exist, so remove_file should fail
		let result = cs.consume(&path, &released);
		assert!(result.is_err(), "Should fail when file cannot be deleted");
	}

	#[test]
	fn consume_changeset_rewrite_fails_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		// Path inside a non-existent subdirectory — fs::write will fail.
		let path = dir.path().join("no-such-dir/change.md");
		let mut packages = BTreeMap::new();
		packages.insert("pkg-a".to_string(), ChangeType::Patch);
		packages.insert("pkg-b".to_string(), ChangeType::Minor);
		let cs = Changeset::new(packages, None);
		let released: BTreeSet<String> = ["pkg-a".to_string()].into();
		// Partially consumed → rewrite branch triggered, but parent dir missing.
		let result = cs.consume(&path, &released);
		assert!(result.is_err(), "Should fail when file cannot be rewritten");
	}

	// open_editor tests
	fn make_env(visual: Option<&str>, editor: Option<&str>) -> crate::Env {
		crate::Env {
			visual: visual.map(String::from),
			editor: editor.map(String::from),
		}
	}

	#[test]
	fn open_editor_visual_takes_priority_over_editor() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("changeset.md");
		std::fs::write(&path, "").unwrap();

		// "true" exits 0; "false" exits 1 — VISUAL must win.
		let result = open_editor(&path, &make_env(Some("true"), Some("false")));
		assert!(
			result.is_ok(),
			"Expected success when VISUAL='true', got: {result:?}"
		);
	}

	#[test]
	fn open_editor_falls_back_to_editor_when_visual_empty() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("changeset.md");
		std::fs::write(&path, "").unwrap();

		// VISUAL is empty string (filtered out), EDITOR = "true"
		let result = open_editor(&path, &make_env(Some(""), Some("true")));
		assert!(
			result.is_ok(),
			"Expected success when EDITOR='true', got: {result:?}"
		);
	}

	#[test]
	fn open_editor_editor_used_when_visual_absent() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("changeset.md");
		std::fs::write(&path, "").unwrap();

		let result = open_editor(&path, &make_env(None, Some("true")));
		assert!(
			result.is_ok(),
			"Expected success when EDITOR='true', got: {result:?}"
		);
	}

	#[test]
	fn open_editor_editor_exits_nonzero_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("changeset.md");
		std::fs::write(&path, "").unwrap();

		// "false" is a standard POSIX command that always exits 1.
		let result = open_editor(&path, &make_env(Some("false"), None));
		assert!(result.is_err(), "Expected error when editor exits non-zero");
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("Editor exited with status"),
			"Error should mention exit status, got: {msg}"
		);
	}

	#[test]
	fn open_editor_nonexistent_editor_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("changeset.md");
		std::fs::write(&path, "").unwrap();

		let result = open_editor(&path, &make_env(Some("__chronicle_no_such_editor__"), None));
		assert!(result.is_err(), "Expected error for nonexistent editor");
		let msg = result.unwrap_err().to_string();
		assert!(
			msg.contains("Failed to open editor"),
			"Error should mention failed to open editor, got: {msg}"
		);
	}
}
