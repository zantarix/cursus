use std::sync::Arc;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::filesystem::LocalFilesystem;
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

#[tokio::test]
async fn write_changeset_creates_file() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let changeset = single_package_changeset();
	let path = changeset.write(env.git(), env.fs()).await.unwrap();
	assert!(path.exists(), "Changeset file should exist");
	assert!(path.starts_with(dir.path().join(".cursus")));
	assert!(path.extension().is_some_and(|ext| ext == "md"));
}

#[tokio::test]
async fn write_changeset_creates_directory() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let changeset = single_package_changeset();
	changeset.write(env.git(), env.fs()).await.unwrap();
	assert!(
		dir.path().join(".cursus").is_dir(),
		".cursus directory should exist"
	);
}

#[tokio::test]
async fn write_changeset_file_has_correct_content() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let mut changeset = single_package_changeset();
	changeset.message = Some("Test message".to_string());
	let path = changeset.write(env.git(), env.fs()).await.unwrap();
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
fn parse_changeset_closing_delimiter_no_trailing_newline() {
	let input = "+++
pkg = \"minor\"
+++";
	let parsed = Changeset::parse(input).unwrap();
	assert_eq!(parsed.packages["pkg"], ChangeType::Minor);
	assert_eq!(parsed.message, None);
}

#[test]
fn parse_changeset_crlf_line_endings() {
	let input = "+++\r\npkg = \"minor\"\r\n+++\r\n\r\nSome message\r\n";
	let parsed = Changeset::parse(input).unwrap();
	assert_eq!(parsed.packages["pkg"], ChangeType::Minor);
	assert_eq!(parsed.message, Some("Some message".to_string()));
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

#[tokio::test]
async fn read_all_changesets_empty_when_no_directory() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let result = Changeset::read_all(&env).await.unwrap();
	assert!(result.is_empty());
}

#[tokio::test]
async fn read_all_changesets_empty_when_no_md_files() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let cursus_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&cursus_dir).unwrap();
	std::fs::write(cursus_dir.join("config.toml"), "").unwrap();
	let result = Changeset::read_all(&env).await.unwrap();
	assert!(result.is_empty());
}

#[tokio::test]
async fn read_all_changesets_single_file() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let cursus_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&cursus_dir).unwrap();
	std::fs::write(
		cursus_dir.join("test.md"),
		"+++\nmy-app = \"minor\"\n+++\n\nA change\n",
	)
	.unwrap();

	let result = Changeset::read_all(&env).await.unwrap();
	assert_eq!(result.len(), 1);
	assert_eq!(result[0].1.packages["my-app"], ChangeType::Minor);
	assert_eq!(result[0].1.message, Some("A change".to_string()));
}

#[tokio::test]
async fn read_all_changesets_multiple_files() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let cursus_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&cursus_dir).unwrap();
	std::fs::write(cursus_dir.join("a.md"), "+++\napp = \"minor\"\n+++\n\n").unwrap();
	std::fs::write(cursus_dir.join("b.md"), "+++\napp = \"patch\"\n+++\n\n").unwrap();

	let result = Changeset::read_all(&env).await.unwrap();
	assert_eq!(result.len(), 2);
}

#[tokio::test]
async fn read_all_changesets_invalid_file_returns_error() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let cursus_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&cursus_dir).unwrap();
	std::fs::write(cursus_dir.join("bad.md"), "not a valid changeset").unwrap();

	let result = Changeset::read_all(&env).await;
	assert!(result.is_err());
}

#[tokio::test]
async fn read_all_changesets_skips_readme() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let cursus_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&cursus_dir).unwrap();
	std::fs::write(
		cursus_dir.join("README.md"),
		"# Changesets\nNot a changeset.",
	)
	.unwrap();
	std::fs::write(cursus_dir.join("valid.md"), "+++\napp = \"minor\"\n+++\n\n").unwrap();

	let result = Changeset::read_all(&env).await.unwrap();
	assert_eq!(result.len(), 1, "README.md should be skipped");
	assert_eq!(result[0].1.packages["app"], ChangeType::Minor);
}

#[tokio::test]
async fn read_all_changesets_skips_readme_case_insensitive() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let cursus_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&cursus_dir).unwrap();
	std::fs::write(cursus_dir.join("readme.md"), "not a changeset").unwrap();

	let result = Changeset::read_all(&env).await.unwrap();
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

#[tokio::test]
async fn consume_changeset_fully_consumed_deletes_file() {
	let dir = tempfile::tempdir().unwrap();
	let (path, cs) = make_path_and_changeset(
		dir.path(),
		"change.md",
		"+++\npkg-a = \"patch\"\n+++\n\nSome message\n",
	);
	let released: BTreeSet<String> = ["pkg-a".to_string()].into();
	cs.consume(
		&crate::path::AbsolutePath::new(&path).unwrap(),
		&released,
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();
	assert!(!path.exists(), "File should be deleted when fully consumed");
}

#[tokio::test]
async fn consume_changeset_partially_consumed_rewrites_file() {
	let dir = tempfile::tempdir().unwrap();
	let (path, cs) = make_path_and_changeset(
		dir.path(),
		"change.md",
		"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome message\n",
	);
	let released: BTreeSet<String> = ["pkg-a".to_string()].into();
	cs.consume(
		&crate::path::AbsolutePath::new(&path).unwrap(),
		&released,
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();

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

#[tokio::test]
async fn consume_changeset_unrelated_leaves_file_untouched() {
	let dir = tempfile::tempdir().unwrap();
	let original = "+++\npkg-b = \"minor\"\n+++\n\nUnrelated change\n";
	let (path, cs) = make_path_and_changeset(dir.path(), "change.md", original);
	let released: BTreeSet<String> = ["pkg-a".to_string()].into();
	cs.consume(
		&crate::path::AbsolutePath::new(&path).unwrap(),
		&released,
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();

	assert!(path.exists(), "File should be untouched");
	let content = std::fs::read_to_string(&path).unwrap();
	assert_eq!(content, original, "File contents should be unchanged");
}

#[tokio::test]
async fn consume_changeset_partial_rewrite_round_trips() {
	let dir = tempfile::tempdir().unwrap();
	let (path, cs) = make_path_and_changeset(
		dir.path(),
		"change.md",
		"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\npkg-c = \"major\"\n+++\n\nMulti-package change\n",
	);
	let released: BTreeSet<String> = ["pkg-a".to_string(), "pkg-c".to_string()].into();
	cs.consume(
		&crate::path::AbsolutePath::new(&path).unwrap(),
		&released,
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();

	let content = std::fs::read_to_string(&path).unwrap();
	let reparsed = Changeset::parse(&content).unwrap();
	assert_eq!(reparsed.packages.len(), 1);
	assert_eq!(reparsed.packages["pkg-b"], ChangeType::Minor);
	assert_eq!(reparsed.message, Some("Multi-package change".to_string()));
}

#[tokio::test]
async fn consume_changeset_delete_fails_returns_error() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("nonexistent.md");
	let mut packages = BTreeMap::new();
	packages.insert("pkg-a".to_string(), ChangeType::Patch);
	let cs = Changeset::new(packages, None);
	let released: BTreeSet<String> = ["pkg-a".to_string()].into();
	// File doesn't exist, so remove_file should fail
	let result = cs
		.consume(
			&crate::path::AbsolutePath::new(&path).unwrap(),
			&released,
			&crate::filesystem::LocalFilesystem,
		)
		.await;
	assert!(result.is_err(), "Should fail when file cannot be deleted");
}

#[tokio::test]
async fn consume_changeset_rewrite_fails_returns_error() {
	let dir = tempfile::tempdir().unwrap();
	// Path inside a non-existent subdirectory — fs::write will fail.
	let path = dir.path().join("no-such-dir/change.md");
	let mut packages = BTreeMap::new();
	packages.insert("pkg-a".to_string(), ChangeType::Patch);
	packages.insert("pkg-b".to_string(), ChangeType::Minor);
	let cs = Changeset::new(packages, None);
	let released: BTreeSet<String> = ["pkg-a".to_string()].into();
	// Partially consumed → rewrite branch triggered, but parent dir missing.
	let result = cs
		.consume(
			&crate::path::AbsolutePath::new(&path).unwrap(),
			&released,
			&crate::filesystem::LocalFilesystem,
		)
		.await;
	assert!(result.is_err(), "Should fail when file cannot be rewritten");
}

// --- derive_changeset ---

#[test]
fn derive_changeset_feat_produces_minor() {
	let result = derive_changeset("feat: add new widget", &["my-app"]).unwrap();
	let cs = result.expect("should produce a changeset");
	assert_eq!(cs.packages["my-app"], ChangeType::Minor);
	assert_eq!(cs.message, Some("add new widget".to_string()));
}

#[test]
fn derive_changeset_fix_produces_patch() {
	let result = derive_changeset("fix: correct off-by-one", &["my-app"]).unwrap();
	let cs = result.expect("should produce a changeset");
	assert_eq!(cs.packages["my-app"], ChangeType::Patch);
}

#[test]
fn derive_changeset_breaking_produces_major() {
	let result = derive_changeset("feat!: remove deprecated API", &["my-app"]).unwrap();
	let cs = result.expect("should produce a changeset");
	assert_eq!(cs.packages["my-app"], ChangeType::Major);
}

#[test]
fn derive_changeset_chore_returns_none() {
	let result = derive_changeset("chore: update deps", &["my-app"]).unwrap();
	assert!(result.is_none());
}

#[test]
fn derive_changeset_invalid_message_returns_error() {
	assert!(derive_changeset("not a valid commit", &["my-app"]).is_err());
}

#[test]
fn derive_changeset_multiple_projects() {
	let result = derive_changeset("feat: shared change", &["app-a", "app-b"]).unwrap();
	let cs = result.expect("should produce a changeset");
	assert_eq!(cs.packages.len(), 2);
	assert_eq!(cs.packages["app-a"], ChangeType::Minor);
	assert_eq!(cs.packages["app-b"], ChangeType::Minor);
}

#[test]
fn derive_changeset_includes_body_in_message() {
	let msg = "feat: add widget\n\nThis adds a comprehensive widget system.";
	let result = derive_changeset(msg, &["my-app"]).unwrap();
	let cs = result.expect("should produce a changeset");
	assert_eq!(
		cs.message,
		Some("add widget\n\nThis adds a comprehensive widget system.".to_string())
	);
}

// --- filter_changeset_paths ---

#[test]
fn filter_changeset_paths_keeps_md_files() {
	let paths = vec![
		".cursus/some-change.md".to_string(),
		".cursus/another.md".to_string(),
	];
	let result = filter_changeset_paths(&paths);
	assert_eq!(result, vec![".cursus/some-change.md", ".cursus/another.md"]);
}

#[test]
fn filter_changeset_paths_excludes_readme() {
	let paths = vec![
		".cursus/README.md".to_string(),
		".cursus/valid.md".to_string(),
	];
	let result = filter_changeset_paths(&paths);
	assert_eq!(result, vec![".cursus/valid.md"]);
}

#[test]
fn filter_changeset_paths_excludes_non_md() {
	let paths = vec![
		".cursus/config.toml".to_string(),
		".cursus/valid.md".to_string(),
	];
	let result = filter_changeset_paths(&paths);
	assert_eq!(result, vec![".cursus/valid.md"]);
}

#[test]
fn filter_changeset_paths_bare_filename_no_directory() {
	let paths = vec!["some-change.md".to_string()];
	let result = filter_changeset_paths(&paths);
	assert_eq!(result, vec!["some-change.md"]);
}

#[test]
fn filter_changeset_paths_empty_input() {
	let paths: Vec<String> = vec![];
	let result = filter_changeset_paths(&paths);
	assert!(result.is_empty());
}

#[tokio::test]
async fn read_all_rejects_oversized_changeset() {
	let dir = tempfile::tempdir().unwrap();
	let (abs, runner) = make_git(&dir);
	let env = crate::Env::new(
		Arc::clone(&runner) as Arc<dyn CommandRunner>,
		Arc::new(LocalFilesystem),
		std::sync::Arc::new(GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			abs.clone(),
		)),
	);
	let cursus_dir = dir.path().join(".cursus");
	std::fs::create_dir_all(&cursus_dir).unwrap();
	// Write a file larger than 64 KiB.
	let oversized = vec![b'x'; 65 * 1024];
	std::fs::write(cursus_dir.join("big.md"), &oversized).unwrap();

	let err = Changeset::read_all(&env).await.unwrap_err();
	assert!(
		err.to_string().contains("too large"),
		"unexpected error: {err}"
	);
}
