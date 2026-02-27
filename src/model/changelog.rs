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
					let _ = writeln!(output, "- {msg}");
				}
			}
		}

		output
	}

	/// Writes or prepends this changelog entry to the project's CHANGELOG.md.
	///
	/// If the CHANGELOG.md file exists, the entry is prepended. Otherwise, a new
	/// file is created with a "# Changelog" header.
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
			format!("{entry}\n{existing}")
		} else {
			format!("# Changelog\n\n{entry}\n")
		};
		std::fs::write(&changelog_path, content)
			.with_context(|| format!("Failed to write {}", changelog_path.display()))?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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
}
