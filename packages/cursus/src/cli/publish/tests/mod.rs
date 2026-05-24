//! Tests for the publish submodule.

mod forge_releases;
mod tags;

use crate::cli::publish::*;
use crate::package_manager::DependencyGraph;

#[tokio::test]
async fn default_publish_args() {
	let args = PublishArgs::default();
	assert!(args.packages.is_empty());
	assert!(!args.no_git);
}

fn make_graph(edges: &[(&str, &[&str])]) -> DependencyGraph {
	let adjacency = edges
		.iter()
		.map(|(k, vs)| (k.to_string(), vs.iter().map(|v| v.to_string()).collect()))
		.collect();
	DependencyGraph::from_adjacency(adjacency)
}

#[tokio::test]
async fn add_transitive_dependents_linear_chain() {
	// c -> b -> a: if c fails, b and a should be blocked
	let graph = make_graph(&[("a", &["b"]), ("b", &["c"]), ("c", &[])]);
	let mut blocked = std::collections::HashSet::new();
	add_transitive_dependents(&graph, "c", &mut blocked);
	assert!(blocked.contains("b"), "b depends on c");
	assert!(blocked.contains("a"), "a depends on b (transitive)");
	assert!(
		!blocked.contains("c"),
		"failed package itself not in blocked"
	);
}

#[tokio::test]
async fn add_transitive_dependents_diamond() {
	// d <- b <- a, d <- c <- a: if d fails, b, c and a should be blocked
	let graph = make_graph(&[("a", &["b", "c"]), ("b", &["d"]), ("c", &["d"]), ("d", &[])]);
	let mut blocked = std::collections::HashSet::new();
	add_transitive_dependents(&graph, "d", &mut blocked);
	assert!(blocked.contains("b"));
	assert!(blocked.contains("c"));
	assert!(blocked.contains("a"));
	assert!(!blocked.contains("d"));
}

#[tokio::test]
async fn add_transitive_dependents_cycle_terminates() {
	// a <-> b: if a fails, b should be blocked; cycle must not loop infinitely
	let graph = make_graph(&[("a", &["b"]), ("b", &["a"])]);
	let mut blocked = std::collections::HashSet::new();
	add_transitive_dependents(&graph, "a", &mut blocked);
	assert!(blocked.contains("b"));
	// Must terminate (would panic/hang otherwise)
}

#[tokio::test]
async fn add_transitive_dependents_independent_subtree_not_blocked() {
	// a -> b, c -> d: if b fails only a is blocked; c and d are unaffected
	let graph = make_graph(&[("a", &["b"]), ("b", &[]), ("c", &["d"]), ("d", &[])]);
	let mut blocked = std::collections::HashSet::new();
	add_transitive_dependents(&graph, "b", &mut blocked);
	assert!(blocked.contains("a"));
	assert!(!blocked.contains("c"));
	assert!(!blocked.contains("d"));
}

// ── log_summary_line tests ────────────────────────────────────────────────

/// Builds a [`GitReleaseOutcome`] carrying only the forge-release counts that
/// `log_forge_releases_summary` reads (tag counts are irrelevant to it).
fn release_outcome(
	releases_created: usize,
	releases_already_present: usize,
	forge_failed: bool,
) -> GitReleaseOutcome {
	GitReleaseOutcome {
		tags_created: 0,
		tags_skipped: 0,
		tags_push_failed: 0,
		releases_created,
		releases_already_present,
		forge_failed,
	}
}

fn make_empty_outcome() -> GitReleaseOutcome {
	GitReleaseOutcome {
		tags_created: 0,
		tags_skipped: 0,
		tags_push_failed: 0,
		releases_created: 0,
		releases_already_present: 0,
		forge_failed: false,
	}
}

fn make_published_package() -> PublishedPackage {
	PublishedPackage {
		name: "pkg".to_string(),
		version: "1.0.0".parse().unwrap(),
		project_path: crate::path::AbsolutePath::new("/nonexistent").unwrap(),
	}
}

#[tokio::test]
async fn log_summary_line_non_dry_run_dep_skipped_note_in_log() {
	// dep_skipped_note only appears in non-dry-run mode.
	// Guards `> 0` → `> 1` on dep_skipped_count condition.
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let mut state = PublishState::new();
	state.dep_skipped_count = 1; // exactly 1, to catch "> 1" mutation
	let flags = PublishFlags {
		dry_run: false,
		git_enabled: false,
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	log_summary_line(&state, &flags, &make_empty_outcome());
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("skipped (dependency failed)")),
		"Expected dep-skipped note in log: {logs:?}"
	);
}

#[tokio::test]
async fn log_summary_line_non_dry_run_unprepared_note_in_log() {
	// unprepared_note appears in both dry-run (via tag_note path) and non-dry-run.
	// Guards `> 0` → `> 1` on unprepared_count condition.
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let mut state = PublishState::new();
	state.unprepared_count = 1; // exactly 1, to catch "> 1" mutation
	let flags = PublishFlags {
		dry_run: false,
		git_enabled: false,
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	log_summary_line(&state, &flags, &make_empty_outcome());
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("skipped (not yet prepared)")),
		"Expected unprepared note in log: {logs:?}"
	);
}

#[tokio::test]
async fn log_summary_line_dry_run_git_disabled_no_tag_note() {
	// Guards &&→|| on `flags.git_enabled && !state.published.is_empty()` (tag_note guard).
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let mut state = PublishState::new();
	state.published.push(make_published_package()); // non-empty published
	let flags = PublishFlags {
		dry_run: true,
		git_enabled: false, // git disabled
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	log_summary_line(&state, &flags, &make_empty_outcome());
	let logs = crate::test_logging::take_logs();
	assert!(
		!logs.iter().any(|(_, m)| m.contains("would be tagged")),
		"Should NOT log 'would be tagged' when git is disabled: {logs:?}"
	);
}

#[tokio::test]
async fn log_summary_line_dry_run_git_enabled_tag_note_present() {
	// Guards &&→|| on `flags.git_enabled && !state.published.is_empty()` (tag_note guard).
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let mut state = PublishState::new();
	state.published.push(make_published_package());
	let flags = PublishFlags {
		dry_run: true,
		git_enabled: true, // git enabled
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	log_summary_line(&state, &flags, &make_empty_outcome());
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter().any(|(_, m)| m.contains("would be tagged")),
		"Should log 'would be tagged' when git is enabled: {logs:?}"
	);
}

// ── log_publish_summary tests ─────────────────────────────────────────────

#[tokio::test]
async fn log_publish_summary_tags_created_appears_in_log() {
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let state = PublishState::new();
	let flags = PublishFlags {
		dry_run: false,
		git_enabled: true,
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	let outcome = GitReleaseOutcome {
		tags_created: 2,
		tags_skipped: 0,
		tags_push_failed: 0,
		releases_created: 0,
		releases_already_present: 0,
		forge_failed: false,
	};
	log_publish_summary(&state, &flags, &outcome);
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("tag") && m.contains("created")),
		"Expected 'tag(s) created' in logs: {logs:?}"
	);
}

#[tokio::test]
async fn log_publish_summary_tags_push_failed_appears_in_log() {
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let state = PublishState::new();
	let flags = PublishFlags {
		dry_run: false,
		git_enabled: true,
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	let outcome = GitReleaseOutcome {
		tags_created: 0,
		tags_skipped: 0,
		tags_push_failed: 1,
		releases_created: 0,
		releases_already_present: 0,
		forge_failed: false,
	};
	log_publish_summary(&state, &flags, &outcome);
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("tag") && m.contains("failed")),
		"Expected 'tag push(es) failed' in logs: {logs:?}"
	);
}

#[tokio::test]
async fn log_publish_summary_dry_run_no_tag_log_lines() {
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let state = PublishState::new();
	let flags = PublishFlags {
		dry_run: true, // dry-run: no tag log lines expected
		git_enabled: true,
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	let outcome = GitReleaseOutcome {
		tags_created: 3,
		tags_skipped: 0,
		tags_push_failed: 2,
		releases_created: 0,
		releases_already_present: 0,
		forge_failed: false,
	};
	log_publish_summary(&state, &flags, &outcome);
	let logs = crate::test_logging::take_logs();
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("created") && m.contains("tag")),
		"Should NOT log tag created count in dry-run: {logs:?}"
	);
	assert!(
		!logs
			.iter()
			.any(|(_, m)| m.contains("tag") && m.contains("failed")),
		"Should NOT log tag push failed in dry-run: {logs:?}"
	);
}

// ── log_forge_releases_summary tests ─────────────────────────────────────

#[tokio::test]
async fn log_forge_releases_summary_no_failure_logs_created_count() {
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	// Uses the GitLab label to guard against the summary regressing to a
	// hardcoded "GitHub Release" on non-GitHub forges.
	log_forge_releases_summary("GitLab", 3, 0, 0, "", &release_outcome(2, 0, false));
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("3 published") && m.contains("2 GitLab Release")),
		"Expected GitLab Release summary: {logs:?}"
	);
}

#[tokio::test]
async fn log_forge_releases_summary_with_failure_logs_failed_count() {
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	log_forge_releases_summary("GitHub", 3, 0, 0, "", &release_outcome(2, 0, true));
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter()
			.any(|(_, m)| m.contains("GitHub Release") && m.contains("failed")),
		"Expected GitHub Release failure count: {logs:?}"
	);
}

// ── PublishState::record_outcome tests ────────────────────────────────────

#[tokio::test]
async fn record_outcome_skipped_increments_skipped_count() {
	// Guards the `Skipped => self.skipped_count += 1` branch in non-dry-run mode.
	// Uses the NpmAdapter (default in new_test_with_runner) with EPUBLISHCONFLICT in
	// stderr to trigger PublishOutcome::AlreadyPublished → PublishResult::Skipped.
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	use std::sync::Arc;
	let runner = Arc::new(
		crate::command::test_support::RecordingCommandRunner::new(1)
			.with_stderr(b"npm ERR! code EPUBLISHCONFLICT".to_vec()),
	);
	let project = crate::package_manager::Project::new_test_with_runner(
		"pkg",
		"/nonexistent",
		Arc::clone(&runner),
	);
	let graph = make_graph(&[]);
	let mut state = PublishState::new();
	state.record_outcome(&project, &graph, false).await;
	assert_eq!(state.skipped_count, 1, "Expected skipped_count == 1");
	assert_eq!(
		state.published.len(),
		1,
		"Skipped package must be added to published so tags/releases are retried"
	);
}

// ── private_tagged_count tests ────────────────────────────────────────────

#[tokio::test]
async fn publish_state_private_tagged_count_starts_at_zero() {
	let state = PublishState::new();
	assert_eq!(state.private_tagged_count, 0);
}

#[tokio::test]
async fn log_forge_releases_summary_private_note_appears_when_nonzero() {
	// Guards `> 0` → `> 1` on `private_tagged_count` condition.
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	log_forge_releases_summary("GitHub", 2, 1, 0, "", &release_outcome(3, 0, false));
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter().any(|(_, m)| m.contains("private (tag only)")),
		"Expected private note in log: {logs:?}"
	);
}

#[tokio::test]
async fn log_forge_releases_summary_no_private_note_when_zero() {
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	log_forge_releases_summary("GitHub", 2, 0, 0, "", &release_outcome(2, 0, false));
	let logs = crate::test_logging::take_logs();
	assert!(
		!logs.iter().any(|(_, m)| m.contains("private (tag only)")),
		"Private note should not appear when count is zero: {logs:?}"
	);
}

#[tokio::test]
async fn log_summary_line_dry_run_shows_private_note() {
	// Guards `> 0` → `> 1` on `private_tagged_count` condition in log_summary_line.
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let mut state = PublishState::new();
	state.published.push(make_published_package());
	state.private_tagged_count = 1;
	let flags = PublishFlags {
		dry_run: true,
		git_enabled: false,
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	log_summary_line(&state, &flags, &make_empty_outcome());
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter().any(|(_, m)| m.contains("private (tag only)")),
		"Expected private note in dry-run summary: {logs:?}"
	);
}

#[tokio::test]
async fn log_summary_line_dry_run_registry_published_excludes_private() {
	// registry_published = published.len() - private_tagged_count
	// Guards `- private_tagged_count` being dropped (mutant: subtract 0 instead).
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let mut state = PublishState::new();
	// 1 registry publish + 1 private-tagged = 2 total in published
	state.published.push(make_published_package());
	state.published.push(make_published_package());
	state.private_tagged_count = 1;
	let flags = PublishFlags {
		dry_run: true,
		git_enabled: false,
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	log_summary_line(&state, &flags, &make_empty_outcome());
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter().any(|(_, m)| m.contains("1 would be published")),
		"Expected '1 would be published' (not 2) in summary: {logs:?}"
	);
}

#[tokio::test]
async fn log_summary_line_non_dry_run_shows_private_note() {
	// Guards `> 0` → `> 1` on `private_tagged_count` in non-dry-run path.
	crate::test_logging::init_test_logger();
	let _ = crate::test_logging::take_logs();
	let mut state = PublishState::new();
	state.published.push(make_published_package());
	state.private_tagged_count = 1;
	let flags = PublishFlags {
		dry_run: false,
		git_enabled: false,
		forge_enabled: false,
		no_git: false,
		is_multi_package: false,
		forge_name: "GitHub",
	};
	log_summary_line(&state, &flags, &make_empty_outcome());
	let logs = crate::test_logging::take_logs();
	assert!(
		logs.iter().any(|(_, m)| m.contains("private (tag only)")),
		"Expected private note in non-dry-run summary: {logs:?}"
	);
}

#[tokio::test]
async fn record_private_tagged_adds_to_published_and_increments_count() {
	// Guards the `record_private_tagged` method: package must be pushed to `published`
	// and `private_tagged_count` incremented, regardless of dry_run mode.
	use std::sync::Arc;
	let runner = Arc::new(
		crate::command::test_support::RecordingCommandRunner::new(0).with_stdout(b"1.0.0".to_vec()),
	);
	let project = crate::package_manager::Project::new_test_with_runner(
		"my-action",
		"/nonexistent",
		Arc::clone(&runner),
	);
	let mut state = PublishState::new();
	state.record_private_tagged(&project);
	assert_eq!(state.published.len(), 1, "Expected 1 package in published");
	assert_eq!(
		state.private_tagged_count, 1,
		"Expected private_tagged_count == 1"
	);
	assert_eq!(state.published[0].name, "my-action");
}

#[tokio::test]
async fn add_transitive_dependents_with_prepopulated_blocked_set() {
	// a -> b -> c: if blocked already contains "a" and we add dependents of b,
	// "a" should not be re-enqueued (returns false from insert) and the BFS still terminates.
	let graph = make_graph(&[("a", &["b"]), ("b", &["c"]), ("c", &[])]);
	let mut blocked = std::collections::HashSet::new();
	blocked.insert("a".to_string()); // pre-populated
	add_transitive_dependents(&graph, "c", &mut blocked);
	// b should be added (depends on c)
	assert!(blocked.contains("b"));
	// a was already present and should still be there
	assert!(blocked.contains("a"));
	// The already-present "a" entry must not cause re-enqueuing (BFS terminates)
}
