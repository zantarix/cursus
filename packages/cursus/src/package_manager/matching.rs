//! Project file attribution — matching changed file paths to workspace projects.

use std::collections::HashSet;

use crate::package_manager::Project;
use crate::path::AbsolutePath;

/// For each project, returns whether any of `changed_files` lies inside it.
///
/// `changed_files` are paths relative to `git_root` (slash-separated, as
/// emitted by `git diff --name-only`). Matching is path-component aware —
/// `foo` does not match `foobar` — and uses longest-prefix attribution:
/// when nested projects exist, only the deepest project(s) containing the
/// file are reported. When several projects share the same deepest path
/// (e.g. Cargo and npm both at the repo root), all of them are marked.
///
/// Projects whose path lies outside `git_root` are always `false`; git
/// cannot track files outside the repository.
///
/// Returns a `Vec<bool>` parallel to `projects`.
pub fn match_files_to_projects(
	projects: &[Project],
	git_root: &AbsolutePath,
	changed_files: &HashSet<String>,
) -> Vec<bool> {
	// Pre-compute relative path strings for each project.
	// `None` means the project path is outside the git root; such projects are left as false.
	let rel_paths: Vec<Option<String>> = projects
		.iter()
		.map(|p| {
			p.path()
				.strip_prefix(git_root.as_path())
				.ok()
				.map(|r| r.to_string_lossy().into_owned())
		})
		.collect();

	let mut matched = vec![false; projects.len()];

	// For each changed file, find all projects that match with the longest prefix.
	// When multiple projects share the same path (e.g. Cargo and npm both at the repo root),
	// all of them are marked changed — not just the last one in the list.
	for file in changed_files {
		let candidates: Vec<(usize, usize)> = rel_paths
			.iter()
			.enumerate()
			.filter_map(|(i, rel_opt)| {
				let rel = rel_opt.as_deref()?;
				if rel.is_empty() {
					// Root project: matches any file, but with the lowest priority (0).
					Some((i, 0usize))
				} else if file.starts_with(rel)
					&& (file.len() == rel.len() || file.as_bytes().get(rel.len()) == Some(&b'/'))
				{
					Some((i, rel.len()))
				} else {
					None
				}
			})
			.collect();

		if let Some(&(_, best_len)) = candidates.iter().max_by_key(|(_, len)| *len) {
			candidates
				.iter()
				.filter(|(_, len)| *len == best_len)
				.for_each(|(i, _)| matched[*i] = true);
		}
	}

	matched
}

/// Like [`match_files_to_projects`], but uses `attribution_scope` for longest-prefix
/// matching and returns a `Vec<bool>` parallel to `projects`.
///
/// Use this when `projects` is a filtered subset of all workspace projects (e.g.
/// after applying `[global].ignore`). Files attributed to a project that is present
/// in `attribution_scope` but absent from `projects` — such as an ignored
/// sub-project — are **not** propagated to that project's parent.
///
/// When `attribution_scope` equals `projects`, this produces the same result as
/// calling [`match_files_to_projects`] directly.
pub fn match_files_to_projects_in_scope(
	projects: &[Project],
	attribution_scope: &[Project],
	git_root: &AbsolutePath,
	changed_files: &HashSet<String>,
) -> Vec<bool> {
	let scope_matched = match_files_to_projects(attribution_scope, git_root, changed_files);
	let matched_paths: HashSet<&AbsolutePath> = attribution_scope
		.iter()
		.zip(scope_matched.iter())
		.filter_map(|(p, &m)| m.then_some(p.path()))
		.collect();
	projects
		.iter()
		.map(|p| matched_paths.contains(p.path()))
		.collect()
}
