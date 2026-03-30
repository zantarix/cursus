use std::collections::BTreeMap;
use std::path::Path;

use crate::git::Git;
use crate::model::changelog::CommitReference;
use crate::model::changeset::{ChangeType, Changeset};
use crate::package_manager::validate_package_names;

use super::PackageChanges;

/// Aggregates changeset data into per-package maps, applying optional package filters.
///
/// Returns a tuple of:
/// - `aggregated`: the maximum `ChangeType` per package name
/// - `changes_per_package`: all `(ChangeType, message, commit_ref)` tuples per package name
pub(super) fn aggregate_changesets(
	changesets: &[(crate::path::AbsolutePath, Changeset)],
	package_filter: &[String],
	projects: &[crate::package_manager::Project],
	commit_refs: &BTreeMap<crate::path::AbsolutePath, Option<CommitReference>>,
) -> anyhow::Result<(
	BTreeMap<String, ChangeType>,
	BTreeMap<String, PackageChanges>,
)> {
	let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
	for (_, cs) in changesets {
		for (pkg, ct) in &cs.packages {
			aggregated
				.entry(pkg.clone())
				.and_modify(|e| *e = (*e).max(*ct))
				.or_insert(*ct);
		}
	}
	let mut changes_per_package: BTreeMap<String, PackageChanges> = BTreeMap::new();
	for (path, cs) in changesets {
		let commit_ref = commit_refs.get(path).and_then(|r| r.clone());
		for (pkg, ct) in &cs.packages {
			changes_per_package.entry(pkg.clone()).or_default().push((
				*ct,
				cs.message.clone(),
				commit_ref.clone(),
			));
		}
	}
	if !package_filter.is_empty() {
		validate_package_names(projects, package_filter)?;
		aggregated.retain(|name, _| package_filter.contains(name));
		changes_per_package.retain(|name, _| package_filter.contains(name));
	}
	Ok((aggregated, changes_per_package))
}

/// Resolves git commit references for each changeset file.
///
/// For each changeset path, looks up the commit that first added the file using
/// `git log --first-parent --diff-filter=A`. Extracts the PR number from the commit
/// subject line when available.
///
/// Never fails — always returns a map entry (possibly `None`) for every path.
pub(super) async fn resolve_commit_references(
	changesets: &[(crate::path::AbsolutePath, Changeset)],
	git: &dyn Git,
	git_enabled: bool,
) -> BTreeMap<crate::path::AbsolutePath, Option<CommitReference>> {
	if !git_enabled {
		log::debug!("Git disabled; skipping commit reference resolution");
		return changesets.iter().map(|(p, _)| (p.clone(), None)).collect();
	}

	let mut result = BTreeMap::new();
	for (path, _) in changesets {
		let commit_ref = resolve_one_commit_reference(path, git).await;
		result.insert(path.clone(), commit_ref);
	}
	result
}

/// Resolves the commit reference for a single changeset path.
///
/// Returns `None` on any failure or when the commit cannot be found,
/// logging warnings for unexpected errors.
pub(super) async fn resolve_one_commit_reference(
	path: &Path,
	git: &dyn Git,
) -> Option<CommitReference> {
	// Make the path relative to the git root for the git log command.
	let repo_root = git.path();
	let rel_path = path.strip_prefix(repo_root).unwrap_or(path);

	let sha = match git.log_added_commit(rel_path).await {
		Ok(Some(sha)) => sha,
		Ok(None) => {
			log::debug!("No introducing commit found for {}", path.display());
			return None;
		}
		Err(e) => {
			log::warn!("Failed to resolve commit for {}: {e:#}", path.display());
			return None;
		}
	};

	let subject = match git.log_subject(&sha).await {
		Ok(s) => s,
		Err(e) => {
			log::warn!("Failed to get commit subject for {sha}: {e:#}");
			return None;
		}
	};

	let commit_ref = CommitReference::new(&sha, &subject);
	if commit_ref.pr_number.is_none() {
		log::debug!("No PR number found in commit subject: {subject:?}");
	}
	Some(commit_ref)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::filesystem::LocalFilesystem;

	use super::*;

	fn make_runner() -> Arc<dyn CommandRunner> {
		Arc::new(RecordingCommandRunner::new(0))
	}

	// ── resolve_commit_references ─────────────────────────────────────────────

	#[tokio::test]
	async fn resolve_commit_references_git_disabled_does_not_call_git() {
		let dir = tempfile::tempdir().unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);

		// Create a fake changeset path
		let changeset_path =
			crate::path::AbsolutePath::new(dir.path().join(".cursus/test.md")).unwrap();
		let fake_cs = crate::model::changeset::Changeset {
			packages: std::collections::BTreeMap::new(),
			message: None,
		};
		let changesets = vec![(changeset_path.clone(), fake_cs)];
		let result = resolve_commit_references(&changesets, &git, false).await;
		assert_eq!(result.len(), 1);
		assert_eq!(result[&changeset_path], None);
		assert!(
			runner.invocations().is_empty(),
			"No git calls when disabled"
		);
	}

	#[tokio::test]
	async fn resolve_commit_references_git_enabled_no_commit_returns_none() {
		// When git log returns empty output, reference is None (no error).
		let dir = tempfile::tempdir().unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0)); // empty stdout
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);

		let changeset_path = crate::path::AbsolutePath::new(dir.path().join("test.md")).unwrap();
		let fake_cs = crate::model::changeset::Changeset {
			packages: std::collections::BTreeMap::new(),
			message: None,
		};
		let changesets = vec![(changeset_path.clone(), fake_cs)];
		let result = resolve_commit_references(&changesets, &git, true).await;
		assert_eq!(result[&changeset_path], None);
	}

	#[tokio::test]
	async fn resolve_commit_references_git_failure_is_nonfatal() {
		// A git failure should produce None, not propagate an error.
		let dir = tempfile::tempdir().unwrap();
		let runner =
			Arc::new(RecordingCommandRunner::new(1).with_stderr(b"fatal: not a git repo".to_vec()));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = crate::git::GitWorkdir::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			dir_abs.clone(),
		);

		let changeset_path = crate::path::AbsolutePath::new(dir.path().join("test.md")).unwrap();
		let fake_cs = crate::model::changeset::Changeset {
			packages: std::collections::BTreeMap::new(),
			message: None,
		};
		let changesets = vec![(changeset_path.clone(), fake_cs)];
		// Should not panic or return an error — just None
		let result = resolve_commit_references(&changesets, &git, true).await;
		assert_eq!(result[&changeset_path], None);
	}

	// ── aggregate_changesets ─────────────────────────────────────────────────

	fn make_changeset(
		pkg: &str,
		ct: crate::model::changeset::ChangeType,
	) -> crate::model::changeset::Changeset {
		let mut packages = std::collections::BTreeMap::new();
		packages.insert(pkg.to_string(), ct);
		crate::model::changeset::Changeset {
			packages,
			message: None,
		}
	}

	#[tokio::test]
	async fn aggregate_changesets_filter_retains_only_matching_packages() {
		let projects = vec![
			crate::package_manager::Project::new_test("alpha", "/nonexistent/alpha"),
			crate::package_manager::Project::new_test("beta", "/nonexistent/beta"),
		];
		let changesets = vec![
			(
				crate::path::AbsolutePath::new("/a.md").unwrap(),
				make_changeset("alpha", crate::model::changeset::ChangeType::Minor),
			),
			(
				crate::path::AbsolutePath::new("/b.md").unwrap(),
				make_changeset("beta", crate::model::changeset::ChangeType::Major),
			),
		];
		let commit_refs = BTreeMap::new();

		let (aggregated, changes) =
			aggregate_changesets(&changesets, &["alpha".to_string()], &projects, &commit_refs)
				.unwrap();

		assert!(aggregated.contains_key("alpha"), "alpha should be present");
		assert!(
			!aggregated.contains_key("beta"),
			"beta should be filtered out"
		);
		assert!(changes.contains_key("alpha"));
		assert!(!changes.contains_key("beta"));
	}

	fn make_test_env(dir: &std::path::Path) -> crate::Env {
		let r = Arc::new(crate::command::test_support::RecordingCommandRunner::new(0))
			as Arc<dyn CommandRunner>;
		crate::Env::new(
			Arc::clone(&r),
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(
				r,
				crate::path::AbsolutePath::new(dir).unwrap(),
			)),
		)
	}

	#[tokio::test]
	async fn aggregate_changesets_unknown_package_filter_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let setup_env = make_test_env(dir.path());
		crate::model::config::Config::new()
			.with_cargo(crate::model::config::CargoConfig::enabled())
			.save(setup_env.fs(), setup_env.git().path())
			.await
			.unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"my-pkg\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();

		let runner = make_runner();
		let path = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(runner, path)),
		);
		let config = crate::model::config::load(env.fs(), env.git().path())
			.await
			.unwrap()
			.unwrap();
		let adapters = config.create_adapters(&env).unwrap();
		let projects = config.load_projects_for_adapters(&adapters).await.unwrap();

		let commit_refs = BTreeMap::new();
		let result =
			aggregate_changesets(&[], &["nonexistent".to_string()], &projects, &commit_refs);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("nonexistent"));
	}

	// ── aggregate_changesets with commit_refs ─────────────────────────────────

	#[tokio::test]
	async fn aggregate_changesets_with_empty_refs_produces_none_references() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let setup_env = make_test_env(dir.path());
		crate::model::config::Config::new()
			.with_cargo(crate::model::config::CargoConfig::enabled())
			.save(setup_env.fs(), setup_env.git().path())
			.await
			.unwrap();

		let path = crate::path::AbsolutePath::new("/test.md").unwrap();
		let mut pkgs = std::collections::BTreeMap::new();
		pkgs.insert(
			"my-pkg".to_string(),
			crate::model::changeset::ChangeType::Minor,
		);
		let cs = crate::model::changeset::Changeset {
			packages: pkgs,
			message: Some("A feature".to_string()),
		};
		let changesets = vec![(path.clone(), cs)];
		let commit_refs = BTreeMap::new(); // empty refs → all None

		let runner = make_runner();
		let path = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(runner, path)),
		);
		let config = crate::model::config::load(env.fs(), env.git().path())
			.await
			.unwrap()
			.unwrap();
		let adapters = config.create_adapters(&env).unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"my-pkg\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();
		let projects = config.load_projects_for_adapters(&adapters).await.unwrap();

		let (_, changes_per_package) =
			aggregate_changesets(&changesets, &[], &projects, &commit_refs).unwrap();
		let changes = changes_per_package.get("my-pkg").unwrap();
		assert_eq!(changes.len(), 1);
		let (_, _, commit_ref) = &changes[0];
		assert_eq!(*commit_ref, None, "Expected None commit reference");
	}
}
