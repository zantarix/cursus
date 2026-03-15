//! Integration tests for `cursus verify`.

mod common;

use std::process::ExitCode;

use common::{add_local_remote, git_cmd, git_push_to_remote, git_set_remote_head, run_cursus};
use tempfile::TempDir;

/// Creates a repo with a feature branch ready for verify tests.
///
/// Sets up: main repo → bare remote → push main → set origin/HEAD → checkout feature branch.
/// Returns `(working_dir, remote_dir)` — both must stay alive for the test duration.
fn setup_verify_repo() -> (TempDir, TempDir) {
	let working = tempfile::tempdir().expect("Failed to create working dir");
	let dir = working.path();

	git_cmd(dir, &["init"]);
	git_cmd(dir, &["config", "user.name", "Cursus Test"]);
	git_cmd(dir, &["config", "user.email", "test@cursus.local"]);
	git_cmd(dir, &["config", "commit.gpgsign", "false"]);
	git_cmd(dir, &["config", "tag.gpgsign", "false"]);
	git_cmd(
		dir,
		&["commit", "--allow-empty", "-m", "chore: initial commit"],
	);

	let remote = add_local_remote(dir);
	git_push_to_remote(dir);
	git_set_remote_head(dir, "main");

	git_cmd(dir, &["checkout", "-b", "feature/my-change"]);

	(working, remote)
}

#[test]
fn verify_exits_0_when_changeset_added() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	std::fs::create_dir_all(dir.join(".cursus")).unwrap();
	std::fs::write(
		dir.join(".cursus/test.md"),
		"+++\nmy-app = \"minor\"\n+++\n\nA change\n",
	)
	.unwrap();
	git_cmd(dir, &["add", ".cursus/test.md"]);
	git_cmd(dir, &["commit", "-m", "feat: add changeset"]);

	let result = run_cursus(["cursus", "--no-interactive", "verify"], dir).unwrap();
	assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn verify_exits_2_when_no_changeset() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	std::fs::write(dir.join("some-file.txt"), "hello").unwrap();
	git_cmd(dir, &["add", "some-file.txt"]);
	git_cmd(dir, &["commit", "-m", "feat: add non-changeset file"]);

	let result = run_cursus(["cursus", "--no-interactive", "verify"], dir).unwrap();
	assert_eq!(result, ExitCode::from(2));
}

#[test]
fn verify_exits_2_on_empty_branch() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	let result = run_cursus(["cursus", "--no-interactive", "verify"], dir).unwrap();
	assert_eq!(result, ExitCode::from(2));
}

#[test]
fn verify_ignores_readme_md() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	std::fs::create_dir_all(dir.join(".cursus")).unwrap();
	std::fs::write(dir.join(".cursus/README.md"), "# Changesets\n").unwrap();
	git_cmd(dir, &["add", ".cursus/README.md"]);
	git_cmd(dir, &["commit", "-m", "docs: add README to .cursus"]);

	let result = run_cursus(["cursus", "--no-interactive", "verify"], dir).unwrap();
	assert_eq!(result, ExitCode::from(2));
}

#[test]
fn verify_ignores_readme_case_insensitive() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	std::fs::create_dir_all(dir.join(".cursus")).unwrap();
	std::fs::write(dir.join(".cursus/Readme.md"), "# Changesets\n").unwrap();
	git_cmd(dir, &["add", ".cursus/Readme.md"]);
	git_cmd(dir, &["commit", "-m", "docs: add readme variant"]);

	let result = run_cursus(["cursus", "--no-interactive", "verify"], dir).unwrap();
	assert_eq!(result, ExitCode::from(2));
}

#[test]
fn verify_custom_base_ref() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	std::fs::create_dir_all(dir.join(".cursus")).unwrap();
	std::fs::write(
		dir.join(".cursus/my-change.md"),
		"+++\nmy-app = \"patch\"\n+++\n\n",
	)
	.unwrap();
	git_cmd(dir, &["add", ".cursus/my-change.md"]);
	git_cmd(dir, &["commit", "-m", "feat: add changeset"]);

	let result = run_cursus(
		["cursus", "--no-interactive", "verify", "--base", "main"],
		dir,
	)
	.unwrap();
	assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn verify_error_on_invalid_base_ref() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	let result = run_cursus(
		[
			"cursus",
			"--no-interactive",
			"verify",
			"--base",
			"nonexistent-ref",
		],
		dir,
	);
	assert!(result.is_err());
}

#[test]
fn verify_ignores_modified_changesets() {
	// Build a repo where an existing changeset is committed to main BEFORE branching.
	let working = tempfile::tempdir().expect("Failed to create working dir");
	let dir = working.path();

	git_cmd(dir, &["init"]);
	git_cmd(dir, &["config", "user.name", "Cursus Test"]);
	git_cmd(dir, &["config", "user.email", "test@cursus.local"]);
	git_cmd(dir, &["config", "commit.gpgsign", "false"]);
	git_cmd(dir, &["config", "tag.gpgsign", "false"]);
	git_cmd(
		dir,
		&["commit", "--allow-empty", "-m", "chore: initial commit"],
	);

	// Add the changeset on main, then push.
	std::fs::create_dir_all(dir.join(".cursus")).unwrap();
	std::fs::write(
		dir.join(".cursus/existing.md"),
		"+++\nmy-app = \"patch\"\n+++\n\n",
	)
	.unwrap();
	git_cmd(dir, &["add", ".cursus/existing.md"]);
	git_cmd(dir, &["commit", "-m", "feat: existing changeset on main"]);

	let _remote = add_local_remote(dir);
	git_push_to_remote(dir);
	git_set_remote_head(dir, "main");

	// Checkout a feature branch and only modify (not add) the changeset.
	git_cmd(dir, &["checkout", "-b", "feature/modify-only"]);
	std::fs::write(
		dir.join(".cursus/existing.md"),
		"+++\nmy-app = \"minor\"\n+++\n\nModified\n",
	)
	.unwrap();
	git_cmd(dir, &["add", ".cursus/existing.md"]);
	git_cmd(dir, &["commit", "-m", "feat: modify changeset"]);

	let result = run_cursus(
		["cursus", "--no-interactive", "verify", "--base", "main"],
		dir,
	)
	.unwrap();
	assert_eq!(result, ExitCode::from(2));
}

#[test]
fn verify_works_without_config() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	// No .cursus/config.toml — verify should not require it
	std::fs::create_dir_all(dir.join(".cursus")).unwrap();
	std::fs::write(
		dir.join(".cursus/change.md"),
		"+++\nmy-app = \"minor\"\n+++\n\n",
	)
	.unwrap();
	git_cmd(dir, &["add", ".cursus/change.md"]);
	git_cmd(dir, &["commit", "-m", "feat: add changeset"]);

	let result = run_cursus(["cursus", "--no-interactive", "verify"], dir).unwrap();
	assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn verify_dry_run_behaves_identically() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	std::fs::create_dir_all(dir.join(".cursus")).unwrap();
	std::fs::write(
		dir.join(".cursus/change.md"),
		"+++\nmy-app = \"minor\"\n+++\n\n",
	)
	.unwrap();
	git_cmd(dir, &["add", ".cursus/change.md"]);
	git_cmd(dir, &["commit", "-m", "feat: add changeset"]);

	let result_normal = run_cursus(["cursus", "--no-interactive", "verify"], dir).unwrap();
	let result_dry =
		run_cursus(["cursus", "--no-interactive", "--dry-run", "verify"], dir).unwrap();
	assert_eq!(result_normal, result_dry);
}

#[test]
fn verify_lists_multiple_changesets() {
	let (working, _remote) = setup_verify_repo();
	let dir = working.path();

	std::fs::create_dir_all(dir.join(".cursus")).unwrap();
	std::fs::write(
		dir.join(".cursus/change-a.md"),
		"+++\nmy-app = \"minor\"\n+++\n\n",
	)
	.unwrap();
	std::fs::write(
		dir.join(".cursus/change-b.md"),
		"+++\nmy-lib = \"patch\"\n+++\n\n",
	)
	.unwrap();
	git_cmd(dir, &["add", ".cursus/change-a.md", ".cursus/change-b.md"]);
	git_cmd(dir, &["commit", "-m", "feat: add two changesets"]);

	let result = run_cursus(["cursus", "--no-interactive", "verify"], dir).unwrap();
	assert_eq!(result, ExitCode::SUCCESS);
}
