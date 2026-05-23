use std::collections::HashSet;

use crate::package_manager::Project;
use crate::package_manager::matching::*;
use crate::path::AbsolutePath;

#[test]
fn basic_prefix_match() {
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("a", "/repo/packages/a"),
		Project::new_test("b", "/repo/packages/b"),
	];
	let mut files = HashSet::new();
	files.insert("packages/a/src/lib.rs".to_string());
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![true, false]
	);
}

#[test]
fn no_match_for_different_project() {
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("a", "/repo/packages/a"),
		Project::new_test("b", "/repo/packages/b"),
	];
	let mut files = HashSet::new();
	files.insert("packages/b/src/lib.rs".to_string());
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![false, true]
	);
}

#[test]
fn no_prefix_match_without_separator() {
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("a", "/repo/packages/a"),
		Project::new_test("a-extra", "/repo/packages/a-extra"),
	];
	let mut files = HashSet::new();
	files.insert("packages/a-extra/lib.rs".to_string());
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![false, true]
	);
}

#[test]
fn nested_file_goes_to_child() {
	// A file inside the child project must only match the child, not the parent.
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("parent", "/repo/packages/a"),
		Project::new_test("child", "/repo/packages/a/sub"),
	];
	let mut files = HashSet::new();
	files.insert("packages/a/sub/src/lib.rs".to_string());
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![false, true]
	);
}

#[test]
fn nested_parent_file_goes_to_parent() {
	// A file inside the parent but outside the child must go to the parent.
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("parent", "/repo/packages/a"),
		Project::new_test("child", "/repo/packages/a/sub"),
	];
	let mut files = HashSet::new();
	files.insert("packages/a/README.md".to_string());
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![true, false]
	);
}

#[test]
fn root_project_matches_unowned_file() {
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("root", "/repo"),
		Project::new_test("a", "/repo/packages/a"),
	];
	let mut files = HashSet::new();
	files.insert("src/main.rs".to_string());
	// src/main.rs is not under packages/a, so root gets it.
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![true, false]
	);
}

#[test]
fn root_does_not_steal_from_subproject() {
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("root", "/repo"),
		Project::new_test("a", "/repo/packages/a"),
	];
	let mut files = HashSet::new();
	files.insert("packages/a/src/lib.rs".to_string());
	// packages/a/src/lib.rs belongs to "a", not root.
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![false, true]
	);
}

#[test]
fn empty_files() {
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![Project::new_test("root", "/repo")];
	let files = HashSet::new();
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![false]
	);
}

#[test]
fn outside_git_root_always_unchanged() {
	// Git cannot track files outside the repo, so out-of-root projects are always unchanged.
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![Project::new_test("outside", "/other/path")];
	let files = HashSet::new();
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![false]
	);
}

#[test]
fn outside_git_root_unchanged_even_with_files() {
	// Out-of-root project is not attributed any files; in-repo project is still matched.
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("outside", "/other/path"),
		Project::new_test("a", "/repo/packages/a"),
	];
	let mut files = HashSet::new();
	files.insert("packages/a/src/lib.rs".to_string());
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![false, true]
	);
}

#[test]
fn unowned_file_with_no_root() {
	// A file that doesn't fall under any project's path should not mark any project.
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("a", "/repo/packages/a"),
		Project::new_test("b", "/repo/packages/b"),
	];
	let mut files = HashSet::new();
	files.insert("other/random.txt".to_string());
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![false, false]
	);
}

#[test]
fn multiple_at_same_path_all_marked() {
	// When multiple projects share the same path (e.g. Cargo and npm at the repo root),
	// all of them are marked changed when a file in their shared directory changes.
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("npm-root", "/repo"),
		Project::new_test("cargo-root", "/repo"),
		Project::new_test("sub", "/repo/packages/sub"),
	];
	let mut files = HashSet::new();
	files.insert("README.md".to_string());
	// README.md is not under packages/sub, so only the two root projects match.
	// Both share priority 0 (root), so both must be marked.
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![true, true, false]
	);
}

#[test]
fn exact_path_length_match() {
	// A changed file whose path exactly equals the project's relative path.
	// Guards `==`→`!=` on `file.len() == rel.len()` boundary check.
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![Project::new_test("my-pkg", "/repo/my-pkg")];
	let mut files = HashSet::new();
	files.insert("my-pkg".to_string()); // exactly matches rel path, no trailing /
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![true]
	);
}

// ── match_files_to_projects_in_scope ──────────────────────────────────────

#[test]
fn in_scope_ignored_subproject_prevents_parent_attribution() {
	// /foo is releasable; /foo/tests is ignored (absent from `projects`).
	// A file inside /foo/tests must NOT be attributed to /foo.
	let path = AbsolutePath::new("/repo").unwrap();
	let releasable = vec![
		Project::new_test("root", "/repo"),
		Project::new_test("foo", "/repo/foo"),
	];
	let all = vec![
		Project::new_test("root", "/repo"),
		Project::new_test("foo", "/repo/foo"),
		Project::new_test("foo-tests", "/repo/foo/tests"),
	];
	let mut files = HashSet::new();
	files.insert("foo/tests/README.md".to_string());
	// foo/tests gets the attribution; foo and root are not changed.
	assert_eq!(
		match_files_to_projects_in_scope(&releasable, &all, &path, &files),
		vec![false, false]
	);
}

#[test]
fn in_scope_file_outside_ignored_subproject_still_attributes_parent() {
	let path = AbsolutePath::new("/repo").unwrap();
	let releasable = vec![
		Project::new_test("root", "/repo"),
		Project::new_test("foo", "/repo/foo"),
	];
	let all = vec![
		Project::new_test("root", "/repo"),
		Project::new_test("foo", "/repo/foo"),
		Project::new_test("foo-tests", "/repo/foo/tests"),
	];
	let mut files = HashSet::new();
	files.insert("foo/src/lib.rs".to_string());
	// foo/src/lib.rs belongs to foo (not to foo-tests), so foo is changed.
	assert_eq!(
		match_files_to_projects_in_scope(&releasable, &all, &path, &files),
		vec![false, true]
	);
}

#[test]
fn in_scope_same_scope_as_projects_matches_identically() {
	// When attribution_scope == projects, result must match match_files_to_projects.
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("a", "/repo/packages/a"),
		Project::new_test("b", "/repo/packages/b"),
	];
	let mut files = HashSet::new();
	files.insert("packages/a/src/lib.rs".to_string());
	let direct = match_files_to_projects(&projects, &path, &files);
	let scoped = match_files_to_projects_in_scope(&projects, &projects, &path, &files);
	assert_eq!(direct, scoped);
}

#[test]
fn multiple_at_same_path_subproject_wins() {
	// When multiple projects share the same root path, a deeper subproject
	// still wins for files inside it — the shared-root projects are not marked.
	let path = AbsolutePath::new("/repo").unwrap();
	let projects = vec![
		Project::new_test("npm-root", "/repo"),
		Project::new_test("cargo-root", "/repo"),
		Project::new_test("sub", "/repo/packages/sub"),
	];
	let mut files = HashSet::new();
	files.insert("packages/sub/src/lib.rs".to_string());
	assert_eq!(
		match_files_to_projects(&projects, &path, &files),
		vec![false, false, true]
	);
}
