//! Changeset file generation for recording semantic version changes.
//!
//! This module handles creating changeset files with TOML frontmatter
//! that record which projects are affected and the type of change.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::git::GitWorkdir;

/// Returns `true` if `filename` looks like a changeset file name
/// (ends with `.md`, case-insensitive, and is not `README.md`).
pub fn is_changeset_filename(filename: &str) -> bool {
	let p = Path::new(filename);
	p.extension()
		.is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
		&& !p
			.file_stem()
			.is_some_and(|stem| stem.eq_ignore_ascii_case("readme"))
}

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

	/// Returns the next change type when cycling forward through options in the TUI.
	pub(crate) fn next(self) -> Self {
		match self {
			Self::Major => Self::Minor,
			Self::Minor => Self::Patch,
			Self::Patch => Self::Major,
		}
	}

	/// Returns the previous change type when cycling backward through options in the TUI.
	pub(crate) fn prev(self) -> Self {
		match self {
			Self::Major => Self::Patch,
			Self::Minor => Self::Major,
			Self::Patch => Self::Minor,
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

	/// Writes this changeset to `.cursus/{name}.md` in the git working directory.
	///
	/// Creates the `.cursus` directory if it doesn't exist. Returns the
	/// path to the written file.
	///
	/// # Errors
	///
	/// Returns an error if the directory cannot be created or the file cannot be written.
	pub(crate) fn write(&self, git: &GitWorkdir) -> anyhow::Result<PathBuf> {
		let cursus_dir = git.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir)
			.with_context(|| format!("Failed to create directory: {}", cursus_dir.display()))?;

		let filename = Self::generate_filename();
		let path = cursus_dir.join(filename);
		let content = self.format()?;
		std::fs::write(&path, &content)
			.with_context(|| format!("Failed to write changeset: {}", path.display()))?;
		Ok(path)
	}

	/// Reads all changeset files from the `.cursus/` directory.
	///
	/// Returns a list of `(path, changeset)` pairs for each `.md` file found.
	/// `README.md` (case-insensitive) is silently skipped.
	/// Returns an empty vec if no changesets exist.
	///
	/// # Errors
	///
	/// Returns an error if any changeset file cannot be read or parsed.
	pub(crate) fn read_all(git: &GitWorkdir) -> anyhow::Result<Vec<(PathBuf, Self)>> {
		let cursus_dir = git.path().join(".cursus");
		if !cursus_dir.is_dir() {
			return Ok(Vec::new());
		}

		let pattern = cursus_dir
			.join("*.md")
			.to_str()
			.context("Invalid UTF-8 in .cursus path")?
			.to_string();

		glob::glob(&pattern)
			.context("Invalid glob pattern")?
			.filter_map(|entry| {
				let path = match entry.context("Failed to read glob entry") {
					Ok(p) => p,
					Err(e) => return Some(Err(e)),
				};
				let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
				if !is_changeset_filename(filename) {
					return None;
				}
				let contents = match std::fs::read_to_string(&path)
					.with_context(|| format!("Failed to read changeset: {}", path.display()))
				{
					Ok(c) => c,
					Err(e) => return Some(Err(e)),
				};
				let changeset = match Self::parse(&contents)
					.with_context(|| format!("Failed to parse changeset: {}", path.display()))
				{
					Ok(c) => c,
					Err(e) => return Some(Err(e)),
				};
				Some(Ok((path, changeset)))
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

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::git::GitWorkdir;
	use crate::path::AbsolutePath;

	use super::*;

	// --- is_changeset_filename ---

	#[test]
	fn is_changeset_filename_accepts_md_files() {
		assert!(is_changeset_filename("evidently-uptown-primate.md"));
		assert!(is_changeset_filename("my-change.md"));
		assert!(is_changeset_filename("a.md"));
	}

	#[test]
	fn is_changeset_filename_rejects_readme_md() {
		assert!(!is_changeset_filename("README.md"));
	}

	#[test]
	fn is_changeset_filename_rejects_readme_case_variants() {
		assert!(!is_changeset_filename("readme.md"));
		assert!(!is_changeset_filename("Readme.md"));
		assert!(!is_changeset_filename("README.MD"));
		assert!(!is_changeset_filename("ReadMe.Md"));
	}

	#[test]
	fn is_changeset_filename_rejects_non_md_files() {
		assert!(!is_changeset_filename("config.toml"));
		assert!(!is_changeset_filename("changeset.txt"));
		assert!(!is_changeset_filename("no_extension"));
	}

	#[test]
	fn is_changeset_filename_accepts_uppercase_md_extension() {
		assert!(is_changeset_filename("my-change.MD"));
	}

	fn make_git(dir: &tempfile::TempDir) -> (AbsolutePath, Arc<RecordingCommandRunner>) {
		let abs = AbsolutePath::new(dir.path()).unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		(abs, runner)
	}

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
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let changeset = single_package_changeset();
		let path = changeset.write(&git).unwrap();
		assert!(path.exists(), "Changeset file should exist");
		assert!(path.starts_with(dir.path().join(".cursus")));
		assert!(path.extension().is_some_and(|ext| ext == "md"));
	}

	#[test]
	fn write_changeset_creates_directory() {
		let dir = tempfile::tempdir().unwrap();
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let changeset = single_package_changeset();
		changeset.write(&git).unwrap();
		assert!(
			dir.path().join(".cursus").is_dir(),
			".cursus directory should exist"
		);
	}

	#[test]
	fn write_changeset_file_has_correct_content() {
		let dir = tempfile::tempdir().unwrap();
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let mut changeset = single_package_changeset();
		changeset.message = Some("Test message".to_string());
		let path = changeset.write(&git).unwrap();
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
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let result = Changeset::read_all(&git).unwrap();
		assert!(result.is_empty());
	}

	#[test]
	fn read_all_changesets_empty_when_no_md_files() {
		let dir = tempfile::tempdir().unwrap();
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		std::fs::write(cursus_dir.join("config.toml"), "").unwrap();
		let result = Changeset::read_all(&git).unwrap();
		assert!(result.is_empty());
	}

	#[test]
	fn read_all_changesets_single_file() {
		let dir = tempfile::tempdir().unwrap();
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		std::fs::write(
			cursus_dir.join("test.md"),
			"+++\nmy-app = \"minor\"\n+++\n\nA change\n",
		)
		.unwrap();

		let result = Changeset::read_all(&git).unwrap();
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].1.packages["my-app"], ChangeType::Minor);
		assert_eq!(result[0].1.message, Some("A change".to_string()));
	}

	#[test]
	fn read_all_changesets_multiple_files() {
		let dir = tempfile::tempdir().unwrap();
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		std::fs::write(cursus_dir.join("a.md"), "+++\napp = \"minor\"\n+++\n\n").unwrap();
		std::fs::write(cursus_dir.join("b.md"), "+++\napp = \"patch\"\n+++\n\n").unwrap();

		let result = Changeset::read_all(&git).unwrap();
		assert_eq!(result.len(), 2);
	}

	#[test]
	fn read_all_changesets_invalid_file_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		std::fs::write(cursus_dir.join("bad.md"), "not a valid changeset").unwrap();

		let result = Changeset::read_all(&git);
		assert!(result.is_err());
	}

	#[test]
	fn read_all_changesets_skips_readme() {
		let dir = tempfile::tempdir().unwrap();
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		std::fs::write(
			cursus_dir.join("README.md"),
			"# Changesets\nNot a changeset.",
		)
		.unwrap();
		std::fs::write(cursus_dir.join("valid.md"), "+++\napp = \"minor\"\n+++\n\n").unwrap();

		let result = Changeset::read_all(&git).unwrap();
		assert_eq!(result.len(), 1, "README.md should be skipped");
		assert_eq!(result[0].1.packages["app"], ChangeType::Minor);
	}

	#[test]
	fn read_all_changesets_skips_readme_case_insensitive() {
		let dir = tempfile::tempdir().unwrap();
		let (abs, runner) = make_git(&dir);
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let git = GitWorkdir::new(&env, abs.clone());
		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		std::fs::write(cursus_dir.join("readme.md"), "not a changeset").unwrap();

		let result = Changeset::read_all(&git).unwrap();
		assert!(result.is_empty(), "readme.md (lowercase) should be skipped");
	}

	// ChangeType tests
	#[test]
	fn change_type_next_cycles_forward() {
		assert_eq!(ChangeType::Major.next(), ChangeType::Minor);
		assert_eq!(ChangeType::Minor.next(), ChangeType::Patch);
		assert_eq!(ChangeType::Patch.next(), ChangeType::Major);
	}

	#[test]
	fn change_type_prev_cycles_backward() {
		assert_eq!(ChangeType::Major.prev(), ChangeType::Patch);
		assert_eq!(ChangeType::Minor.prev(), ChangeType::Major);
		assert_eq!(ChangeType::Patch.prev(), ChangeType::Minor);
	}

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
}
