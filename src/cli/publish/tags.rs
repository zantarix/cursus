//! Git tag creation and pushing for published packages.

use log::{info, warn};

use crate::git::Git;
use crate::model::config::Config;

use super::PublishedPackage;

/// Creates and pushes an annotated git tag for each published package serially.
///
/// Tags that already exist in the repository are skipped (making the operation idempotent).
/// Each tag is pushed immediately after creation. If a push fails, the local tag is deleted
/// so that a retry can re-create and re-push it, then processing continues with the next tag.
///
/// Returns `(tags_created, tags_skipped, tags_push_failed)`.
pub(super) fn create_and_push_tags(
	published: &[PublishedPackage],
	config: &Config,
	git: &dyn Git,
	is_multi_package: bool,
) -> anyhow::Result<(usize, usize, usize)> {
	let mut created = 0;
	let mut skipped = 0;
	let mut push_failed = 0;

	for pkg in published {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);

		if git.tag_exists(&tag)? {
			info!("Tag {tag} already exists, skipping");
			skipped += 1;
			continue;
		}

		let message = format!("Release {} version {}", pkg.name, pkg.version);
		// Tag creation failure is propagated as a hard error: an `Err` from `git tag -a`
		// typically indicates a deeper problem (corrupt repo, permission denied) that
		// cannot be resolved by retrying. Push failures, by contrast, are often transient
		// and are handled with best-effort cleanup so retries can succeed.
		git.tag(&tag, &message)?;
		info!("Created tag {tag}");

		if let Err(e) = git.push_tag(&tag) {
			warn!("Failed to push tag {tag}: {e:#}");
			if let Err(del_err) = git.delete_tag(&tag) {
				warn!("Failed to delete local tag {tag} after push failure: {del_err:#}");
			}
			push_failed += 1;
		} else {
			created += 1;
		}
	}

	if created > 0 {
		info!(
			"Pushed {} tag{} to origin",
			created,
			if created == 1 { "" } else { "s" }
		);
	}

	Ok((created, skipped, push_failed))
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;
	use crate::cli::publish::tests_common::{make_github_config, workdir};
	use crate::command::CommandRunner;
	use crate::command::test_support::{DispatchingCommandRunner, RecordingCommandRunner};
	use crate::filesystem::LocalFilesystem;
	use crate::model::config::Config;
	use crate::path::AbsolutePath;

	#[test]
	fn create_and_push_tags_creates_annotated_tags_and_pushes() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// rev-parse exits 1 → tag_exists returns false (no existing tag)
		// all other git commands succeed via default exit 0
		let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args(
			"git",
			vec!["rev-parse".to_string(), "--verify".to_string()],
			1,
		));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let (created, skipped, push_failed) =
			create_and_push_tags(&published, &config, &git, false).unwrap();

		assert_eq!(created, 1);
		assert_eq!(skipped, 0);
		assert_eq!(push_failed, 0);
		let invocations = runner.invocations();
		// rev-parse (exists check), tag -a (create), push origin <tag>
		assert_eq!(invocations.len(), 3);
		assert!(invocations[0].args.contains(&"rev-parse".to_string()));
		assert!(invocations[1].args.contains(&"-a".to_string()));
		// Pushes the specific tag, not all tags
		assert!(
			invocations[2].args.contains(&"v1.2.0".to_string()),
			"Expected specific tag push, got: {:?}",
			invocations[2].args
		);
		assert!(
			!invocations[2].args.contains(&"--tags".to_string()),
			"Should not push --tags (all local tags)"
		);
	}

	#[test]
	fn create_and_push_tags_skips_existing_tag() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// rev-parse exits 0 → tag_exists returns true (tag already exists)
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let (created, skipped, push_failed) =
			create_and_push_tags(&published, &config, &git, false).unwrap();

		assert_eq!(created, 0);
		assert_eq!(skipped, 1);
		assert_eq!(push_failed, 0);
		// Only the rev-parse check; no tag creation or push
		let invocations = runner.invocations();
		assert_eq!(invocations.len(), 1);
		assert!(invocations[0].args.contains(&"rev-parse".to_string()));
	}

	#[test]
	fn create_and_push_tags_empty_list_does_nothing() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);

		let (created, skipped, push_failed) =
			create_and_push_tags(&[], &config, &git, false).unwrap();

		assert_eq!(created, 0);
		assert_eq!(skipped, 0);
		assert_eq!(push_failed, 0);
		assert!(runner.invocations().is_empty());
	}

	#[test]
	fn create_and_push_tags_does_not_log_when_nothing_created() {
		// When no packages are published the "Pushed N tag(s)" info line must NOT appear.
		// This guards against mutations that make the `if created > 0` guard always true
		// (e.g. replace `>` with `>=`), which would log a misleading message for 0 tags.
		use crate::test_logging::{init_test_logger, take_logs};
		init_test_logger();
		let _ = take_logs();

		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, dir_abs);

		let (created, skipped, push_failed) =
			create_and_push_tags(&[], &config, &git, false).unwrap();
		assert_eq!(created, 0);
		assert_eq!(skipped, 0);
		assert_eq!(push_failed, 0);

		let logs = take_logs();
		assert!(
			!logs
				.iter()
				.any(|(_, m)| m.contains("Pushed") && m.contains("tag")),
			"Should not log a 'Pushed N tag(s)' message when nothing was created, got: {logs:?}"
		);
	}

	#[test]
	fn create_and_push_tags_logs_when_tags_created() {
		// When a tag IS created the "Pushed N tag(s)" info line MUST appear.
		// This guards against mutations that make `if created > 0` always false
		// (e.g. replace `>` with `<`), which would suppress the log even when tags
		// were actually pushed.
		use crate::test_logging::{init_test_logger, take_logs};
		init_test_logger();
		let _ = take_logs();

		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// rev-parse exits 1 → tag_exists returns false → tag gets created
		let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args(
			"git",
			vec!["rev-parse".to_string(), "--verify".to_string()],
			1,
		));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git =
			crate::git::GitWorkdir::new(Arc::clone(&runner) as Arc<dyn CommandRunner>, dir_abs);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.2.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let (created, skipped, push_failed) =
			create_and_push_tags(&published, &config, &git, false).unwrap();
		assert_eq!(created, 1);
		assert_eq!(skipped, 0);
		assert_eq!(push_failed, 0);

		let logs = take_logs();
		assert!(
			logs.iter()
				.any(|(_, m)| m.contains("Pushed") && m.contains("tag")),
			"Should log 'Pushed N tag(s)' when tags were created, got: {logs:?}"
		);
	}

	#[test]
	fn create_and_push_tags_uses_prefixed_tag_for_monorepo() {
		// Verifies that the `my-app@2.0.0` prefix format is used throughout
		// the full create-and-push flow, not just the existence check.
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// rev-parse exits 1 (tag absent) so tag creation and push are exercised.
		let runner = Arc::new(DispatchingCommandRunner::new(0).on_with_args(
			"git",
			vec!["rev-parse".to_string(), "--verify".to_string()],
			1,
		));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "2.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		create_and_push_tags(&published, &config, &git, true).unwrap();

		let invocations = runner.invocations();
		// rev-parse ref, tag -a create, push — all must use the prefixed name
		assert!(
			invocations[0]
				.args
				.contains(&"refs/tags/my-app@2.0.0".to_string()),
			"Expected monorepo ref in rev-parse, got: {:?}",
			invocations[0].args
		);
		assert!(
			invocations[1].args.contains(&"my-app@2.0.0".to_string()),
			"Expected monorepo tag name in tag -a, got: {:?}",
			invocations[1].args
		);
		assert!(
			invocations[2].args.contains(&"my-app@2.0.0".to_string()),
			"Expected monorepo tag name in push, got: {:?}",
			invocations[2].args
		);
	}

	#[test]
	fn create_and_push_tags_push_failure_deletes_local_tag_and_counts_failed() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// rev-parse exits 1 (tag absent); push of v1.0.0 also fails; all else succeeds.
		let runner = Arc::new(
			DispatchingCommandRunner::new(0)
				.on_with_args(
					"git",
					vec!["rev-parse".to_string(), "--verify".to_string()],
					1,
				)
				.on_with_args(
					"git",
					vec![
						"push".to_string(),
						"origin".to_string(),
						"tag".to_string(),
						"v1.0.0".to_string(),
					],
					1,
				),
		);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		let (created, skipped, push_failed) =
			create_and_push_tags(&published, &config, &git, false).unwrap();

		assert_eq!(created, 0);
		assert_eq!(skipped, 0);
		assert_eq!(push_failed, 1);

		let invocations = runner.invocations();
		// rev-parse (exists check), tag -a (create), push origin <tag> (fails), tag -d (cleanup)
		assert_eq!(invocations.len(), 4);
		assert!(invocations[0].args.contains(&"rev-parse".to_string()));
		assert!(invocations[1].args.contains(&"-a".to_string()));
		assert!(invocations[2].args.contains(&"push".to_string()));
		// Cleanup: git tag -d
		assert_eq!(invocations[3].args[0], "tag");
		assert_eq!(invocations[3].args[1], "-d");
		assert!(invocations[3].args.contains(&"v1.0.0".to_string()));
	}

	#[test]
	fn create_and_push_tags_push_failure_continues_to_next_package() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// rev-parse exits 1 (both tags absent); only the first push (v1.0.0) fails.
		let runner = Arc::new(
			DispatchingCommandRunner::new(0)
				.on_with_args(
					"git",
					vec!["rev-parse".to_string(), "--verify".to_string()],
					1,
				)
				.on_with_args(
					"git",
					vec![
						"push".to_string(),
						"origin".to_string(),
						"tag".to_string(),
						"v1.0.0".to_string(),
					],
					1,
				),
		);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let published = vec![
			PublishedPackage {
				name: "pkg-a".to_string(),
				version: "1.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
			PublishedPackage {
				name: "pkg-b".to_string(),
				version: "2.0.0".parse().unwrap(),
				project_path: AbsolutePath::new("/nonexistent").unwrap(),
			},
		];

		let (created, skipped, push_failed) =
			create_and_push_tags(&published, &config, &git, false).unwrap();

		// First package push failed, second succeeded.
		assert_eq!(created, 1);
		assert_eq!(skipped, 0);
		assert_eq!(push_failed, 1);
	}

	#[test]
	fn create_and_push_tags_delete_failure_after_push_failure_is_non_fatal() {
		let dir = tempfile::tempdir().unwrap();
		let config = Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap());
		// rev-parse exits 1 (tag absent); push and delete also fail.
		let runner = Arc::new(
			DispatchingCommandRunner::new(0)
				.on_with_args(
					"git",
					vec!["rev-parse".to_string(), "--verify".to_string()],
					1,
				)
				.on_with_args("git", vec!["push".to_string()], 1)
				.on_with_args("git", vec!["tag".to_string(), "-d".to_string()], 1),
		);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);
		let published = vec![PublishedPackage {
			name: "my-app".to_string(),
			version: "1.0.0".parse().unwrap(),
			project_path: AbsolutePath::new("/nonexistent").unwrap(),
		}];

		// Must not error out even though both push and delete failed.
		let result = create_and_push_tags(&published, &config, &git, false);
		assert!(result.is_ok());
		let (created, skipped, push_failed) = result.unwrap();
		assert_eq!(created, 0);
		assert_eq!(skipped, 0);
		assert_eq!(push_failed, 1);
	}

	// Suppress unused import warning for make_github_config which is used in other test modules
	#[allow(dead_code)]
	fn _use_make_github_config() {
		let _ = make_github_config("", std::collections::BTreeMap::new());
		let _ = workdir();
	}
}
