//! The `prepare` subcommand.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use clap::Args;
use log::info;

use semver::Version;

use crate::git;
use crate::github::GitHubRepo;
use crate::github::client::GitHubClient;
use crate::model::changelog::{Changelog, CommitReference};
use crate::model::changeset::{ChangeType, Changeset};
use crate::model::config::{Config, DependencyBump, Strategy};
use crate::package_manager::{PackageManagerAdapter, Project, filter_projects_by_name};
use crate::utils::today_iso_date;

/// Arguments for the `prepare` subcommand.
#[derive(Args, Default)]
pub struct PrepareArgs {
	/// Only prepare specific packages (repeatable)
	#[arg(short = 'p', long = "package")]
	pub packages: Vec<String>,

	/// Skip git lifecycle automation even if enabled in config
	#[arg(long)]
	pub no_git: bool,

	/// Override the release branch name (branch strategy only).
	///
	/// If not provided, the branch name is derived from `release_branch_prefix`
	/// in the config plus the current branch name.
	#[arg(long)]
	pub branch: Option<String>,
}

/// Bumps a semver version according to the given change type.
fn bump_version(version: &semver::Version, change_type: ChangeType) -> semver::Version {
	let mut v = version.clone();
	match change_type {
		ChangeType::Major => {
			v.major += 1;
			v.minor = 0;
			v.patch = 0;
		}
		ChangeType::Minor => {
			v.minor += 1;
			v.patch = 0;
		}
		ChangeType::Patch => {
			v.patch += 1;
		}
	}
	v.pre = semver::Prerelease::EMPTY;
	v.build = semver::BuildMetadata::EMPTY;
	v
}

/// Checks that the working tree is clean before making changes.
///
/// # Errors
///
/// Returns an error if the working tree has uncommitted changes.
fn check_dirty_tree(git: &git::GitWorkdir) -> anyhow::Result<()> {
	let status = git.status_porcelain()?;
	if !status.trim().is_empty() {
		anyhow::bail!(
			"Working tree is dirty. Commit or stash changes before releasing.\n\
			 Run `git status` to see pending changes."
		);
	}
	Ok(())
}

/// Computes the release branch name from CLI flags, config, and current branch.
///
/// Priority order:
/// 1. `args_branch` — explicit `--branch` flag
/// 2. `{config_prefix}{current_branch}` — derived from config prefix and current branch
/// 3. `{config_prefix}detached` — fallback when HEAD is detached
fn compute_release_branch(
	args_branch: Option<&str>,
	config_prefix: &str,
	current_branch: Option<&str>,
) -> String {
	if let Some(branch) = args_branch {
		return branch.to_string();
	}
	let base = current_branch.unwrap_or("detached");
	format!("{config_prefix}{base}")
}

/// Information about a single package prepared for release.
#[derive(Debug)]
struct ReleaseInfo {
	package_name: String,
	new_version: Version,
	/// Formatted changelog sections (### headings + bullets) without the version heading.
	changelog_entry: String,
}

/// Formats the git commit message for a prepare run.
///
/// Produces `chore(release): pkg1@1.0.0, pkg2@2.0.0` for the given releases.
///
/// # Note
///
/// `release_infos` must be non-empty; passing an empty slice produces the
/// malformed string `"chore(release): "`. This is guaranteed by the early
/// return in [`stage_and_commit`] which skips all git operations when there
/// are no releases.
fn format_commit_message(release_infos: &[ReleaseInfo]) -> String {
	let parts: Vec<String> = release_infos
		.iter()
		.map(|r| format!("{}@{}", r.package_name, r.new_version))
		.collect();
	format!("chore(release): {}", parts.join(", "))
}

/// Stages files and creates a commit for the prepare step.
///
/// This is the core git operation for `prepare` — it only commits.
/// Pushing is handled by the strategy dispatch in `cmd_prepare`, and tagging
/// happens in `publish`.
///
/// If `dry_run` is `true`, prints what would be done without executing any git commands.
///
/// # Arguments
///
/// * `git` - Git working directory with command runner.
/// * `extra_files` - Additional files to unconditionally stage, relative to the git root.
/// * `release_infos` - The packages that were prepared for release.
/// * `modified_files` - Files to stage before committing.
/// * `dry_run` - If `true`, only print a summary; do not modify git state.
///
/// # Errors
///
/// Returns an error if any git command fails.
fn stage_and_commit(
	git: &git::GitWorkdir,
	extra_files: &[String],
	release_infos: &[ReleaseInfo],
	modified_files: &[PathBuf],
) -> anyhow::Result<()> {
	if release_infos.is_empty() {
		return Ok(());
	}

	let commit_message = format_commit_message(release_infos);

	// Build the full staging list, validating that extra_files resolve inside the repo root.
	let git_workdir = git.path();
	let canonical_workdir = std::fs::canonicalize(git_workdir)
		.with_context(|| format!("failed to canonicalize git workdir {:?}", git_workdir))?;
	let mut all_files = modified_files.to_vec();
	for f in extra_files {
		let full_path = git_workdir.join(f);
		let resolved = match std::fs::canonicalize(&full_path) {
			Ok(p) => p,
			Err(_) => {
				log::warn!("extra_files entry {:?} does not exist, skipping", f);
				continue;
			}
		};
		if !resolved.starts_with(&canonical_workdir) {
			anyhow::bail!(
				"extra_files entry {:?} resolves outside the repository root",
				f
			);
		}
		all_files.push(resolved);
	}

	git.add(&all_files)
		.context("Failed to stage files for git commit")?;
	git.commit(&commit_message)
		.context("Failed to create git commit")?;

	Ok(())
}

/// Creates or updates a pull request for the given head branch.
///
/// If an open pull request already exists for `head`, it is updated with the
/// new `title` and `body`. Otherwise a new pull request is created from `head`
/// into `base`. Returns the URL of the created or updated pull request.
///
/// # Errors
///
/// Returns an error if the find, create, or update API call fails.
fn upsert_pull_request(
	client: &dyn GitHubClient,
	gh_repo: &GitHubRepo,
	title: &str,
	body: &str,
	head: &str,
	base: &str,
) -> anyhow::Result<String> {
	match client.find_open_pull_request(gh_repo, head)? {
		Some(pr) => {
			let url = client.update_pull_request(gh_repo, pr.number, title, body)?;
			info!("Updated pull request: {url}");
			Ok(url)
		}
		None => {
			let url = client.create_pull_request(gh_repo, title, body, head, base)?;
			info!("Created pull request: {url}");
			Ok(url)
		}
	}
}

/// Builds the pull request body with an introduction and per-package changelog sections.
fn build_pr_body(releases: &[ReleaseInfo], base_branch: &str) -> String {
	let mut body = format!(
		"This PR was opened by Cursus. When ready to release, you should merge this PR \
		 which will trigger a release. If you're not ready to do a release then simply leave \
		 this PR and it will be updated as you merge more changesets into `{base_branch}`.\n\
		 \n\
		 # Releases\n"
	);
	for r in releases {
		let _ = write!(
			body,
			"\n## {}@{}\n\n{}",
			r.package_name, r.new_version, r.changelog_entry
		);
	}
	body
}

/// Per-package changelog entries: `(ChangeType, Option<message>, Option<CommitReference>)` tuples.
type PackageChanges = Vec<(ChangeType, Option<String>, Option<CommitReference>)>;

/// Aggregates changeset data into per-package maps, applying optional package filters.
///
/// Returns a tuple of:
/// - `aggregated`: the maximum `ChangeType` per package name
/// - `changes_per_package`: all `(ChangeType, message, commit_ref)` tuples per package name
fn aggregate_changesets(
	changesets: &[(PathBuf, Changeset)],
	package_filter: &[String],
	projects: &[crate::package_manager::Project],
	commit_refs: &BTreeMap<PathBuf, Option<CommitReference>>,
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
		filter_projects_by_name(projects, package_filter)?;
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
fn resolve_commit_references(
	changesets: &[(PathBuf, Changeset)],
	git: &git::GitWorkdir,
	git_enabled: bool,
) -> BTreeMap<PathBuf, Option<CommitReference>> {
	if !git_enabled {
		log::debug!("Git disabled; skipping commit reference resolution");
		return changesets.iter().map(|(p, _)| (p.clone(), None)).collect();
	}

	changesets
		.iter()
		.map(|(path, _)| {
			let commit_ref = resolve_one_commit_reference(path, git);
			(path.clone(), commit_ref)
		})
		.collect()
}

/// Resolves the commit reference for a single changeset path.
///
/// Returns `None` on any failure or when the commit cannot be found,
/// logging warnings for unexpected errors.
fn resolve_one_commit_reference(path: &Path, git: &git::GitWorkdir) -> Option<CommitReference> {
	// Make the path relative to the git root for the git log command.
	let repo_root = git.path();
	let rel_path = path.strip_prefix(repo_root).unwrap_or(path);

	let sha = match git.log_added_commit(rel_path) {
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

	let subject = match git.log_subject(&sha) {
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

/// Runs pre-release checks and sets up the git branch if needed.
///
/// Validates GitHub token availability when required, checks for a dirty working
/// tree, and checks out the release branch for the branch strategy.
/// Returns `(original_branch, release_branch)`.
fn preflight_checks(
	git: &git::GitWorkdir,
	config: &Config,
	env: &crate::Env,
	args: &PrepareArgs,
	git_enabled: bool,
	strategy: Strategy,
	dry_run: bool,
) -> anyhow::Result<(Option<String>, Option<String>)> {
	if git_enabled
		&& strategy == Strategy::Branch
		&& config.github.enabled
		&& !dry_run
		&& env.github_client().is_none()
	{
		anyhow::bail!(
			"GitHub integration is enabled but no GitHub token found. \
			 Set GH_TOKEN or GITHUB_TOKEN environment variable."
		);
	}
	if !git_enabled {
		return Ok((None, None));
	}
	if !dry_run {
		check_dirty_tree(git)?;
	}
	if strategy == Strategy::Branch {
		let current = if dry_run {
			git.current_branch().ok().flatten()
		} else {
			git.current_branch()?
		};
		let branch = compute_release_branch(
			args.branch.as_deref(),
			config.git.release_branch_prefix(),
			current.as_deref(),
		);
		git.checkout_or_reset_branch(&branch)?;
		Ok((current, Some(branch)))
	} else {
		Ok((None, None))
	}
}

/// Infers the [`ChangeType`] needed to move from `old` to `new`.
///
/// Returns the highest change type implied by the version difference:
/// - Major if the major component changed
/// - Minor if only the minor component changed
/// - Patch otherwise
fn infer_change_type(old: &Version, new: &Version) -> ChangeType {
	if new.major != old.major {
		ChangeType::Major
	} else if new.minor != old.minor {
		ChangeType::Minor
	} else {
		ChangeType::Patch
	}
}

/// Computes the effective new version for a package given its current state.
///
/// Checks, in priority order:
/// 1. `version_overrides` (linked-version reconciliation)
/// 2. `aggregated` (changeset-driven bump)
/// 3. `propagation_map` (propagation-driven bump, complete after phase 1)
///
/// When called during the sweep phase, `aggregated` is being mutated. This
/// function is correct because both the `aggregated` fallback (catches packages
/// already updated earlier in the loop, ordered by BTreeMap iteration) and the
/// `propagation_map` fallback (contains the complete phase-1 result) cover all
/// upstream packages.
fn effective_new_version(
	pkg_name: &str,
	projects: &[Project],
	aggregated: &BTreeMap<String, ChangeType>,
	version_overrides: &BTreeMap<String, Version>,
	propagation_map: &BTreeMap<String, (ChangeType, Vec<String>)>,
) -> Option<Version> {
	if let Some(v) = version_overrides.get(pkg_name) {
		return Some(v.clone());
	}
	let ct = aggregated
		.get(pkg_name)
		.copied()
		.or_else(|| propagation_map.get(pkg_name).map(|(ct, _)| *ct))?;
	let project = projects.iter().find(|p| p.name() == pkg_name)?;
	Some(bump_version(project.version(), ct))
}

/// Builds a reverse dependency graph for intra-workspace dependencies.
///
/// Returns a map from each package name to the list of packages that depend on it.
fn build_reverse_dep_graph(projects: &[Project]) -> BTreeMap<String, Vec<String>> {
	let project_names: BTreeSet<String> = projects.iter().map(|p| p.name().to_string()).collect();
	let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
	for project in projects {
		for dep_name in project.dependency_names() {
			if project_names.contains(dep_name.as_str()) {
				reverse_deps
					.entry(dep_name.clone())
					.or_default()
					.push(project.name().to_string());
			}
		}
	}
	reverse_deps
}

/// Phase 1 of dependency propagation: marks all transitively dependent packages.
///
/// Starting from the initially-bumped set (`aggregated`), traverses the reverse
/// dependency graph and returns a map of `pkg_name → (effective_ct, [upstream_names])`.
/// Packages in `version_overrides` (linked-version bumps) are exempt.
fn mark_propagation_bumps(
	aggregated: &BTreeMap<String, ChangeType>,
	version_overrides: &BTreeMap<String, Version>,
	reverse_deps: &BTreeMap<String, Vec<String>>,
	dep_bump: DependencyBump,
) -> BTreeMap<String, (ChangeType, Vec<String>)> {
	let mut queue: VecDeque<(String, ChangeType)> = aggregated
		.iter()
		.map(|(name, &ct)| (name.clone(), ct))
		.collect();
	let mut propagation_map: BTreeMap<String, (ChangeType, Vec<String>)> = BTreeMap::new();

	while let Some((bumped_name, upstream_ct)) = queue.pop_front() {
		let effective_ct = dep_bump.to_change_type(upstream_ct);
		let Some(dependents) = reverse_deps.get(&bumped_name) else {
			continue;
		};
		for dependent_name in dependents {
			if version_overrides.contains_key(dependent_name.as_str()) {
				continue; // Linked packages are exempt from propagation.
			}
			let current_ct = aggregated
				.get(dependent_name.as_str())
				.copied()
				.or_else(|| {
					propagation_map
						.get(dependent_name.as_str())
						.map(|(ct, _)| *ct)
				});
			if current_ct.is_some_and(|c| c >= effective_ct) {
				continue; // Already at a sufficient bump level.
			}
			let entry = propagation_map
				.entry(dependent_name.clone())
				.or_insert_with(|| (effective_ct, Vec::new()));
			entry.0 = effective_ct;
			entry.1.push(bumped_name.clone());
			queue.push_back((dependent_name.clone(), effective_ct));
		}
	}
	propagation_map
}

/// Writes or logs a changeset for an out-of-scope dependent package.
fn write_out_of_scope_changeset(
	pkg_name: &str,
	effective_ct: ChangeType,
	dep_msgs: &[String],
	git: &git::GitWorkdir,
	dry_run: bool,
) -> anyhow::Result<Option<PathBuf>> {
	let message = format!("Dependency updates: {}", dep_msgs.join(", "));
	let mut packages = BTreeMap::new();
	packages.insert(pkg_name.to_string(), effective_ct);
	let changeset = Changeset::new(packages, Some(message));
	if dry_run {
		info!(
			"Would write dependency propagation changeset for \
			 out-of-scope package '{pkg_name}' ({effective_ct})"
		);
		return Ok(None);
	}
	let path = changeset
		.write(git)
		.with_context(|| format!("Failed to write propagation changeset for '{pkg_name}'"))?;
	info!(
		"Wrote dependency propagation changeset for '{pkg_name}': {}",
		path.display()
	);
	Ok(Some(path))
}

/// `(dep_entries_per_package, new_changeset_paths)` returned by [`apply_dependency_propagation`].
type PropagationResult = (BTreeMap<String, Vec<String>>, Vec<PathBuf>);

/// Applies dependency propagation bumps (ADR-023).
///
/// Walks the intra-workspace dependency graph using a two-phase mark-then-sweep
/// algorithm. In-scope packages have their entry in `aggregated` updated; out-of-scope
/// dependents receive a newly written changeset file in `.cursus/`.
///
/// Returns `(dep_entries_per_package, new_changeset_paths)` where:
/// - `dep_entries_per_package`: human-readable dependency update messages per in-scope
///   package, for rendering in the `### Dependencies` changelog section.
/// - `new_changeset_paths`: paths of changeset files written for out-of-scope packages.
///
/// # Errors
///
/// Returns an error if writing a changeset file for an out-of-scope dependent fails.
fn apply_dependency_propagation(
	projects: &[Project],
	aggregated: &mut BTreeMap<String, ChangeType>,
	version_overrides: &BTreeMap<String, Version>,
	package_filter: &[String],
	dep_bump: DependencyBump,
	git: &git::GitWorkdir,
	dry_run: bool,
) -> anyhow::Result<PropagationResult> {
	let reverse_deps = build_reverse_dep_graph(projects);
	let propagation_map =
		mark_propagation_bumps(aggregated, version_overrides, &reverse_deps, dep_bump);
	if propagation_map.is_empty() {
		return Ok((BTreeMap::new(), Vec::new()));
	}

	let mut dep_entries: BTreeMap<String, Vec<String>> = BTreeMap::new();
	let mut new_changeset_paths: Vec<PathBuf> = Vec::new();

	for (pkg_name, (effective_ct, upstream_names)) in &propagation_map {
		let dep_msgs: Vec<String> = upstream_names
			.iter()
			.map(|up| {
				match effective_new_version(
					up,
					projects,
					aggregated,
					version_overrides,
					&propagation_map,
				) {
					Some(v) => format!("`{up}` bumped to {v}"),
					None => format!("`{up}` bumped"),
				}
			})
			.collect();

		if package_filter.is_empty() || package_filter.contains(pkg_name) {
			let existing_ct = aggregated.get(pkg_name.as_str()).copied();
			if existing_ct.is_none_or(|c| c < *effective_ct) {
				aggregated.insert(pkg_name.clone(), *effective_ct);
				dep_entries.insert(pkg_name.clone(), dep_msgs);
				info!(
					"{pkg_name}: dependency propagation bump ({effective_ct}) from {}",
					upstream_names.join(", ")
				);
			}
		} else if let Some(path) =
			write_out_of_scope_changeset(pkg_name, *effective_ct, &dep_msgs, git, dry_run)?
		{
			new_changeset_paths.push(path);
		}
	}

	Ok((dep_entries, new_changeset_paths))
}

/// Validates that a scoped prepare does not partially overlap any linked group.
///
/// Returns an error if `--package` includes some but not all packages from a
/// linked group, which would break the version-sync invariant. Full group
/// inclusion or full group exclusion are both fine.
///
/// For global linking (`package_filter` non-empty with no groups, i.e., all
/// packages linked), any `--package` filter is rejected outright because there
/// is no valid subset.
///
/// # Errors
///
/// Returns an error when partial overlap is detected or global linking is
/// active with a package filter.
fn validate_scoped_prepare_linked_groups(
	package_filter: &[String],
	linked_groups: &[Vec<String>],
	is_global: bool,
) -> anyhow::Result<()> {
	if package_filter.is_empty() {
		return Ok(());
	}

	if is_global {
		anyhow::bail!(
			"Cannot use --package with global linked-versions (enabled = true with no groups). \
			 All packages must be prepared together when globally linked."
		);
	}

	for group in linked_groups {
		let in_scope: Vec<&String> = group
			.iter()
			.filter(|p| package_filter.contains(p))
			.collect();
		let out_of_scope: Vec<&String> = group
			.iter()
			.filter(|p| !package_filter.contains(p))
			.collect();

		if !in_scope.is_empty() && !out_of_scope.is_empty() {
			let group_list = group.join(", ");
			let missing_list: Vec<&str> = out_of_scope.iter().map(|s| s.as_str()).collect();
			anyhow::bail!(
				"--package scope partially overlaps a linked-versions group [{group_list}]. \
				 Missing packages: {}. \
				 Include all packages from the group or exclude all of them.",
				missing_list.join(", ")
			);
		}
	}

	Ok(())
}

/// Computes the final target version for all packages in a linked group.
///
/// The algorithm:
/// 1. Find the maximum **current** version across all packages in the group.
/// 2. Find the highest `ChangeType` from any pending changeset in the group.
/// 3. If any changeset exists, apply it to the group max: `bump_version(max_current, highest_ct)`.
///    Otherwise the group max itself is the target (pure sync, no increment).
///
/// This ensures that a changeset on any package always advances the group,
/// even when another package already holds a higher current version.
fn compute_group_final_version(
	group: &[String],
	aggregated: &BTreeMap<String, ChangeType>,
	projects: &[Project],
) -> Option<Version> {
	let mut max_current: Option<Version> = None;
	let mut highest_ct: Option<ChangeType> = None;
	for pkg_name in group {
		let Some(project) = projects.iter().find(|p| p.name() == pkg_name) else {
			continue;
		};
		let current = project.version().clone();
		max_current = Some(match max_current {
			Some(c) => c.max(current),
			None => current,
		});
		if let Some(&ct) = aggregated.get(pkg_name) {
			highest_ct = Some(match highest_ct {
				Some(h) => h.max(ct),
				None => ct,
			});
		}
	}
	let max_current = max_current?;
	Some(match highest_ct {
		Some(ct) => bump_version(&max_current, ct),
		None => max_current,
	})
}

/// Promotes a no-changeset package to `final_version`, inserting a sync changelog entry.
fn promote_package_to_final(
	pkg_name: &str,
	final_version: &Version,
	aggregated: &mut BTreeMap<String, ChangeType>,
	changes_per_package: &mut BTreeMap<String, PackageChanges>,
	version_overrides: &mut BTreeMap<String, Version>,
	projects: &[Project],
) {
	let Some(project) = projects.iter().find(|p| p.name() == pkg_name) else {
		log::warn!(
			"Package '{pkg_name}' in linked group not found in projects; skipping version sync"
		);
		return;
	};
	let sync_ct = infer_change_type(project.version(), final_version);
	aggregated.insert(pkg_name.to_string(), sync_ct);
	changes_per_package
		.entry(pkg_name.to_string())
		.or_default()
		.push((
			sync_ct,
			Some(format!("version sync to {final_version} (linked versions)")),
			None,
		));
	version_overrides.insert(pkg_name.to_string(), final_version.clone());
}

/// Applies the group `final_version` to every package in the group.
///
/// - Packages **without** a changeset that are below `final_version` get a
///   synthetic sync changelog entry and a version override.
/// - Packages **with** a changeset whose natural bump differs from `final_version`
///   get a version override only (their own changeset entry documents the change).
fn apply_group_final_version(
	group: &[String],
	final_version: &Version,
	aggregated: &mut BTreeMap<String, ChangeType>,
	changes_per_package: &mut BTreeMap<String, PackageChanges>,
	version_overrides: &mut BTreeMap<String, Version>,
	projects: &[Project],
) {
	for pkg_name in group {
		let Some(project) = projects.iter().find(|p| p.name() == pkg_name) else {
			continue;
		};
		if let Some(&ct) = aggregated.get(pkg_name) {
			if bump_version(project.version(), ct) != *final_version {
				version_overrides.insert(pkg_name.clone(), final_version.clone());
			}
		} else if project.version() < final_version {
			promote_package_to_final(
				pkg_name,
				final_version,
				aggregated,
				changes_per_package,
				version_overrides,
				projects,
			);
		}
	}
}

/// Runs the linked-version reconciliation step.
///
/// For each linked group, computes the target version (max current version bumped
/// by the highest change type from any group changeset) and promotes all packages
/// to that target. Returns a map of `package_name → override_version` for packages
/// that need a non-default version.
fn reconcile_linked_versions(
	aggregated: &mut BTreeMap<String, ChangeType>,
	changes_per_package: &mut BTreeMap<String, PackageChanges>,
	linked_groups: &[Vec<String>],
	projects: &[Project],
) -> BTreeMap<String, Version> {
	let mut version_overrides: BTreeMap<String, Version> = BTreeMap::new();
	for group in linked_groups {
		let Some(final_version) = compute_group_final_version(group, aggregated, projects) else {
			continue;
		};
		apply_group_final_version(
			group,
			&final_version,
			aggregated,
			changes_per_package,
			&mut version_overrides,
			projects,
		);
	}
	version_overrides
}

/// After dependency propagation, syncs linked groups so that any member raised by
/// propagation pulls the rest of the group with it.
///
/// Unlike `reconcile_linked_versions`, this pass does not re-derive a bump level
/// from `aggregated` data (which would misread the synthetic sync entries the first
/// pass inserted). Instead it promotes all group members to the max *effective new
/// version* already determined for any member.
fn sync_linked_groups_after_propagation(
	aggregated: &mut BTreeMap<String, ChangeType>,
	changes_per_package: &mut BTreeMap<String, PackageChanges>,
	version_overrides: &mut BTreeMap<String, Version>,
	linked_groups: &[Vec<String>],
	projects: &[Project],
) {
	for group in linked_groups {
		let mut max_effective: Option<Version> = None;
		for pkg_name in group {
			let Some(project) = projects.iter().find(|p| p.name() == pkg_name) else {
				continue;
			};
			let effective = if let Some(v) = version_overrides.get(pkg_name) {
				v.clone()
			} else if let Some(&ct) = aggregated.get(pkg_name) {
				bump_version(project.version(), ct)
			} else {
				project.version().clone()
			};
			max_effective = Some(match max_effective {
				Some(m) => m.max(effective),
				None => effective,
			});
		}
		let Some(target) = max_effective else {
			continue;
		};
		apply_group_final_version(
			group,
			&target,
			aggregated,
			changes_per_package,
			version_overrides,
			projects,
		);
	}
}

/// Bumps package versions, writes changelogs, and collects all modified file paths.
///
/// Runs version bumping, changelog generation, dependency propagation, lock
/// file updates, and changeset consumption. Returns the list of release infos
/// and the deduplicated list of all paths written.
#[allow(clippy::too_many_arguments)]
fn prepare_release_files(
	adapters: &[Arc<dyn PackageManagerAdapter>],
	projects: &[crate::package_manager::Project],
	changesets: &[(PathBuf, Changeset)],
	plan: VersionPlan,
	dry_run: bool,
) -> anyhow::Result<(Vec<ReleaseInfo>, Vec<PathBuf>)> {
	let (release_infos, mut files) = bump_versions_and_generate_changelogs(
		&plan.aggregated,
		&plan.changes_per_package,
		projects,
		&plan.version_overrides,
		&plan.dep_entries,
		dry_run,
	)?;
	files.extend(propagate_dependency_updates(
		projects,
		&release_infos,
		dry_run,
	)?);
	files.extend(update_lock_files(adapters)?);
	let released: BTreeSet<String> = plan.aggregated.keys().cloned().collect();
	files.extend(consume_changesets(changesets, &released, dry_run)?);
	files.extend(plan.propagation_changeset_paths);
	files.sort();
	files.dedup();
	Ok((release_infos, files))
}

/// Bumps versions and generates changelog entries for all affected packages.
///
/// Returns a tuple of `(release_infos, modified_files)` where `release_infos` describes
/// each package prepared for release and `modified_files` is the list of paths modified.
///
/// When `version_overrides` is non-empty, packages in that map use the override
/// version instead of the standard semver bump.
fn bump_versions_and_generate_changelogs(
	aggregated: &BTreeMap<String, ChangeType>,
	changes_per_package: &BTreeMap<String, PackageChanges>,
	projects: &[crate::package_manager::Project],
	version_overrides: &BTreeMap<String, Version>,
	dep_entries: &BTreeMap<String, Vec<String>>,
	dry_run: bool,
) -> anyhow::Result<(Vec<ReleaseInfo>, Vec<PathBuf>)> {
	let mut release_infos: Vec<ReleaseInfo> = Vec::new();
	let mut modified_files: Vec<PathBuf> = Vec::new();
	for (pkg_name, change_type) in aggregated {
		let project = projects
			.iter()
			.find(|p| p.name() == pkg_name)
			.with_context(|| {
				format!("Package '{pkg_name}' from changeset not found in projects")
			})?;
		let current_version = project.version();
		let new_version = version_overrides
			.get(pkg_name)
			.cloned()
			.unwrap_or_else(|| bump_version(current_version, *change_type));
		modified_files.push(project.manifest_path());
		modified_files.push(project.path().join("CHANGELOG.md"));
		let changes = changes_per_package
			.get(pkg_name)
			.cloned()
			.unwrap_or_default();
		let pkg_dep_entries = dep_entries.get(pkg_name).cloned().unwrap_or_default();
		let changelog = Changelog::new(
			new_version.clone(),
			today_iso_date(),
			changes,
			project.path().clone(),
		)
		.with_dependency_entries(pkg_dep_entries);
		let changelog_entry = changelog.format_sections();
		project.write_version(&new_version, dry_run)?;
		changelog.update(dry_run)?;
		info!("{pkg_name}: {current_version} -> {new_version} ({change_type})");
		release_infos.push(ReleaseInfo {
			package_name: pkg_name.clone(),
			new_version,
			changelog_entry,
		});
	}
	Ok((release_infos, modified_files))
}

/// Updates intra-workspace dependency references for all projects.
///
/// For each project that depends on a bumped package, calls `update_dependency_version`
/// and returns the list of modified manifest paths.
fn propagate_dependency_updates(
	projects: &[crate::package_manager::Project],
	release_infos: &[ReleaseInfo],
	dry_run: bool,
) -> anyhow::Result<Vec<PathBuf>> {
	let bumped_versions: BTreeMap<String, semver::Version> = release_infos
		.iter()
		.map(|info| (info.package_name.clone(), info.new_version.clone()))
		.collect();
	let update_verb = if dry_run { "would update" } else { "update" };
	let mut additional_files: Vec<PathBuf> = Vec::new();
	for project in projects {
		for dep_name in project.dependency_names() {
			let Some(new_version) = bumped_versions.get(dep_name.as_str()) else {
				continue;
			};
			let paths = project.update_dependency_version(dep_name, new_version, dry_run)?;
			if !paths.is_empty() {
				info!(
					"  {}: {update_verb} dependency {} to {}",
					project.name(),
					dep_name,
					new_version
				);
				additional_files.extend(paths);
			}
		}
	}
	Ok(additional_files)
}

/// Runs `update_lock_file` on all adapters and collects the resulting paths.
fn update_lock_files(adapters: &[Arc<dyn PackageManagerAdapter>]) -> anyhow::Result<Vec<PathBuf>> {
	let mut files: Vec<PathBuf> = Vec::new();
	for adapter in adapters {
		if let Some(path) = adapter.update_lock_file()? {
			files.push(path);
		}
	}
	Ok(files)
}

/// Consumes or dry-runs the given changeset files for the released packages.
///
/// Returns the list of changeset paths that would be (or were) modified or deleted.
fn consume_changesets(
	changesets: &[(PathBuf, Changeset)],
	released: &BTreeSet<String>,
	dry_run: bool,
) -> anyhow::Result<Vec<PathBuf>> {
	let mut additional_files: Vec<PathBuf> = Vec::new();
	for (path, cs) in changesets {
		let released_pkgs: Vec<&String> = cs
			.packages
			.keys()
			.filter(|name| released.contains(*name))
			.collect();
		if !released_pkgs.is_empty() {
			additional_files.push(path.clone());
		}
		if dry_run {
			if !released_pkgs.is_empty() {
				let pkg_list = released_pkgs
					.iter()
					.map(|s| s.as_str())
					.collect::<Vec<_>>()
					.join(", ");
				info!("Would consume changeset {}: {pkg_list}", path.display());
			}
		} else {
			cs.consume(path, released)?;
		}
	}
	Ok(additional_files)
}

/// Creates or updates the GitHub pull request for the release branch.
///
/// No-ops in dry-run mode or when no GitHub client is available. The dry-run
/// short-circuit is intentional per ADR-017: the PR upsert is the side-effecting
/// operation being guarded, so the check lives here rather than at the call site.
fn upsert_release_pull_request(
	git: &git::GitWorkdir,
	config: &Config,
	env: &crate::Env,
	release_infos: &[ReleaseInfo],
	branch: &str,
	original_branch: Option<&str>,
	dry_run: bool,
) -> anyhow::Result<()> {
	if dry_run {
		info!("Would attempt to create or update a PR in GitHub.");
		return Ok(());
	}
	let Some(client) = env.github_client() else {
		return Ok(());
	};
	let base = original_branch.unwrap_or_else(|| {
		log::warn!("HEAD is detached; using \"main\" as the PR base branch.");
		"main"
	});
	let gh_repo = GitHubRepo::resolve(&config.github, git)
		.context("Could not resolve GitHub repository for PR creation")?;
	let title = config.github.pull_request_title();
	let pr_body = build_pr_body(release_infos, base);
	upsert_pull_request(client, &gh_repo, title, &pr_body, branch, base)
		.context("Failed to create or update pull request")?;
	Ok(())
}

/// Stages, commits, and pushes release changes according to the configured git strategy.
#[allow(clippy::too_many_arguments)]
fn finalize_git_lifecycle(
	git: &git::GitWorkdir,
	config: &Config,
	env: &crate::Env,
	release_infos: &[ReleaseInfo],
	modified_files: &[PathBuf],
	original_branch: Option<&str>,
	release_branch: Option<&str>,
	git_enabled: bool,
	strategy: Strategy,
	dry_run: bool,
) -> anyhow::Result<()> {
	if !git_enabled {
		return Ok(());
	}
	stage_and_commit(git, &config.git.extra_files, release_infos, modified_files)?;
	match strategy {
		Strategy::Push => {
			git.push()?;
		}
		Strategy::Branch => {
			if let Some(branch) = release_branch {
				info!("Pushing branch '{branch}' to origin");
				git.force_push_branch(branch).with_context(|| {
					format!(
						"Failed to push release branch '{branch}'. \
						 You are still on the release branch; run \
						 `git checkout <your-branch>` to return."
					)
				})?;
				let pr_result = if config.github.enabled {
					upsert_release_pull_request(
						git,
						config,
						env,
						release_infos,
						branch,
						original_branch,
						dry_run,
					)
				} else {
					Ok(())
				};
				if let Some(orig) = original_branch
					&& let Err(checkout_err) = git.checkout(orig)
				{
					log::error!(
						"Failed to check out original branch after release: {checkout_err:#}"
					);
				}
				pr_result?;
			}
		}
	}
	Ok(())
}

/// Resolves and applies linked-version constraints to the aggregated changeset data.
///
/// Validates scoped-prepare rules, then runs reconciliation and returns the
/// resulting version overrides.
fn resolve_linked_groups(
	config: &Config,
	args: &PrepareArgs,
	projects: &[Project],
) -> anyhow::Result<Vec<Vec<String>>> {
	let project_names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let linked_groups = config.linked_versions.resolve_groups(&project_names)?;
	validate_scoped_prepare_linked_groups(
		&args.packages,
		&linked_groups,
		config.linked_versions.is_global(),
	)?;
	Ok(linked_groups)
}

/// Result of computing the version plan for a prepare run.
struct VersionPlan {
	aggregated: BTreeMap<String, ChangeType>,
	changes_per_package: BTreeMap<String, PackageChanges>,
	version_overrides: BTreeMap<String, Version>,
	dep_entries: BTreeMap<String, Vec<String>>,
	propagation_changeset_paths: Vec<PathBuf>,
}

/// Aggregates changesets, applies linked versions, and runs dependency propagation.
///
/// Returns the full version plan for the prepare run.
#[allow(clippy::too_many_arguments)]
fn compute_version_plan(
	git: &git::GitWorkdir,
	changesets: &[(PathBuf, Changeset)],
	args: &PrepareArgs,
	config: &Config,
	projects: &[Project],
	git_enabled: bool,
	dry_run: bool,
) -> anyhow::Result<VersionPlan> {
	let commit_refs = resolve_commit_references(changesets, git, git_enabled);
	let (mut aggregated, mut changes_per_package) =
		aggregate_changesets(changesets, &args.packages, projects, &commit_refs)?;
	let linked_groups = resolve_linked_groups(config, args, projects)?;
	// First pass: sync linked groups from explicit changesets.
	let mut version_overrides = reconcile_linked_versions(
		&mut aggregated,
		&mut changes_per_package,
		&linked_groups,
		projects,
	);
	let (dep_entries, propagation_changeset_paths) = apply_dependency_propagation(
		projects,
		&mut aggregated,
		&version_overrides,
		&args.packages,
		config.prepare.dependency_bump,
		git,
		dry_run,
	)?;
	// Second pass: propagated bumps may have raised a linked member's version, so
	// re-sync to bring the rest of each group up to the new target.
	sync_linked_groups_after_propagation(
		&mut aggregated,
		&mut changes_per_package,
		&mut version_overrides,
		&linked_groups,
		projects,
	);
	Ok(VersionPlan {
		aggregated,
		changes_per_package,
		version_overrides,
		dep_entries,
		propagation_changeset_paths,
	})
}

/// Resolves git-enabled flag, strategy, and emits a warning for incompatible flags.
fn setup_git_context(config: &Config, args: &PrepareArgs) -> (bool, Strategy) {
	let git_enabled = config.git.enabled() && !args.no_git;
	let strategy = config.git.strategy();
	if args.branch.is_some() && strategy == Strategy::Push {
		log::warn!("--branch has no effect with the push strategy; ignoring");
	}
	(git_enabled, strategy)
}

/// Runs the `prepare` subcommand.
pub(crate) fn cmd_prepare(
	git: &git::GitWorkdir,
	args: &PrepareArgs,
	dry_run: bool,
	config: Config,
) -> anyhow::Result<ExitCode> {
	let env = config.env().context("env not set")?;
	let adapters = config.create_adapters()?;
	let projects = config.load_projects_for_adapters(&adapters)?;
	let changesets = Changeset::read_all(git)?;

	if changesets.is_empty() {
		info!("No pending changesets found. Nothing to prepare.");
		return Ok(ExitCode::SUCCESS);
	}

	let (git_enabled, strategy) = setup_git_context(&config, args);
	let plan = compute_version_plan(
		git,
		&changesets,
		args,
		&config,
		&projects,
		git_enabled,
		dry_run,
	)?;
	let (original_branch, release_branch) =
		preflight_checks(git, &config, env, args, git_enabled, strategy, dry_run)?;
	let (release_infos, modified_files) =
		prepare_release_files(&adapters, &projects, &changesets, plan, dry_run)?;

	finalize_git_lifecycle(
		git,
		&config,
		env,
		&release_infos,
		&modified_files,
		original_branch.as_deref(),
		release_branch.as_deref(),
		git_enabled,
		strategy,
		dry_run,
	)?;

	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::model::config;

	use super::*;

	fn make_runner() -> Arc<dyn CommandRunner> {
		Arc::new(RecordingCommandRunner::new(0))
	}

	// ── format_commit_message ─────────────────────────────────────────────────

	#[test]
	fn format_commit_message_single_package() {
		let infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		assert_eq!(
			format_commit_message(&infos),
			"chore(release): my-pkg@1.0.0"
		);
	}

	#[test]
	fn format_commit_message_multiple_packages() {
		let infos = vec![
			ReleaseInfo {
				package_name: "pkg-a".to_string(),
				new_version: "1.0.0".parse().unwrap(),
				changelog_entry: String::new(),
			},
			ReleaseInfo {
				package_name: "pkg-b".to_string(),
				new_version: "2.1.0".parse().unwrap(),
				changelog_entry: String::new(),
			},
		];
		assert_eq!(
			format_commit_message(&infos),
			"chore(release): pkg-a@1.0.0, pkg-b@2.1.0"
		);
	}

	#[test]
	fn format_commit_message_empty() {
		assert_eq!(format_commit_message(&[]), "chore(release): ");
	}

	// ── stage_and_commit ──────────────────────────────────────────────────────

	#[test]
	fn stage_and_commit_empty_releases_is_noop() {
		let dir = tempfile::tempdir().unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = stage_and_commit(&git, &[], &[], &[]);
		assert!(result.is_ok());
	}

	#[test]
	fn stage_and_commit_dry_run_suppresses_git_commands() {
		let dir = tempfile::tempdir().unwrap();
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let inner = Arc::new(RecordingCommandRunner::new(0));
		let dry_run_runner =
			crate::command::DryRunCommandRunner::new(Arc::clone(&inner) as Arc<dyn CommandRunner>);
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::new(dry_run_runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = stage_and_commit(&git, &[], &release_infos, &[]);
		assert!(result.is_ok());
		// DryRunCommandRunner suppresses run_mut calls — the inner recorder receives nothing.
		assert!(inner.invocations().is_empty());
	}

	#[test]
	fn extra_files_outside_repo_is_rejected() {
		// Create an outer temp dir with a repo subdir and a secret file alongside it.
		let outer = tempfile::tempdir().unwrap();
		let repo_dir = outer.path().join("repo");
		std::fs::create_dir(&repo_dir).unwrap();
		let secret = outer.path().join("secret.txt");
		std::fs::write(&secret, "secret").unwrap();
		// "../secret.txt" from repo_dir resolves to outer/secret.txt (outside the repo).
		let extra_files = vec!["../secret.txt".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(&repo_dir).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = stage_and_commit(&git, &extra_files, &release_infos, &[]);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[cfg(unix)]
	#[test]
	fn extra_files_symlink_outside_repo_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		// Create a symlink inside the tempdir pointing to /tmp (outside the repo).
		let symlink_path = dir.path().join("escape");
		std::os::unix::fs::symlink("/tmp", &symlink_path).unwrap();
		let extra_files = vec!["escape".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = stage_and_commit(&git, &extra_files, &release_infos, &[]);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[test]
	fn extra_files_nonexistent_is_skipped() {
		let dir = tempfile::tempdir().unwrap();
		let extra_files = vec!["does-not-exist.txt".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		// Should succeed (non-existent file is a warning, not an error).
		let result = stage_and_commit(&git, &extra_files, &release_infos, &[]);
		assert!(result.is_ok());
	}

	#[test]
	fn extra_files_absolute_path_is_rejected() {
		let dir = tempfile::tempdir().unwrap();
		let extra_files = vec!["/etc/passwd".to_string()];
		let release_infos = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.0.0".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = stage_and_commit(&git, &extra_files, &release_infos, &[]);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("resolves outside the repository root")
		);
	}

	#[test]
	fn bump_version_major() {
		let v = "1.2.3".parse().unwrap();
		assert_eq!(bump_version(&v, ChangeType::Major).to_string(), "2.0.0");
	}

	#[test]
	fn bump_version_minor() {
		let v = "1.2.3".parse().unwrap();
		assert_eq!(bump_version(&v, ChangeType::Minor).to_string(), "1.3.0");
	}

	#[test]
	fn bump_version_patch() {
		let v = "1.2.3".parse().unwrap();
		assert_eq!(bump_version(&v, ChangeType::Patch).to_string(), "1.2.4");
	}

	#[test]
	fn bump_version_clears_prerelease() {
		let v = "1.0.0-alpha.1".parse().unwrap();
		assert_eq!(bump_version(&v, ChangeType::Patch).to_string(), "1.0.1");
	}

	#[test]
	fn bump_version_major_resets_minor_and_patch() {
		let v = "1.5.9".parse().unwrap();
		let bumped = bump_version(&v, ChangeType::Major);
		assert_eq!(bumped.to_string(), "2.0.0");
	}

	#[test]
	fn bump_version_minor_resets_patch() {
		let v = "1.5.9".parse().unwrap();
		let bumped = bump_version(&v, ChangeType::Minor);
		assert_eq!(bumped.to_string(), "1.6.0");
	}

	#[test]
	fn compute_release_branch_uses_flag_over_all() {
		assert_eq!(
			compute_release_branch(Some("my-branch"), "release/", Some("main")),
			"my-branch"
		);
	}

	#[test]
	fn compute_release_branch_uses_config_prefix() {
		assert_eq!(
			compute_release_branch(None, "release/", Some("main")),
			"release/main"
		);
	}

	#[test]
	fn compute_release_branch_uses_default_prefix() {
		assert_eq!(
			compute_release_branch(None, "cursus-release/", Some("main")),
			"cursus-release/main"
		);
	}

	#[test]
	fn compute_release_branch_detached_fallback() {
		assert_eq!(
			compute_release_branch(None, "cursus-release/", None),
			"cursus-release/detached"
		);
	}

	#[test]
	fn check_dirty_tree_succeeds_when_clean() {
		let dir = tempfile::tempdir().unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0)); // empty stdout → clean
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = check_dirty_tree(&git);
		assert!(result.is_ok());
	}

	#[test]
	fn check_dirty_tree_fails_when_dirty() {
		let dir = tempfile::tempdir().unwrap();
		let runner =
			Arc::new(RecordingCommandRunner::new(0).with_stdout(b" M src/main.rs\n".to_vec()));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = check_dirty_tree(&git);
		assert!(result.is_err());
		assert!(
			result.unwrap_err().to_string().contains("dirty"),
			"Expected 'dirty' in error message"
		);
	}

	#[test]
	fn cmd_prepare_no_changesets_succeeds() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::model::config::CargoConfig::enabled());
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();

		let args = PrepareArgs::default();
		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, false, config).unwrap();
		assert_eq!(result, ExitCode::SUCCESS);
	}

	#[test]
	fn cmd_prepare_unknown_package_in_changeset_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::model::config::CargoConfig::enabled());
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"real-project\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();
		// Changeset references a package that doesn't exist
		let cursus_dir = dir.path().join(".cursus");
		std::fs::write(
			cursus_dir.join("test.md"),
			"+++\nnonexistent-package = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let args = PrepareArgs::default();
		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, false, config);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("not found in projects")
		);
	}

	/// Sets up a temporary Cargo workspace with `pkg-a` (0.1.0) and `pkg-b` (0.2.0).
	fn setup_two_package_workspace() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::model::config::CargoConfig::enabled());
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[workspace]\nmembers = [\"pkg-a\", \"pkg-b\"]\n",
		)
		.unwrap();
		std::fs::create_dir_all(dir.path().join("pkg-a/src")).unwrap();
		std::fs::write(
			dir.path().join("pkg-a/Cargo.toml"),
			"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
		)
		.unwrap();
		std::fs::write(dir.path().join("pkg-a/src/lib.rs"), "").unwrap();
		std::fs::create_dir_all(dir.path().join("pkg-b/src")).unwrap();
		std::fs::write(
			dir.path().join("pkg-b/Cargo.toml"),
			"[package]\nname = \"pkg-b\"\nversion = \"0.2.0\"\nedition = \"2024\"\n",
		)
		.unwrap();
		std::fs::write(dir.path().join("pkg-b/src/lib.rs"), "").unwrap();
		dir
	}

	#[test]
	fn cmd_prepare_package_flag_filters_packages() {
		let dir = setup_two_package_workspace();

		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		let changeset_path = cursus_dir.join("test.md");
		std::fs::write(
			&changeset_path,
			"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs {
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, false, config);
		assert!(result.is_ok());

		// Changeset should be rewritten with only pkg-b remaining
		assert!(
			changeset_path.exists(),
			"Changeset should still exist (partially consumed)"
		);
		let content = std::fs::read_to_string(&changeset_path).unwrap();
		assert!(
			content.contains("pkg-b = \"minor\""),
			"pkg-b should remain in changeset, got: {content}"
		);
		assert!(
			!content.contains("pkg-a"),
			"pkg-a should be removed from changeset, got: {content}"
		);
	}

	#[test]
	fn cmd_prepare_package_flag_with_dry_run_leaves_changeset_untouched() {
		let dir = setup_two_package_workspace();

		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		let changeset_path = cursus_dir.join("test.md");
		let original = "+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n";
		std::fs::write(&changeset_path, original).unwrap();

		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs {
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, true, config);
		assert!(result.is_ok());

		// Dry-run must not touch the changeset even when scoped
		let content = std::fs::read_to_string(&changeset_path).unwrap();
		assert_eq!(
			content, original,
			"Changeset should be untouched in dry-run"
		);
	}

	#[test]
	fn cmd_prepare_unknown_package_flag_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::model::config::CargoConfig::enabled());
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"real-project\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();

		let cursus_dir = dir.path().join(".cursus");
		std::fs::write(
			cursus_dir.join("test.md"),
			"+++\nreal-project = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let runner = make_runner();
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs {
			packages: vec!["nonexistent".to_string()],
			no_git: true,
			..PrepareArgs::default()
		};
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);
		let result = cmd_prepare(&git, &args, false, config);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("Unknown package: nonexistent")
		);
	}

	#[test]
	fn build_pr_body_empty_releases() {
		let body = build_pr_body(&[], "main");
		assert!(body.contains("# Releases"));
		assert!(body.contains("`main`"));
		assert!(body.contains("Cursus"));
	}

	#[test]
	fn build_pr_body_formats_single_release() {
		let releases = vec![ReleaseInfo {
			package_name: "my-pkg".to_string(),
			new_version: "1.2.0".parse().unwrap(),
			changelog_entry: "### Features\n\n- Added something\n".to_string(),
		}];
		let body = build_pr_body(&releases, "main");
		assert!(body.contains("## my-pkg@1.2.0"));
		assert!(body.contains("### Features"));
		assert!(body.contains("- Added something"));
		assert!(body.contains("`main`"));
	}

	#[test]
	fn build_pr_body_formats_multiple_releases() {
		let releases = vec![
			ReleaseInfo {
				package_name: "pkg-a".to_string(),
				new_version: "1.0.0".parse().unwrap(),
				changelog_entry: "### Bug Fixes\n\n- Fixed a bug\n".to_string(),
			},
			ReleaseInfo {
				package_name: "pkg-b".to_string(),
				new_version: "2.1.0".parse().unwrap(),
				changelog_entry: String::new(),
			},
		];
		let body = build_pr_body(&releases, "develop");
		assert!(body.contains("## pkg-a@1.0.0"));
		assert!(body.contains("### Bug Fixes"));
		assert!(body.contains("- Fixed a bug"));
		assert!(body.contains("## pkg-b@2.1.0"));
		assert!(body.contains("`develop`"));
		// pkg-a section must appear before pkg-b
		let pos_a = body.find("## pkg-a").unwrap();
		let pos_b = body.find("## pkg-b").unwrap();
		assert!(pos_a < pos_b);
	}

	#[test]
	fn build_pr_body_includes_base_branch_in_intro() {
		let body = build_pr_body(&[], "my-feature-branch");
		assert!(body.contains("`my-feature-branch`"));
	}

	#[test]
	fn build_pr_body_snapshot() {
		let releases = vec![
			ReleaseInfo {
				package_name: "pkg-a".to_string(),
				new_version: "2.0.0".parse().unwrap(),
				changelog_entry: "### Breaking Changes\n\n- Removed old API\n".to_string(),
			},
			ReleaseInfo {
				package_name: "pkg-b".to_string(),
				new_version: "1.3.0".parse().unwrap(),
				changelog_entry:
					"### Features\n\n- Added widget\n\n### Bug Fixes\n\n- Fixed crash\n".to_string(),
			},
			ReleaseInfo {
				package_name: "pkg-c".to_string(),
				new_version: "0.9.1".parse().unwrap(),
				changelog_entry: String::new(),
			},
		];
		insta::assert_snapshot!(build_pr_body(&releases, "main"));
	}

	/// Sets up a temp dir with a Cargo project, branch strategy git config, and GitHub config.
	fn setup_branch_strategy_with_github() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::model::config::CargoConfig::enabled())
				.with_git(
					crate::model::config::GitConfig::enabled_config()
						.with_strategy(crate::model::config::Strategy::Branch),
				)
				.with_github(
					crate::model::config::GitHubConfig::enabled_config()
						.with_owner("acme".to_string())
						.with_repo("app".to_string())
						.with_pull_request_title("My Release PR".to_string()),
				);
		cfg.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"test-pkg\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
		)
		.unwrap();
		std::fs::create_dir_all(dir.path().join("src")).unwrap();
		std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
		let cursus_dir = dir.path().join(".cursus");
		std::fs::write(
			cursus_dir.join("change.md"),
			"+++\ntest-pkg = \"patch\"\n+++\n\nFix\n",
		)
		.unwrap();
		dir
	}

	#[test]
	fn cmd_prepare_branch_strategy_with_github_creates_pr() {
		use crate::github::client::GitHubClient;
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let client = Arc::new(RecordingGitHubClient::new());
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>)
			.with_github_client(Arc::clone(&client) as Arc<dyn GitHubClient>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs::default();

		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&env, dir_abs.clone());
		let result = cmd_prepare(&git, &args, false, config);
		assert!(result.is_ok(), "Expected Ok, got: {result:?}");

		let invocations = client.invocations();
		let pr = invocations
			.iter()
			.find(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. }));
		assert!(pr.is_some(), "Expected PR creation, got: {invocations:?}");
		if let Some(GitHubInvocation::CreatePullRequest { title, gh_repo, .. }) = pr {
			assert_eq!(title, "My Release PR");
			assert_eq!(gh_repo.owner, "acme");
			assert_eq!(gh_repo.repo, "app");
		}
	}

	#[test]
	fn cmd_prepare_branch_strategy_pr_failure_is_fatal() {
		use crate::github::client::GitHubClient;
		use crate::github::client::test_support::RecordingGitHubClient;
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let client = Arc::new(RecordingGitHubClient::new().with_create_pr_failure());
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>)
			.with_github_client(Arc::clone(&client) as Arc<dyn GitHubClient>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs::default();

		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&env, dir_abs.clone());
		let result = cmd_prepare(&git, &args, false, config);
		assert!(
			result.is_err(),
			"PR failure should be fatal, got: {result:?}"
		);
	}

	// ── upsert_pull_request ───────────────────────────────────────────────────

	#[test]
	fn upsert_pull_request_creates_when_no_existing() {
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let client = RecordingGitHubClient::new(); // no existing PR
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"cursus-release/main",
			"main",
		);
		assert!(result.is_ok(), "Expected Ok, got: {result:?}");
		let invocations = client.invocations();
		// Should have called find then create
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::FindOpenPullRequest { .. })),
			"Expected FindOpenPullRequest invocation"
		);
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. })),
			"Expected CreatePullRequest invocation"
		);
	}

	#[test]
	fn upsert_pull_request_updates_when_existing() {
		use crate::github::client::PullRequest;
		use crate::github::client::test_support::{GitHubInvocation, RecordingGitHubClient};
		let existing_pr = PullRequest {
			number: 7,
			html_url: "https://github.com/acme/app/pull/7".to_string(),
		};
		let client = RecordingGitHubClient::new().with_existing_pr(existing_pr);
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"updated body",
			"cursus-release/main",
			"main",
		);
		assert!(result.is_ok(), "Expected Ok, got: {result:?}");
		let invocations = client.invocations();
		assert!(
			invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::FindOpenPullRequest { .. })),
			"Expected FindOpenPullRequest invocation"
		);
		assert!(
			invocations.iter().any(|i| matches!(
				i,
				GitHubInvocation::UpdatePullRequest { pull_number, .. } if *pull_number == 7
			)),
			"Expected UpdatePullRequest invocation for PR #7"
		);
		assert!(
			!invocations
				.iter()
				.any(|i| matches!(i, GitHubInvocation::CreatePullRequest { .. })),
			"Should NOT call CreatePullRequest when existing PR found"
		);
	}

	#[test]
	fn upsert_pull_request_propagates_find_error() {
		use crate::github::client::test_support::RecordingGitHubClient;
		let client = RecordingGitHubClient::new().with_find_pr_failure();
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"release-branch",
			"main",
		);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("simulated find_open_pull_request failure"),
			"Expected find failure error, got: {msg}"
		);
	}

	#[test]
	fn upsert_pull_request_propagates_update_error() {
		use crate::github::client::PullRequest;
		use crate::github::client::test_support::RecordingGitHubClient;
		let existing_pr = PullRequest {
			number: 1,
			html_url: "https://github.com/acme/app/pull/1".to_string(),
		};
		let client = RecordingGitHubClient::new()
			.with_existing_pr(existing_pr)
			.with_update_pr_failure();
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"release-branch",
			"main",
		);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("simulated update_pull_request failure"),
			"Expected update failure error, got: {msg}"
		);
	}

	#[test]
	fn upsert_pull_request_propagates_create_error() {
		use crate::github::client::test_support::RecordingGitHubClient;
		let client = RecordingGitHubClient::new().with_create_pr_failure();
		let gh_repo = crate::github::remote::GitHubRepo::new("acme", "app").unwrap();
		let result = upsert_pull_request(
			&client,
			&gh_repo,
			"Release PR",
			"body",
			"release-branch",
			"main",
		);
		assert!(result.is_err());
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("simulated create_pull_request failure"),
			"Expected create failure error, got: {msg}"
		);
	}

	#[test]
	fn cmd_prepare_no_github_client_errors() {
		let dir = setup_branch_strategy_with_github();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		// No github client — pre-flight check should error
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env).unwrap();
		let args = PrepareArgs::default();

		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(&env, dir_abs.clone());
		let result = cmd_prepare(&git, &args, false, config);
		assert!(result.is_err(), "Expected Err without github client");
		let msg = format!("{:#}", result.unwrap_err());
		assert!(
			msg.contains("no GitHub token"),
			"Expected 'no GitHub token' error, got: {msg}"
		);
	}

	// ── resolve_commit_references ─────────────────────────────────────────────

	#[test]
	fn resolve_commit_references_git_disabled_does_not_call_git() {
		let dir = tempfile::tempdir().unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);

		// Create a fake changeset path
		let changeset_path = dir.path().join(".cursus/test.md");
		let fake_cs = crate::model::changeset::Changeset {
			packages: std::collections::BTreeMap::new(),
			message: None,
		};
		let changesets = vec![(changeset_path.clone(), fake_cs)];
		let result = resolve_commit_references(&changesets, &git, false);
		assert_eq!(result.len(), 1);
		assert_eq!(result[&changeset_path], None);
		assert!(
			runner.invocations().is_empty(),
			"No git calls when disabled"
		);
	}

	#[test]
	fn resolve_commit_references_git_enabled_no_commit_returns_none() {
		// When git log returns empty output, reference is None (no error).
		let dir = tempfile::tempdir().unwrap();
		let runner = Arc::new(RecordingCommandRunner::new(0)); // empty stdout
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);

		let changeset_path = dir.path().join("test.md");
		let fake_cs = crate::model::changeset::Changeset {
			packages: std::collections::BTreeMap::new(),
			message: None,
		};
		let changesets = vec![(changeset_path.clone(), fake_cs)];
		let result = resolve_commit_references(&changesets, &git, true);
		assert_eq!(result[&changeset_path], None);
	}

	#[test]
	fn resolve_commit_references_git_failure_is_nonfatal() {
		// A git failure should produce None, not propagate an error.
		let dir = tempfile::tempdir().unwrap();
		let runner =
			Arc::new(RecordingCommandRunner::new(1).with_stderr(b"fatal: not a git repo".to_vec()));
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let git = git::GitWorkdir::new(
			&crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>),
			dir_abs.clone(),
		);

		let changeset_path = dir.path().join("test.md");
		let fake_cs = crate::model::changeset::Changeset {
			packages: std::collections::BTreeMap::new(),
			message: None,
		};
		let changesets = vec![(changeset_path.clone(), fake_cs)];
		// Should not panic or return an error — just None
		let result = resolve_commit_references(&changesets, &git, true);
		assert_eq!(result[&changeset_path], None);
	}

	// ── aggregate_changesets with commit_refs ─────────────────────────────────

	#[test]
	fn aggregate_changesets_with_empty_refs_produces_none_references() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let cfg =
			crate::model::config::Config::new(&crate::path::AbsolutePath::new(dir.path()).unwrap())
				.with_cargo(crate::model::config::CargoConfig::enabled());
		cfg.save().unwrap();

		let path = PathBuf::from("test.md");
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
		let env = crate::Env::new(Arc::clone(&runner) as Arc<dyn CommandRunner>);
		let config =
			crate::model::config::load(&crate::path::AbsolutePath::new(dir.path()).unwrap(), &env)
				.unwrap();
		let adapters = config.create_adapters().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"my-pkg\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();
		let projects = config.load_projects_for_adapters(&adapters).unwrap();

		let (_, changes_per_package) =
			aggregate_changesets(&changesets, &[], &projects, &commit_refs).unwrap();
		let changes = changes_per_package.get("my-pkg").unwrap();
		assert_eq!(changes.len(), 1);
		let (_, _, commit_ref) = &changes[0];
		assert_eq!(*commit_ref, None, "Expected None commit reference");
	}

	// ── infer_change_type ─────────────────────────────────────────────────────

	#[test]
	fn infer_change_type_major_when_major_differs() {
		let old: semver::Version = "1.2.3".parse().unwrap();
		let new: semver::Version = "2.0.0".parse().unwrap();
		assert_eq!(
			infer_change_type(&old, &new),
			crate::model::changeset::ChangeType::Major
		);
	}

	#[test]
	fn infer_change_type_minor_when_only_minor_differs() {
		let old: semver::Version = "1.2.3".parse().unwrap();
		let new: semver::Version = "1.3.0".parse().unwrap();
		assert_eq!(
			infer_change_type(&old, &new),
			crate::model::changeset::ChangeType::Minor
		);
	}

	#[test]
	fn infer_change_type_patch_when_only_patch_differs() {
		let old: semver::Version = "1.2.3".parse().unwrap();
		let new: semver::Version = "1.2.4".parse().unwrap();
		assert_eq!(
			infer_change_type(&old, &new),
			crate::model::changeset::ChangeType::Patch
		);
	}

	#[test]
	fn infer_change_type_patch_when_equal() {
		let v: semver::Version = "1.2.3".parse().unwrap();
		assert_eq!(
			infer_change_type(&v, &v),
			crate::model::changeset::ChangeType::Patch
		);
	}

	// ── validate_scoped_prepare_linked_groups ─────────────────────────────────

	#[test]
	fn validate_empty_filter_always_passes() {
		let groups = vec![vec!["pkg-a".to_string(), "pkg-b".to_string()]];
		assert!(validate_scoped_prepare_linked_groups(&[], &groups, false).is_ok());
		assert!(validate_scoped_prepare_linked_groups(&[], &groups, true).is_ok());
	}

	#[test]
	fn validate_global_with_filter_errors() {
		let result = validate_scoped_prepare_linked_groups(&["pkg-a".to_string()], &[], true);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("global linked-versions")
		);
	}

	#[test]
	fn validate_partial_overlap_errors() {
		let groups = vec![vec!["pkg-a".to_string(), "pkg-b".to_string()]];
		let filter = vec!["pkg-a".to_string()];
		let result = validate_scoped_prepare_linked_groups(&filter, &groups, false);
		assert!(result.is_err());
		let msg = result.unwrap_err().to_string();
		assert!(msg.contains("partially overlaps"));
		assert!(msg.contains("pkg-b"));
	}

	#[test]
	fn validate_full_group_in_scope_passes() {
		let groups = vec![vec!["pkg-a".to_string(), "pkg-b".to_string()]];
		let filter = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		assert!(validate_scoped_prepare_linked_groups(&filter, &groups, false).is_ok());
	}

	#[test]
	fn validate_group_entirely_out_of_scope_passes() {
		let groups = vec![vec!["pkg-a".to_string(), "pkg-b".to_string()]];
		let filter = vec!["standalone".to_string()];
		assert!(validate_scoped_prepare_linked_groups(&filter, &groups, false).is_ok());
	}

	// ── compute_group_final_version ───────────────────────────────────────────

	fn v(s: &str) -> semver::Version {
		s.parse().unwrap()
	}

	fn make_project(name: &str, version: &str) -> crate::package_manager::Project {
		crate::package_manager::Project::new_test_with_version(name, v(version))
	}

	#[test]
	fn compute_group_final_version_no_changeset_returns_max_current() {
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let aggregated = BTreeMap::new(); // no changesets
		let projects = vec![
			make_project("pkg-a", "1.5.0"),
			make_project("pkg-b", "1.2.0"),
		];
		let result = compute_group_final_version(&group, &aggregated, &projects);
		// No changeset → final version = max current = 1.5.0
		assert_eq!(result, Some(v("1.5.0")));
	}

	#[test]
	fn compute_group_final_version_applies_highest_change_type_to_max_current() {
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated = BTreeMap::new();
		aggregated.insert("pkg-b".to_string(), ChangeType::Minor); // only B has changeset
		let projects = vec![
			make_project("pkg-a", "2.3.4"), // higher current, no changeset
			make_project("pkg-b", "1.2.3"), // lower current, has changeset
		];
		let result = compute_group_final_version(&group, &aggregated, &projects);
		// max_current = 2.3.4; highest_ct = Minor → bump(2.3.4, Minor) = 2.4.0
		assert_eq!(result, Some(v("2.4.0")));
	}

	#[test]
	fn compute_group_final_version_major_wins_over_minor() {
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Major);
		aggregated.insert("pkg-b".to_string(), ChangeType::Minor);
		let projects = vec![
			make_project("pkg-a", "1.0.0"),
			make_project("pkg-b", "1.0.0"),
		];
		let result = compute_group_final_version(&group, &aggregated, &projects);
		// highest_ct = Major → bump(1.0.0, Major) = 2.0.0
		assert_eq!(result, Some(v("2.0.0")));
	}

	#[test]
	fn compute_group_final_version_returns_none_for_empty_group_in_projects() {
		let group = vec!["nonexistent".to_string()];
		let aggregated = BTreeMap::new();
		let projects: Vec<crate::package_manager::Project> = vec![];
		let result = compute_group_final_version(&group, &aggregated, &projects);
		// No project found → max_current is None → returns None
		assert_eq!(result, None);
	}

	// ── reconcile_linked_versions ─────────────────────────────────────────────

	#[test]
	fn reconcile_promotes_no_changeset_package_to_final() {
		// A@2.3.4 (no cs) + B@1.2.3 (patch) → both should end at 2.3.5
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
		aggregated.insert("pkg-b".to_string(), ChangeType::Patch);
		let mut changes_per_package: BTreeMap<String, PackageChanges> = BTreeMap::new();
		changes_per_package
			.entry("pkg-b".to_string())
			.or_default()
			.push((ChangeType::Patch, Some("a fix".to_string()), None));
		let projects = vec![
			make_project("pkg-a", "2.3.4"),
			make_project("pkg-b", "1.2.3"),
		];
		let overrides = reconcile_linked_versions(
			&mut aggregated,
			&mut changes_per_package,
			&[group],
			&projects,
		);
		// pkg-a has no changeset, is below 2.3.5 → gets a sync override
		assert_eq!(overrides.get("pkg-a"), Some(&v("2.3.5")));
		// pkg-b has a changeset; natural bump(1.2.3, Patch)=1.2.4 ≠ 2.3.5 → override
		assert_eq!(overrides.get("pkg-b"), Some(&v("2.3.5")));
		// pkg-a should now have a sync changelog entry
		let a_changes = changes_per_package.get("pkg-a").unwrap();
		assert!(
			a_changes
				.iter()
				.any(|(_, msg, _)| msg.as_deref().is_some_and(|m| m.contains("version sync")))
		);
	}

	#[test]
	fn reconcile_no_override_when_natural_bump_matches_final() {
		// A@1.0.0 (patch) + B@1.0.0 (no cs): final = 1.0.1; A natural bump = 1.0.1 → no override for A
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Patch);
		let mut changes_per_package: BTreeMap<String, PackageChanges> = BTreeMap::new();
		changes_per_package
			.entry("pkg-a".to_string())
			.or_default()
			.push((ChangeType::Patch, Some("a fix".to_string()), None));
		let projects = vec![
			make_project("pkg-a", "1.0.0"),
			make_project("pkg-b", "1.0.0"),
		];
		let overrides = reconcile_linked_versions(
			&mut aggregated,
			&mut changes_per_package,
			&[group],
			&projects,
		);
		// A natural bump = 1.0.1 == final → no override for A
		assert!(
			!overrides.contains_key("pkg-a"),
			"pkg-a should not be overridden"
		);
		// B has no changeset, current 1.0.0 < 1.0.1 → promoted
		assert_eq!(overrides.get("pkg-b"), Some(&v("1.0.1")));
	}

	#[test]
	fn reconcile_skips_packages_already_at_final_version() {
		// Both packages at the same version, only A has a changeset
		let group = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Patch);
		let mut changes_per_package: BTreeMap<String, PackageChanges> = BTreeMap::new();
		changes_per_package
			.entry("pkg-a".to_string())
			.or_default()
			.push((ChangeType::Patch, Some("a fix".to_string()), None));
		let projects = vec![
			make_project("pkg-a", "1.0.1"),
			make_project("pkg-b", "1.0.1"),
		];
		// final = bump(1.0.1, Patch) = 1.0.2; B is at 1.0.1 < 1.0.2 → promoted
		let overrides = reconcile_linked_versions(
			&mut aggregated,
			&mut changes_per_package,
			&[group],
			&projects,
		);
		assert_eq!(overrides.get("pkg-b"), Some(&v("1.0.2")));
	}

	// ── DependencyBump::to_change_type ───────────────────────────────────────

	#[test]
	fn propagation_change_type_patch_mode_always_returns_patch() {
		for upstream in [ChangeType::Patch, ChangeType::Minor, ChangeType::Major] {
			assert_eq!(
				DependencyBump::Patch.to_change_type(upstream),
				ChangeType::Patch,
			);
		}
	}

	#[test]
	fn propagation_change_type_minor_mode_always_returns_minor() {
		for upstream in [ChangeType::Patch, ChangeType::Minor, ChangeType::Major] {
			assert_eq!(
				DependencyBump::Minor.to_change_type(upstream),
				ChangeType::Minor,
			);
		}
	}

	#[test]
	fn propagation_change_type_major_mode_always_returns_major() {
		for upstream in [ChangeType::Patch, ChangeType::Minor, ChangeType::Major] {
			assert_eq!(
				DependencyBump::Major.to_change_type(upstream),
				ChangeType::Major,
			);
		}
	}

	#[test]
	fn propagation_change_type_match_mode_mirrors_upstream() {
		assert_eq!(
			DependencyBump::Match.to_change_type(ChangeType::Patch),
			ChangeType::Patch,
		);
		assert_eq!(
			DependencyBump::Match.to_change_type(ChangeType::Minor),
			ChangeType::Minor,
		);
		assert_eq!(
			DependencyBump::Match.to_change_type(ChangeType::Major),
			ChangeType::Major,
		);
	}

	#[test]
	fn propagation_change_type_auto_mode_maps_minor_and_patch_to_patch() {
		assert_eq!(
			DependencyBump::Auto.to_change_type(ChangeType::Patch),
			ChangeType::Patch,
		);
		assert_eq!(
			DependencyBump::Auto.to_change_type(ChangeType::Minor),
			ChangeType::Patch,
		);
	}

	#[test]
	fn propagation_change_type_auto_mode_maps_major_to_major() {
		assert_eq!(
			DependencyBump::Auto.to_change_type(ChangeType::Major),
			ChangeType::Major,
		);
	}

	// ── build_reverse_dep_graph ───────────────────────────────────────────────

	fn make_project_with_deps(
		name: &str,
		version: &str,
		deps: Vec<&str>,
	) -> crate::package_manager::Project {
		crate::package_manager::Project::new_test_with_deps(name, version, deps)
	}

	#[test]
	fn build_reverse_dep_graph_empty_projects_returns_empty() {
		let graph = build_reverse_dep_graph(&[]);
		assert!(graph.is_empty());
	}

	#[test]
	fn build_reverse_dep_graph_no_deps_returns_empty() {
		let projects = vec![
			make_project("pkg-a", "1.0.0"),
			make_project("pkg-b", "1.0.0"),
		];
		let graph = build_reverse_dep_graph(&projects);
		assert!(graph.is_empty());
	}

	#[test]
	fn build_reverse_dep_graph_filters_external_deps() {
		// pkg-a depends on serde (external) and pkg-b (intra-workspace)
		let projects = vec![
			make_project_with_deps("pkg-a", "1.0.0", vec!["serde", "pkg-b"]),
			make_project("pkg-b", "1.0.0"),
		];
		let graph = build_reverse_dep_graph(&projects);
		// Only pkg-b should appear (serde is external)
		assert_eq!(graph.len(), 1);
		assert_eq!(graph["pkg-b"], vec!["pkg-a"]);
	}

	#[test]
	fn build_reverse_dep_graph_multiple_dependents_on_same_package() {
		let projects = vec![
			make_project_with_deps("pkg-a", "1.0.0", vec!["pkg-c"]),
			make_project_with_deps("pkg-b", "1.0.0", vec!["pkg-c"]),
			make_project("pkg-c", "1.0.0"),
		];
		let graph = build_reverse_dep_graph(&projects);
		let mut dependents = graph["pkg-c"].clone();
		dependents.sort();
		assert_eq!(dependents, vec!["pkg-a", "pkg-b"]);
	}

	// ── mark_propagation_bumps ────────────────────────────────────────────────

	#[test]
	fn mark_propagation_bumps_empty_aggregated_returns_empty() {
		let aggregated = BTreeMap::new();
		let version_overrides = BTreeMap::new();
		let reverse_deps = BTreeMap::new();
		let result = mark_propagation_bumps(
			&aggregated,
			&version_overrides,
			&reverse_deps,
			DependencyBump::Auto,
		);
		assert!(result.is_empty());
	}

	#[test]
	fn mark_propagation_bumps_skips_linked_packages() {
		let mut aggregated = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Major);
		let mut version_overrides = BTreeMap::new();
		version_overrides.insert("pkg-b".to_string(), "2.0.0".parse().unwrap());
		let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
		reverse_deps.insert("pkg-a".to_string(), vec!["pkg-b".to_string()]);
		let result = mark_propagation_bumps(
			&aggregated,
			&version_overrides,
			&reverse_deps,
			DependencyBump::Auto,
		);
		// pkg-b is linked (in version_overrides) so should not be propagated to
		assert!(!result.contains_key("pkg-b"));
	}

	#[test]
	fn mark_propagation_bumps_only_upgrades_not_downgrades() {
		let mut aggregated = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Patch);
		// pkg-b already has a Major changeset
		aggregated.insert("pkg-b".to_string(), ChangeType::Major);
		let version_overrides = BTreeMap::new();
		let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
		reverse_deps.insert("pkg-a".to_string(), vec!["pkg-b".to_string()]);
		let result = mark_propagation_bumps(
			&aggregated,
			&version_overrides,
			&reverse_deps,
			DependencyBump::Auto, // Patch upstream → Patch propagation
		);
		// pkg-b already has Major, propagation would be Patch → should not appear
		assert!(!result.contains_key("pkg-b"));
	}

	#[test]
	fn mark_propagation_bumps_terminates_with_circular_deps() {
		// A depends on B, B depends on A — cycle.
		// Note: Cargo rejects circular dependencies at the workspace level, so this
		// scenario is more relevant to npm workspaces. This unit test verifies that
		// the BFS algorithm terminates regardless, via idempotent marking.
		let mut aggregated = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Minor);
		let version_overrides = BTreeMap::new();
		let mut reverse_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
		reverse_deps.insert("pkg-a".to_string(), vec!["pkg-b".to_string()]);
		reverse_deps.insert("pkg-b".to_string(), vec!["pkg-a".to_string()]);
		// Should terminate (not loop forever) and produce a result
		let result = mark_propagation_bumps(
			&aggregated,
			&version_overrides,
			&reverse_deps,
			DependencyBump::Auto,
		);
		assert!(result.contains_key("pkg-b"));
	}

	// ── effective_new_version ─────────────────────────────────────────────────

	#[test]
	fn effective_new_version_returns_none_for_unknown_package() {
		let projects = vec![make_project("pkg-a", "1.0.0")];
		let aggregated = BTreeMap::new();
		let version_overrides = BTreeMap::new();
		let propagation_map = BTreeMap::new();
		let result = effective_new_version(
			"unknown",
			&projects,
			&aggregated,
			&version_overrides,
			&propagation_map,
		);
		assert!(result.is_none());
	}

	#[test]
	fn effective_new_version_prefers_version_override() {
		let projects = vec![make_project("pkg-a", "1.0.0")];
		let mut aggregated = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Major);
		let mut version_overrides = BTreeMap::new();
		version_overrides.insert("pkg-a".to_string(), "9.9.9".parse().unwrap());
		let propagation_map = BTreeMap::new();
		let result = effective_new_version(
			"pkg-a",
			&projects,
			&aggregated,
			&version_overrides,
			&propagation_map,
		);
		assert_eq!(result, Some("9.9.9".parse().unwrap()));
	}

	#[test]
	fn effective_new_version_uses_aggregated_changeset() {
		let projects = vec![make_project("pkg-a", "1.2.0")];
		let mut aggregated = BTreeMap::new();
		aggregated.insert("pkg-a".to_string(), ChangeType::Minor);
		let version_overrides = BTreeMap::new();
		let propagation_map = BTreeMap::new();
		let result = effective_new_version(
			"pkg-a",
			&projects,
			&aggregated,
			&version_overrides,
			&propagation_map,
		);
		assert_eq!(result, Some("1.3.0".parse().unwrap()));
	}

	#[test]
	fn effective_new_version_falls_back_to_propagation_map() {
		let projects = vec![make_project("pkg-a", "1.0.0")];
		let aggregated = BTreeMap::new();
		let version_overrides = BTreeMap::new();
		let mut propagation_map = BTreeMap::new();
		propagation_map.insert("pkg-a".to_string(), (ChangeType::Patch, vec![]));
		let result = effective_new_version(
			"pkg-a",
			&projects,
			&aggregated,
			&version_overrides,
			&propagation_map,
		);
		assert_eq!(result, Some("1.0.1".parse().unwrap()));
	}
}
