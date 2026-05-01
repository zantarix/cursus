use super::*;

// --- extract_pr_number ---

#[test]
fn extract_pr_number_squash_merge_format() {
	assert_eq!(extract_pr_number("feat: add widget (#42)"), Some(42));
}

#[test]
fn extract_pr_number_squash_merge_at_start() {
	assert_eq!(extract_pr_number("fix: thing (#1)"), Some(1));
}

#[test]
fn extract_pr_number_merge_commit_format() {
	assert_eq!(
		extract_pr_number("Merge pull request #123 from owner/branch"),
		Some(123)
	);
}

#[test]
fn extract_pr_number_no_match_rebase() {
	assert_eq!(extract_pr_number("feat: add widget"), None);
}

#[test]
fn extract_pr_number_no_match_hash_without_parens() {
	assert_eq!(extract_pr_number("fix: issue #99 workaround"), None);
}

#[test]
fn extract_pr_number_empty_subject() {
	assert_eq!(extract_pr_number(""), None);
}

// --- CommitReference::format_suffix ---

#[test]
fn commit_reference_format_suffix_with_pr() {
	let r = CommitReference {
		short_hash: "abc1234".to_string(),
		pr_number: Some(42),
	};
	assert_eq!(r.format_suffix(), " [abc1234] via #42");
}

#[test]
fn commit_reference_format_suffix_without_pr() {
	let r = CommitReference {
		short_hash: "abc1234".to_string(),
		pr_number: None,
	};
	assert_eq!(r.format_suffix(), " [abc1234]");
}

#[test]
fn commit_reference_new_truncates_sha_to_7_chars() {
	let r = CommitReference::new("abcdef1234567890", "feat: stuff (#5)");
	assert_eq!(r.short_hash, "abcdef1");
	assert_eq!(r.pr_number, Some(5));
}

// --- format_sections with commit references ---

#[test]
fn format_sections_with_commit_reference_renders_suffix() {
	let commit_ref = CommitReference {
		short_hash: "abc1234".to_string(),
		pr_number: Some(42),
	};
	let changes = vec![(
		ChangeType::Minor,
		Some("Added widget".to_string()),
		Some(commit_ref),
	)];
	let changelog = Changelog::new(
		"1.0.0".parse().unwrap(),
		"2024-01-01".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
	);
	let sections = changelog.format_sections();
	assert!(
		sections.contains("- Added widget [abc1234] via #42"),
		"Expected suffix in output, got: {sections}"
	);
}

#[test]
fn format_sections_multiline_message_suffix_on_first_line() {
	let commit_ref = CommitReference {
		short_hash: "abc1234".to_string(),
		pr_number: None,
	};
	let changes = vec![(
		ChangeType::Minor,
		Some("Added widget\nwith extra details".to_string()),
		Some(commit_ref),
	)];
	let changelog = Changelog::new(
		"1.0.0".parse().unwrap(),
		"2024-01-01".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
	);
	let sections = changelog.format_sections();
	assert!(
		sections.contains("- Added widget [abc1234]\n  with extra details"),
		"Expected suffix on first line with indented continuation, got: {sections}"
	);
}

// --- split_at_first_h2 ---

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

#[tokio::test]
async fn update_changelog_preserves_custom_preamble() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::write(
		dir.path().join("CHANGELOG.md"),
		"# My Custom Title\n\nAn intro paragraph.\n\n## 0.1.0\n\nOld entry\n",
	)
	.unwrap();
	let changes = vec![(ChangeType::Minor, Some("New thing".to_string()), None)];
	let changelog = Changelog::new(
		"0.2.0".parse().unwrap(),
		"2024-06-01".to_string(),
		changes,
		AbsolutePath::new(dir.path()).unwrap(),
	);
	changelog
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();

	let content = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
	insta::assert_snapshot!(content);
}

#[test]
fn format_sections_returns_sections_without_heading() {
	let changes = vec![
		(ChangeType::Minor, Some("Added feature X".to_string()), None),
		(ChangeType::Patch, Some("Fixed bug Y".to_string()), None),
	];
	let changelog = Changelog::new(
		"1.1.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
	);
	let sections = changelog.format_sections();
	assert!(!sections.contains("## 1.1.0"));
	assert!(sections.contains("### Features"));
	assert!(sections.contains("- Added feature X"));
	assert!(sections.contains("### Bug Fixes"));
	assert!(sections.contains("- Fixed bug Y"));
}

#[test]
fn format_sections_dependency_section_separated_by_blank_line() {
	let changes = vec![(ChangeType::Minor, Some("Feature X".to_string()), None)];
	let changelog = Changelog::new(
		"1.0.0".parse().unwrap(),
		"2024-01-01".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
	)
	.with_dependency_entries(vec!["`pkg-a` bumped to 1.0.0".to_string()]);
	let sections = changelog.format_sections();
	assert!(
		sections.contains("\n\n### Dependencies"),
		"Expected blank line before Dependencies section, got: {sections}"
	);
	assert!(sections.contains("### Features"));
	assert!(sections.contains("### Dependencies"));
}

#[test]
fn format_sections_returns_empty_when_no_messages() {
	let changes: Vec<(ChangeType, Option<String>, Option<CommitReference>)> =
		vec![(ChangeType::Minor, None, None)];
	let changelog = Changelog::new(
		"1.1.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
	);
	assert!(changelog.format_sections().is_empty());
}

#[test]
fn format_changelog_entry_with_messages() {
	let changes = vec![
		(ChangeType::Minor, Some("Added feature X".to_string()), None),
		(ChangeType::Patch, Some("Fixed bug Y".to_string()), None),
	];
	let changelog = Changelog::new(
		"1.1.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
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
	let changes: Vec<(ChangeType, Option<String>, Option<CommitReference>)> =
		vec![(ChangeType::Minor, None, None)];
	let changelog = Changelog::new(
		"1.1.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
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
		None,
	)];
	let changelog = Changelog::new(
		"1.1.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
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
		None,
	)];
	let changelog = Changelog::new(
		"1.1.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
	);
	let entry = changelog.format_entry();
	// Blank lines must not be indented
	assert!(entry.contains("- First line\n\n  Second paragraph"));
}

#[test]
fn format_changelog_entry_major_section() {
	let changes = vec![(
		ChangeType::Major,
		Some("Breaking API change".to_string()),
		None,
	)];
	let changelog = Changelog::new(
		"2.0.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new("/nonexistent").unwrap(),
	);
	let entry = changelog.format_entry();
	assert!(entry.contains("### Breaking Changes"));
	assert!(entry.contains("- Breaking API change"));
}

#[tokio::test]
async fn update_changelog_creates_new_file() {
	let dir = tempfile::tempdir().unwrap();
	let changes = vec![(ChangeType::Minor, Some("Something new".to_string()), None)];
	let changelog = Changelog::new(
		"1.0.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new(dir.path()).unwrap(),
	);
	changelog
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();

	let content = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
	assert!(content.contains("# Changelog"));
	assert!(content.contains("## 1.0.0 - 2024-01-15"));
}

#[tokio::test]
async fn update_changelog_prepends_to_existing() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::write(
		dir.path().join("CHANGELOG.md"),
		"# Changelog\n\n## 0.1.0\n\nOld entry\n",
	)
	.unwrap();
	let changes = vec![(ChangeType::Minor, Some("New thing".to_string()), None)];
	let changelog = Changelog::new(
		"0.2.0".parse().unwrap(),
		"2024-06-01".to_string(),
		changes,
		AbsolutePath::new(dir.path()).unwrap(),
	);
	changelog
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();

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

#[tokio::test]
async fn update_changelog_successive_releases_snapshot() {
	let dir = tempfile::tempdir().unwrap();

	let make = |version: &str, msg: &str| {
		Changelog::new(
			version.parse().unwrap(),
			"2024-01-01".to_string(),
			vec![(ChangeType::Patch, Some(msg.to_string()), None)],
			AbsolutePath::new(dir.path()).unwrap(),
		)
	};

	make("1.0.0", "Initial release")
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();
	make("1.0.1", "Second release")
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();
	make("1.0.2", "Third release")
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();

	let content = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
	insta::assert_snapshot!(content);
}

#[tokio::test]
async fn update_changelog_no_duplicate_header_on_successive_releases() {
	let dir = tempfile::tempdir().unwrap();

	let make = |version: &str, msg: &str| {
		Changelog::new(
			version.parse().unwrap(),
			"2024-01-01".to_string(),
			vec![(ChangeType::Patch, Some(msg.to_string()), None)],
			AbsolutePath::new(dir.path()).unwrap(),
		)
	};

	make("1.0.0", "Initial release")
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();
	make("1.0.1", "Second release")
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();
	make("1.0.2", "Third release")
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();

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

#[tokio::test]
async fn update_changelog_in_subdir() {
	let dir = tempfile::tempdir().unwrap();
	let sub = dir.path().join("packages/my-pkg");
	std::fs::create_dir_all(&sub).unwrap();
	let changes = vec![(ChangeType::Patch, Some("Release".to_string()), None)];
	let changelog = Changelog::new(
		"1.0.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new(sub.clone()).unwrap(),
	);
	changelog
		.update(false, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();

	let content = std::fs::read_to_string(sub.join("CHANGELOG.md")).unwrap();
	assert!(content.contains("## 1.0.0 - 2024-01-15"));
}

#[tokio::test]
async fn update_changelog_fails_when_cannot_read_existing() {
	let dir = tempfile::tempdir().unwrap();
	let changelog_path = dir.path().join("CHANGELOG.md");
	// Create a directory with the same name as the file we want to read
	std::fs::create_dir(&changelog_path).unwrap();

	let changes = vec![(ChangeType::Minor, Some("New".to_string()), None)];
	let changelog = Changelog::new(
		"1.0.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new(dir.path()).unwrap(),
	);
	let result = changelog
		.update(false, &crate::filesystem::LocalFilesystem)
		.await;

	// Should fail because CHANGELOG.md is a directory, not a file
	assert!(result.is_err());
}

#[tokio::test]
async fn update_changelog_fails_when_cannot_write() {
	use crate::path::AbsolutePath;

	#[derive(Debug)]
	struct FailingWriteFilesystem;

	#[async_trait::async_trait]
	impl crate::filesystem::Filesystem for FailingWriteFilesystem {
		async fn read_to_string(&self, _: &AbsolutePath) -> anyhow::Result<String> {
			anyhow::bail!("not implemented")
		}
		async fn read(&self, _: &AbsolutePath) -> anyhow::Result<Vec<u8>> {
			anyhow::bail!("not implemented")
		}
		async fn write(&self, _: &AbsolutePath, _: &[u8]) -> anyhow::Result<()> {
			anyhow::bail!("simulated write failure")
		}
		async fn create_dir_all(&self, _: &AbsolutePath) -> anyhow::Result<()> {
			anyhow::bail!("not implemented")
		}
		async fn remove_file(&self, _: &AbsolutePath) -> anyhow::Result<()> {
			anyhow::bail!("not implemented")
		}
		async fn exists(&self, _: &AbsolutePath) -> anyhow::Result<bool> {
			Ok(false)
		}
		async fn is_dir(&self, _: &AbsolutePath) -> anyhow::Result<bool> {
			anyhow::bail!("not implemented")
		}
		async fn canonicalize(&self, _: &AbsolutePath) -> anyhow::Result<std::path::PathBuf> {
			anyhow::bail!("not implemented")
		}
		async fn glob(&self, _: &str) -> anyhow::Result<Vec<std::path::PathBuf>> {
			anyhow::bail!("not implemented")
		}
		async fn file_size(&self, _: &AbsolutePath) -> anyhow::Result<u64> {
			anyhow::bail!("not implemented")
		}
	}

	let dir = tempfile::tempdir().unwrap();
	let changes = vec![(ChangeType::Patch, Some("Fix".to_string()), None)];
	let changelog = Changelog::new(
		"1.0.0".parse().unwrap(),
		"2024-01-15".to_string(),
		changes,
		AbsolutePath::new(dir.path()).unwrap(),
	);
	let result = changelog.update(false, &FailingWriteFilesystem).await;

	// Should fail because the filesystem returns an error on write.
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

#[tokio::test]
async fn extract_version_body_finds_middle_version() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");
	std::fs::write(&path, MULTI_VERSION_CHANGELOG).unwrap();

	let body = extract_version_body(
		&path,
		&"1.1.0".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert!(body.contains("### Bug Fixes"));
	assert!(body.contains("- Fixed thing"));
	assert!(!body.contains("### Features"));
}

#[tokio::test]
async fn extract_version_body_finds_first_version() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");
	std::fs::write(&path, MULTI_VERSION_CHANGELOG).unwrap();

	let body = extract_version_body(
		&path,
		&"1.2.0".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert!(body.contains("### Features"));
	assert!(body.contains("- Added widget"));
}

#[tokio::test]
async fn extract_version_body_finds_version_at_eof() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");
	std::fs::write(&path, MULTI_VERSION_CHANGELOG).unwrap();

	let body = extract_version_body(
		&path,
		&"1.0.0".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert_eq!(body.trim(), "Initial release");
}

#[tokio::test]
async fn extract_version_body_returns_empty_for_missing_version() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");
	std::fs::write(&path, MULTI_VERSION_CHANGELOG).unwrap();

	let body = extract_version_body(
		&path,
		&"9.9.9".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert!(body.is_empty());
}

#[tokio::test]
async fn extract_version_body_returns_error_for_missing_file() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");

	let result = extract_version_body(
		&path,
		&"1.0.0".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn extract_version_body_does_not_match_version_prefix() {
	let changelog = "# Changelog\n\n## 1.2.0-beta - 2024-01-01\n\nbeta content\n\n## 1.2.0 - 2024-02-01\n\nstable content\n";
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");
	std::fs::write(&path, changelog).unwrap();

	let body = extract_version_body(
		&path,
		&"1.2.0".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert!(body.contains("stable content"));
	assert!(!body.contains("beta content"));
}

#[tokio::test]
async fn extract_version_body_with_date_suffix() {
	let changelog = "# Changelog\n\n## 2.0.0 - 2025-01-01\n\nMajor release\n";
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");
	std::fs::write(&path, changelog).unwrap();

	let body = extract_version_body(
		&path,
		&"2.0.0".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert!(body.contains("Major release"));
}

#[tokio::test]
async fn extract_version_body_empty_body() {
	let changelog = "# Changelog\n\n## 1.0.0\n\n## 0.9.0\n\nPrevious\n";
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");
	std::fs::write(&path, changelog).unwrap();

	let body = extract_version_body(
		&path,
		&"1.0.0".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert!(body.is_empty());
}

#[tokio::test]
async fn extract_version_body_strips_leading_blank_lines() {
	// Multiple blank lines before content — the result must not start with a blank line.
	let changelog = "# Changelog\n\n## 1.0.0\n\n\n\nContent here\n";
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");
	std::fs::write(&path, changelog).unwrap();

	let body = extract_version_body(
		&path,
		&"1.0.0".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert!(
		!body.starts_with('\n'),
		"body should not start with blank line, got: {body:?}"
	);
	assert!(body.contains("Content here"));
}

#[tokio::test]
async fn extract_version_body_strips_trailing_blank_lines() {
	// Trailing blank lines between sections — the result must not end with a blank line.
	let changelog = "# Changelog\n\n## 1.0.0\n\nContent here\n\n\n## 0.9.0\n\nPrevious\n";
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("CHANGELOG.md");
	std::fs::write(&path, changelog).unwrap();

	let body = extract_version_body(
		&path,
		&"1.0.0".parse().unwrap(),
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert!(
		!body.ends_with('\n'),
		"body should not end with blank line, got: {body:?}"
	);
	assert!(body.contains("Content here"));
}
