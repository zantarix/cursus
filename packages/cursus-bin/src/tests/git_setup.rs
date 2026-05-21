use cursus::model::config::SignedCommitsMode;

use crate::git_setup::resolve_signed_commits_mode;

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
