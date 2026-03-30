use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use log::info;

use crate::model::changelog::Changelog;
use crate::model::changeset::{ChangeType, Changeset};
use crate::package_manager::{PackageManagerAdapter, Project};
use crate::utils::today_iso_date;

use super::version::bump_version;
use super::{PackageChanges, PrepareOutput, ReleaseInfo, VersionPlan};

/// Bumps package versions, writes changelogs, and collects all modified file paths.
///
/// Runs version bumping, changelog generation, dependency propagation, lock
/// file updates, and changeset consumption. Returns a [`PrepareOutput`] containing
/// release infos and the deduplicated list of all paths written.
pub(super) async fn prepare_release_files(
	adapters: &[Arc<dyn PackageManagerAdapter>],
	projects: &[Project],
	changesets: &[(crate::path::AbsolutePath, Changeset)],
	plan: VersionPlan,
	dry_run: bool,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<PrepareOutput> {
	let (release_infos, mut files) = bump_versions_and_generate_changelogs(
		&plan.aggregated,
		&plan.changes_per_package,
		projects,
		&plan.version_overrides,
		&plan.dep_entries,
		dry_run,
		fs,
	)
	.await?;
	files.extend(propagate_dependency_updates(projects, &release_infos, dry_run).await?);
	files.extend(update_lock_files(adapters).await?);
	let released: BTreeSet<String> = plan.aggregated.keys().cloned().collect();
	files.extend(consume_changesets(changesets, &released, dry_run, fs).await?);
	files.extend(plan.propagation_changeset_paths);
	files.sort();
	files.dedup();
	Ok(PrepareOutput {
		release_infos,
		modified_files: files,
	})
}

/// Bumps versions and generates changelog entries for all affected packages.
///
/// Returns a tuple of `(release_infos, modified_files)` where `release_infos` describes
/// each package prepared for release and `modified_files` is the list of paths modified.
///
/// When `version_overrides` is non-empty, packages in that map use the override
/// version instead of the standard semver bump.
pub(super) async fn bump_versions_and_generate_changelogs(
	aggregated: &BTreeMap<String, ChangeType>,
	changes_per_package: &BTreeMap<String, PackageChanges>,
	projects: &[Project],
	version_overrides: &BTreeMap<String, semver::Version>,
	dep_entries: &BTreeMap<String, Vec<String>>,
	dry_run: bool,
	fs: &dyn crate::filesystem::Filesystem,
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
		modified_files.push(project.manifest_path().await);
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
		project.write_version(&new_version, dry_run).await?;
		changelog.update(dry_run, fs).await?;
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
pub(super) async fn propagate_dependency_updates(
	projects: &[Project],
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
			let paths = project
				.update_dependency_version(dep_name, new_version, dry_run)
				.await?;
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
pub(super) async fn update_lock_files(
	adapters: &[Arc<dyn PackageManagerAdapter>],
) -> anyhow::Result<Vec<PathBuf>> {
	let mut files: Vec<PathBuf> = Vec::new();
	for adapter in adapters {
		if let Some(path) = adapter.update_lock_file().await? {
			files.push(path);
		}
	}
	Ok(files)
}

/// Consumes or dry-runs the given changeset files for the released packages.
///
/// Returns the list of changeset paths that would be (or were) modified or deleted.
pub(super) async fn consume_changesets(
	changesets: &[(crate::path::AbsolutePath, Changeset)],
	released: &BTreeSet<String>,
	dry_run: bool,
	fs: &dyn crate::filesystem::Filesystem,
) -> anyhow::Result<Vec<PathBuf>> {
	let mut additional_files: Vec<PathBuf> = Vec::new();
	for (path, cs) in changesets {
		let released_pkgs: Vec<&String> = cs
			.packages
			.keys()
			.filter(|name| released.contains(*name))
			.collect();
		if !released_pkgs.is_empty() {
			additional_files.push(path.clone().into_path_buf());
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
			cs.consume(path, released, fs).await?;
		}
	}
	Ok(additional_files)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::filesystem::LocalFilesystem;
	use crate::model::config;

	fn make_runner() -> Arc<dyn CommandRunner> {
		Arc::new(RecordingCommandRunner::new(0))
	}

	/// Loads all Cargo projects from a temporary workspace directory.
	async fn load_projects_for_dir(
		dir: &tempfile::TempDir,
	) -> Vec<crate::package_manager::Project> {
		let runner = make_runner();
		let path = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(runner, path)),
		);
		config::load(env.fs(), env.git().path())
			.await
			.unwrap()
			.unwrap()
			.load_projects(&env)
			.await
			.unwrap()
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

	/// Sets up a temporary Cargo workspace with `pkg-a` (0.1.0) and `pkg-b` (0.2.0).
	async fn setup_two_package_workspace() -> tempfile::TempDir {
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

	/// Sets up a temporary Cargo workspace where `pkg-b` (1.0.0) has an
	/// intra-workspace dependency on `pkg-a` (1.0.0).
	async fn setup_workspace_with_dependency() -> tempfile::TempDir {
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
			"[workspace]\nmembers = [\"pkg-a\", \"pkg-b\"]\n",
		)
		.unwrap();
		std::fs::create_dir_all(dir.path().join("pkg-a/src")).unwrap();
		std::fs::write(
			dir.path().join("pkg-a/Cargo.toml"),
			"[package]\nname = \"pkg-a\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
		)
		.unwrap();
		std::fs::write(dir.path().join("pkg-a/src/lib.rs"), "").unwrap();
		std::fs::create_dir_all(dir.path().join("pkg-b/src")).unwrap();
		std::fs::write(
			dir.path().join("pkg-b/Cargo.toml"),
			"[package]\nname = \"pkg-b\"\nversion = \"1.0.0\"\nedition = \"2024\"\n\n[dependencies]\npkg-a = { path = \"../pkg-a\", version = \"1.0.0\" }\n",
		)
		.unwrap();
		std::fs::write(dir.path().join("pkg-b/src/lib.rs"), "").unwrap();
		dir
	}

	#[tokio::test]
	async fn cmd_prepare_package_flag_filters_packages() {
		let dir = setup_two_package_workspace().await;

		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		let changeset_path = cursus_dir.join("test.md");
		std::fs::write(
			&changeset_path,
			"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let runner = make_runner();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				dir_abs.clone(),
			)),
		);
		let config = config::load(env.fs(), env.git().path())
			.await
			.unwrap()
			.unwrap();
		let args = super::super::PrepareArgs {
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..super::super::PrepareArgs::default()
		};
		let result = super::super::cmd_prepare(&args, false, &env, config).await;
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

	#[tokio::test]
	async fn propagate_dependency_updates_returns_modified_manifest_paths() {
		// Guards `additional_files.extend(paths)` mutation: verifies that when a dependency
		// version is updated, the modified manifest path is included in the returned vec.
		let dir = setup_workspace_with_dependency().await;
		let projects = load_projects_for_dir(&dir).await;
		let release_infos = vec![super::ReleaseInfo {
			package_name: "pkg-a".to_string(),
			new_version: "1.0.1".parse().unwrap(),
			changelog_entry: String::new(),
		}];
		let modified_paths = super::propagate_dependency_updates(&projects, &release_infos, false)
			.await
			.unwrap();
		assert!(
			modified_paths
				.iter()
				.any(|p| p.to_string_lossy().contains("pkg-b")),
			"Expected pkg-b manifest in modified paths, got: {modified_paths:?}"
		);
	}

	#[tokio::test]
	async fn cmd_prepare_package_flag_with_dry_run_leaves_changeset_untouched() {
		let dir = setup_two_package_workspace().await;

		let cursus_dir = dir.path().join(".cursus");
		std::fs::create_dir_all(&cursus_dir).unwrap();
		let changeset_path = cursus_dir.join("test.md");
		let original = "+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n";
		std::fs::write(&changeset_path, original).unwrap();

		let runner = make_runner();
		let dir_abs = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let env = crate::Env::new(
			Arc::clone(&runner) as Arc<dyn CommandRunner>,
			Arc::new(LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(
				Arc::clone(&runner) as Arc<dyn CommandRunner>,
				dir_abs.clone(),
			)),
		);
		let config = config::load(env.fs(), env.git().path())
			.await
			.unwrap()
			.unwrap();
		let args = super::super::PrepareArgs {
			packages: vec!["pkg-a".to_string()],
			no_git: true,
			..super::super::PrepareArgs::default()
		};
		let result = super::super::cmd_prepare(&args, true, &env, config).await;
		assert!(result.is_ok());

		// Dry-run must not touch the changeset even when scoped
		let content = std::fs::read_to_string(&changeset_path).unwrap();
		assert_eq!(
			content, original,
			"Changeset should be untouched in dry-run"
		);
	}
}
