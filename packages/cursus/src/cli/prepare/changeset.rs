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
pub(crate) fn aggregate_changesets(
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
pub(crate) async fn resolve_commit_references(
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
