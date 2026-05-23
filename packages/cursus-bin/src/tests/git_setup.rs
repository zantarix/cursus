use cursus::model::config::SignedCommitsMode;

use crate::git_setup::{resolve_signed_commits_mode, resolve_signed_commits_mode_gitlab};

#[test]
fn off_always_false() {
	assert!(!resolve_signed_commits_mode(
		SignedCommitsMode::Off,
		true,
		true
	));
	assert!(!resolve_signed_commits_mode(
		SignedCommitsMode::Off,
		false,
		false
	));
}

#[test]
fn force_requires_only_token() {
	assert!(resolve_signed_commits_mode(
		SignedCommitsMode::Force,
		true,
		false
	));
	assert!(!resolve_signed_commits_mode(
		SignedCommitsMode::Force,
		false,
		true
	));
}

#[test]
fn auto_requires_gha_and_token() {
	assert!(resolve_signed_commits_mode(
		SignedCommitsMode::Auto,
		true,
		true
	));
	assert!(!resolve_signed_commits_mode(
		SignedCommitsMode::Auto,
		true,
		false
	));
	assert!(!resolve_signed_commits_mode(
		SignedCommitsMode::Auto,
		false,
		true
	));
	assert!(!resolve_signed_commits_mode(
		SignedCommitsMode::Auto,
		false,
		false
	));
}

// ── GitLab parallel ──────────────────────────────────────────────────────────

#[test]
fn gitlab_off_always_false() {
	assert!(!resolve_signed_commits_mode_gitlab(
		SignedCommitsMode::Off,
		true,
		true
	));
	assert!(!resolve_signed_commits_mode_gitlab(
		SignedCommitsMode::Off,
		false,
		false
	));
}

#[test]
fn gitlab_force_requires_only_token() {
	assert!(resolve_signed_commits_mode_gitlab(
		SignedCommitsMode::Force,
		true,
		false
	));
	assert!(!resolve_signed_commits_mode_gitlab(
		SignedCommitsMode::Force,
		false,
		true
	));
}

#[test]
fn gitlab_auto_requires_gitlab_ci_and_token() {
	assert!(resolve_signed_commits_mode_gitlab(
		SignedCommitsMode::Auto,
		true,
		true
	));
	assert!(!resolve_signed_commits_mode_gitlab(
		SignedCommitsMode::Auto,
		true,
		false
	));
	assert!(!resolve_signed_commits_mode_gitlab(
		SignedCommitsMode::Auto,
		false,
		true
	));
	assert!(!resolve_signed_commits_mode_gitlab(
		SignedCommitsMode::Auto,
		false,
		false
	));
}
