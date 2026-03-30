//! Changeset file generation for recording semantic version changes.
//!
//! This module handles creating changeset files with TOML frontmatter
//! that record which projects are affected and the type of change.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::conventional_commit;
use crate::git::Git;

/// Derives a changeset from a conventional commit message.
///
/// Returns `Ok(None)` when the commit has no semver significance (e.g. `chore:`).
/// `project_names` are the projects to include in the changeset.
///
/// # Errors
///
/// Returns an error if the commit message does not conform to the
/// Conventional Commits specification.
pub fn derive_changeset(
	commit_message: &str,
	project_names: &[&str],
) -> anyhow::Result<Option<Changeset>> {
	let commit = conventional_commit::parse(commit_message)?;
	let Some(change_type) = commit.change_type() else {
		return Ok(None);
	};
	let message = match &commit.body {
		Some(body) => format!("{}\n\n{body}", commit.description),
		None => commit.description.clone(),
	};
	let packages: BTreeMap<String, ChangeType> = project_names
		.iter()
		.map(|name| ((*name).to_string(), change_type))
		.collect();
	Ok(Some(Changeset::new(packages, Some(message))))
}

/// Filters a list of file paths to only those that are changeset files.
///
/// Strips directory prefixes and checks each filename against changeset
/// naming rules (`.md` extension, not `README.md`).
pub fn filter_changeset_paths(paths: &[String]) -> Vec<&str> {
	paths
		.iter()
		.filter(|path| {
			Path::new(path.as_str())
				.file_name()
				.and_then(|n| n.to_str())
				.is_some_and(is_changeset_filename)
		})
		.map(|s| s.as_str())
		.collect()
}

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
	pub fn generate_filename() -> String {
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
		let input: std::borrow::Cow<str> = if input.contains('\r') {
			input.replace("\r\n", "\n").into()
		} else {
			input.into()
		};
		let rest = input
			.strip_prefix("+++\n")
			.context("Missing opening +++ delimiter")?;
		let (toml_section, body) = rest
			.split_once("+++\n")
			.or_else(|| rest.strip_suffix("+++").map(|t| (t, "")))
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
	pub(crate) async fn write(
		&self,
		git: &dyn Git,
		fs: &dyn crate::filesystem::Filesystem,
	) -> anyhow::Result<PathBuf> {
		let cursus_dir = git.path().child(".cursus");
		fs.create_dir_all(&cursus_dir)
			.await
			.with_context(|| format!("Failed to create directory: {}", cursus_dir.display()))?;

		let filename = Self::generate_filename();
		let path = cursus_dir.child(filename);
		let content = self.format()?;
		fs.write(&path, content.as_bytes())
			.await
			.with_context(|| format!("Failed to write changeset: {}", path.display()))?;
		Ok(path.into_path_buf())
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
	pub(crate) async fn read_all(
		env: &crate::Env,
	) -> anyhow::Result<Vec<(crate::path::AbsolutePath, Self)>> {
		let git = env.git();
		let fs = env.fs();
		let cursus_dir = git.path().child(".cursus");
		if !fs.is_dir(&cursus_dir).await? {
			return Ok(Vec::new());
		}

		let pattern = cursus_dir
			.join("*.md")
			.to_str()
			.context("Invalid UTF-8 in .cursus path")?
			.to_string();

		let paths = fs.glob(&pattern).await?;
		let mut result = Vec::new();
		for path in paths {
			let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
			if !is_changeset_filename(filename) {
				continue;
			}
			let abs_path = crate::path::AbsolutePath::new(&path)?;
			let contents = fs
				.read_to_string(&abs_path)
				.await
				.with_context(|| format!("Failed to read changeset: {}", path.display()))?;
			let changeset = Self::parse(&contents)
				.with_context(|| format!("Failed to parse changeset: {}", path.display()))?;
			result.push((abs_path, changeset));
		}
		Ok(result)
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
	pub async fn consume(
		&self,
		path: &crate::path::AbsolutePath,
		released_packages: &BTreeSet<String>,
		fs: &dyn crate::filesystem::Filesystem,
	) -> anyhow::Result<()> {
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
			fs.remove_file(path)
				.await
				.with_context(|| format!("Failed to delete changeset: {}", path.display()))?;
		} else {
			// Partially consumed — rewrite with remaining packages only.
			let rewritten = Self::new(remaining, self.message.clone());
			let content = rewritten.format()?;
			fs.write(path, content.as_bytes())
				.await
				.with_context(|| format!("Failed to rewrite changeset: {}", path.display()))?;
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests;
