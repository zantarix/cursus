//! Package manager adapters for project enumeration and management.
//!
//! This module provides a trait-based abstraction over different package managers,
//! allowing Cursus to work with various ecosystems (npm, Cargo, etc.) through
//! a unified interface.

mod cargo;
pub mod matching;
mod npm;

pub use cargo::CargoAdapter;
pub use npm::NpmAdapter;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use semver::Version;

use crate::filesystem::Filesystem;
use crate::model::config::Config;
use crate::path::AbsolutePath;

/// Raw project data returned by package manager adapters.
///
/// This intermediate type contains only the project metadata without a reference
/// to the adapter. Use [`enumerate_projects`] to get full [`Project`] instances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
	/// The name of the project (e.g., package name).
	pub name: String,
	/// The absolute path to the project root.
	pub path: AbsolutePath,
	/// The current version of the project.
	pub version: Version,
	/// Whether the project is publishable (not marked as private).
	pub publishable: bool,
	/// Names of intra-workspace dependencies.
	pub dependency_names: Vec<String>,
	/// Whether `publishConfig.provenance` is `true` in the manifest.
	///
	/// Only populated by the npm adapter; `None` for other adapters.
	pub publishconfig_provenance: Option<bool>,
	/// Whether this project inherits its version from the workspace root
	/// (e.g. `version.workspace = true` in Cargo).
	///
	/// When `true`, [`PackageManagerAdapter::write_version`] updates the workspace
	/// root rather than this project's manifest. All projects sharing a workspace
	/// version must be in the same linked-versions group.
	pub workspace_version: bool,
}

#[cfg(test)]
impl ProjectInfo {
	/// Creates a minimal `ProjectInfo` for use in unit tests.
	///
	/// Fills in: version `0.0.0-development`, `publishable = true`, empty `dependency_names`.
	pub fn for_test(name: &str, path: AbsolutePath) -> Self {
		use semver::{BuildMetadata, Prerelease};
		Self {
			name: name.to_string(),
			path,
			version: Version {
				major: 0,
				minor: 0,
				patch: 0,
				pre: Prerelease::new("development").unwrap(),
				build: BuildMetadata::EMPTY,
			},
			publishable: true,
			dependency_names: Vec::new(),
			publishconfig_provenance: None,
			workspace_version: false,
		}
	}
}

/// Outcome of a package publish operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
	/// The package was successfully published.
	Published,
	/// The package version already exists in the registry (not an error).
	AlreadyPublished,
}

/// Represents a project discovered by a package manager adapter.
///
/// Each project maintains a reference to the package manager that discovered it,
/// allowing further interaction through methods implemented on this type.
pub struct Project {
	/// The project metadata.
	info: ProjectInfo,
	/// Reference to the package manager that discovered this project.
	adapter: Arc<dyn PackageManagerAdapter>,
}

impl std::fmt::Debug for Project {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Project")
			.field("name", &self.info.name)
			.field("path", &self.info.path)
			.finish_non_exhaustive()
	}
}

impl Clone for Project {
	fn clone(&self) -> Self {
		Self {
			info: self.info.clone(),
			adapter: Arc::clone(&self.adapter),
		}
	}
}

impl PartialEq for Project {
	fn eq(&self, other: &Self) -> bool {
		self.info == other.info
	}
}

impl Eq for Project {}

impl Project {
	/// Returns the name of the project (e.g., package name).
	pub fn name(&self) -> &str {
		&self.info.name
	}

	/// Returns the absolute path to the project root.
	pub fn path(&self) -> &AbsolutePath {
		&self.info.path
	}

	/// Returns the current version of this project.
	///
	/// The version is cached from when the project was enumerated.
	pub fn version(&self) -> &Version {
		&self.info.version
	}

	/// Writes a new version to this project's manifest file.
	///
	/// Returns the list of file paths that were (or would be) modified by the write.
	/// Callers should extend their `modified_files` list with the returned paths so
	/// those files are staged for git.
	///
	/// When `dry_run` is `true`, no files are written to disk, but the returned
	/// list still describes which paths would be modified.
	/// Delegates to the underlying package manager adapter.
	pub async fn write_version(
		&self,
		version: &Version,
		dry_run: bool,
	) -> anyhow::Result<Vec<PathBuf>> {
		self.adapter
			.write_version(&self.info, version, dry_run)
			.await
	}

	/// Publishes this project to its package registry.
	///
	/// Delegates to the underlying package manager adapter.
	pub async fn publish(&self) -> anyhow::Result<PublishOutcome> {
		self.adapter.publish(&self.info).await
	}

	/// Returns the name of the registry this project would be published to.
	///
	/// Delegates to the underlying package manager adapter.
	pub async fn registry_name(&self) -> &str {
		self.adapter.registry_name().await
	}

	/// Returns whether this project is publishable (not marked as private).
	///
	/// The publishable status is cached from when the project was enumerated.
	pub fn is_publishable(&self) -> bool {
		self.info.publishable
	}

	/// Returns `true` if `config` allows this project to be released.
	///
	/// A project is releasable when its manifest is publishable, or its name
	/// is listed in `[git].publish_private_packages` (tag-only release path).
	/// Does **not** check whether release artefacts have been prepared —
	/// compose with [`Project::is_prepared_for_release`] for that.
	pub fn is_releasable_under(&self, config: &Config) -> bool {
		self.is_publishable()
			|| config
				.git
				.publish_private_packages()
				.iter()
				.any(|p| p == self.name())
	}

	/// Returns `true` if a `CHANGELOG.md` exists at this project's root,
	/// meaning `cursus prepare` has already been run for this project.
	///
	/// Compose with [`Project::is_releasable_under`] to determine whether
	/// the project will be acted on during `cursus publish`.
	pub async fn is_prepared_for_release(&self, fs: &dyn Filesystem) -> anyhow::Result<bool> {
		fs.exists(&self.path().child("CHANGELOG.md")).await
	}

	/// Returns the names of intra-workspace dependencies for this project.
	///
	/// The dependency names are cached from when the project was enumerated.
	pub fn dependency_names(&self) -> &[String] {
		&self.info.dependency_names
	}

	/// Returns `true` if this project inherits its version from the workspace root.
	pub fn workspace_version(&self) -> bool {
		self.info.workspace_version
	}

	/// Updates a dependency version in this project's manifest file.
	///
	/// When `dry_run` is `true`, detects which files would change but does not write them.
	/// Delegates to the underlying package manager adapter.
	pub async fn update_dependency_version(
		&self,
		dependency_name: &str,
		new_version: &Version,
		dry_run: bool,
	) -> anyhow::Result<Vec<PathBuf>> {
		self.adapter
			.update_dependency_version(&self.info, dependency_name, new_version, dry_run)
			.await
	}

	/// Returns the absolute path to this project's manifest file.
	///
	/// Combines the project's absolute path with the adapter-specific manifest
	/// filename (e.g., `Cargo.toml` or `package.json`).
	pub async fn manifest_path(&self) -> std::path::PathBuf {
		self.info.path.join(self.adapter.manifest_filename().await)
	}

	/// Creates a minimal `Project` with a dummy adapter for use in unit tests.
	#[cfg(test)]
	pub fn new_test(name: &str, path: &str) -> Self {
		use crate::command::test_support::RecordingCommandRunner;
		Self::new_test_with_runner(name, path, Arc::new(RecordingCommandRunner::new(0)))
	}

	/// Sets the `workspace_version` flag on this test project.
	#[cfg(test)]
	pub fn with_workspace_version(mut self, workspace_version: bool) -> Self {
		self.info.workspace_version = workspace_version;
		self
	}

	/// Creates a `Project` with specific dependency names for use in unit tests.
	#[cfg(test)]
	pub fn new_test_with_deps(name: &str, version: &str, deps: Vec<&str>) -> Self {
		let mut p = Self::new_test_with_version(name, version.parse().unwrap());
		p.info.dependency_names = deps.into_iter().map(str::to_string).collect();
		p
	}

	/// Creates a minimal `Project` with a specific version for use in unit tests.
	#[cfg(test)]
	pub fn new_test_with_version(name: &str, version: semver::Version) -> Self {
		use crate::command::test_support::RecordingCommandRunner;
		use crate::model::config::NpmConfig;
		let runner =
			Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn crate::command::CommandRunner>;
		let env = crate::Env::new(
			Arc::clone(&runner),
			Arc::new(crate::filesystem::LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(
				runner,
				crate::path::AbsolutePath::new("/tmp").unwrap(),
			)),
		);
		let path = format!("/nonexistent/packages/{name}");
		Self {
			info: ProjectInfo {
				name: name.to_string(),
				path: AbsolutePath::new(&path).unwrap(),
				version,
				publishable: true,
				dependency_names: Vec::new(),
				publishconfig_provenance: None,
				workspace_version: false,
			},
			adapter: Arc::new(NpmAdapter::new(
				NpmConfig::default(),
				AbsolutePath::new("/nonexistent").unwrap(),
				env,
			)),
		}
	}

	/// Creates a `Project` with a custom recording runner for use in unit tests.
	#[cfg(test)]
	pub fn new_test_with_runner(
		name: &str,
		path: &str,
		runner: Arc<crate::command::test_support::RecordingCommandRunner>,
	) -> Self {
		use crate::command::CommandRunner;
		use crate::model::config::NpmConfig;
		let runner = runner as Arc<dyn CommandRunner>;
		let env = crate::Env::new(
			Arc::clone(&runner),
			Arc::new(crate::filesystem::LocalFilesystem),
			Arc::new(crate::git::GitWorkdir::new(
				runner,
				crate::path::AbsolutePath::new("/tmp").unwrap(),
			)),
		);
		Self {
			info: ProjectInfo::for_test(name, AbsolutePath::new(path).unwrap()),
			adapter: Arc::new(NpmAdapter::new(
				NpmConfig::default(),
				AbsolutePath::new("/nonexistent").unwrap(),
				env,
			)),
		}
	}

	/// Creates a non-publishable `Project` for use in unit tests.
	#[cfg(test)]
	pub fn new_test_not_publishable(name: &str, path: &str) -> Self {
		let mut p = Self::new_test(name, path);
		p.info.publishable = false;
		p
	}
}

/// Trait for package manager adapters.
///
/// Implementations of this trait provide package-manager-specific functionality
/// for discovering and managing projects within a repository.
#[async_trait]
pub trait PackageManagerAdapter: Send + Sync + std::fmt::Debug {
	/// Enumerates all projects managed by this package manager.
	///
	/// For single-package repositories, this returns a single project.
	/// For monorepos, this returns all workspace packages.
	///
	/// The returned `ProjectInfo` instances include:
	/// - `version`: The current version from the manifest
	/// - `publishable`: Whether the project is publishable (not private)
	/// - `dependency_names`: Names of intra-workspace dependencies
	///
	/// # Errors
	///
	/// Returns an error if project enumeration fails (e.g., invalid manifest files).
	async fn enumerate_projects(&self) -> anyhow::Result<Vec<ProjectInfo>>;

	/// Writes a new version to a project's manifest file, preserving formatting.
	///
	/// Returns the list of file paths that were (or would be) modified. For most
	/// adapters this is just the project's own manifest (e.g. `package.json` or
	/// `Cargo.toml`), but for Cargo projects that inherit `version.workspace = true`
	/// it is the workspace-root `Cargo.toml` instead. Callers must extend their
	/// `modified_files` list with the returned paths so those files are staged for
	/// git — this mirrors the contract of [`PackageManagerAdapter::update_dependency_version`].
	///
	/// When `dry_run` is `true`, the method validates but does not write to disk.
	/// Still returns the paths of files that would be modified.
	///
	/// # Arguments
	///
	/// * `project` - The project to update.
	/// * `version` - The new version to write.
	/// * `dry_run` - If `true`, skip the filesystem write.
	///
	/// # Errors
	///
	/// Returns an error if the manifest file cannot be read or (when `dry_run` is false) written.
	async fn write_version(
		&self,
		project: &ProjectInfo,
		version: &Version,
		dry_run: bool,
	) -> anyhow::Result<Vec<PathBuf>>;

	/// Updates the lock file after version changes.
	///
	/// Resolves the lock file path unconditionally, then runs the update command
	/// via `run_mut` (which is a no-op when [`DryRunCommandRunner`] is active).
	/// This means the path is always returned so that callers can track it for
	/// git staging — even in dry-run mode, the path is known without running any command.
	///
	/// This is a workspace-level operation and should be called once per adapter
	/// after all version writes are complete.
	///
	/// Returns `Some(path)` with the lock file path that was (or would be) updated,
	/// or `None` if no lock file exists or the location cannot be determined (e.g.
	/// when a custom `lock_command` is configured).
	///
	/// # Errors
	///
	/// Returns an error if the lock file update command fails.
	async fn update_lock_file(&self) -> anyhow::Result<Option<PathBuf>>;

	/// Publishes a project to its package registry.
	///
	/// If the package version already exists in the registry, this should return
	/// `Ok(PublishOutcome::AlreadyPublished)` rather than an error.
	///
	/// # Arguments
	///
	/// * `project` - The project to publish.
	///
	/// # Errors
	///
	/// Returns an error if the publish operation fails for reasons other than
	/// the package already existing.
	///
	/// # Note on dry-run
	///
	/// This method uses `run_mut` internally, so `DryRunCommandRunner` suppresses the
	/// subprocess. However, the synthetic success output causes `publish()` to return
	/// `PublishOutcome::Published`. Callers that need dry-run semantics **must** gate
	/// this call themselves — see `publish_projects()` in `src/cli/publish.rs`.
	async fn publish(&self, project: &ProjectInfo) -> anyhow::Result<PublishOutcome>;

	/// Returns the name of the registry this adapter publishes to.
	///
	/// Used for display purposes in CLI output (e.g., "crates.io", "npm").
	async fn registry_name(&self) -> &str;

	/// Returns the filename of the package manifest (e.g., `"Cargo.toml"` or `"package.json"`).
	async fn manifest_filename(&self) -> &str;

	/// Updates a dependency version in a project's manifest file.
	///
	/// `dependency_name` is the name of the dependency to update.
	/// `new_version` is the version to write.
	///
	/// When `dry_run` is `true`, detects which files would change but does not write them.
	/// Still returns the paths of files that would be modified so callers can track them
	/// for git staging purposes.
	///
	/// Returns a list of modified file paths (may include the project's own
	/// manifest and/or a workspace-level manifest).
	///
	/// # Errors
	///
	/// Returns an error if a manifest file cannot be read or (when `dry_run` is false) written.
	async fn update_dependency_version(
		&self,
		project: &ProjectInfo,
		dependency_name: &str,
		new_version: &Version,
		dry_run: bool,
	) -> anyhow::Result<Vec<PathBuf>>;
}

/// Extracts the non-numeric prefix from a semver range string.
///
/// For example, `"^1.0.0"` returns `"^"`, `"~1.0.0"` returns `"~"`, and
/// `"1.0.0"` returns `""`. Used to preserve operator prefixes when rewriting
/// version strings in manifest files.
pub(crate) fn semver_range_prefix(version_range: &str) -> &str {
	let digit_pos = version_range
		.char_indices()
		.find(|(_, c)| c.is_ascii_digit())
		.map_or(version_range.len(), |(i, _)| i);
	&version_range[..digit_pos]
}

mod dependency_graph;
pub use dependency_graph::DependencyGraph;

/// Validates that every name in `package_names` matches a known project.
///
/// # Errors
///
/// Returns an error if any name in `package_names` does not match a known project.
pub fn validate_package_names(
	projects: &[Project],
	package_names: &[String],
) -> anyhow::Result<()> {
	for name in package_names {
		if !projects.iter().any(|p| p.name() == name) {
			anyhow::bail!("Unknown package: {name}");
		}
	}
	Ok(())
}

/// Filters a project list by package names, validating that all names exist.
///
/// If `package_names` is empty, returns all projects unchanged.
/// Otherwise, returns only projects whose names match the given list.
///
/// # Errors
///
/// Returns an error if any name in `package_names` does not match a known project.
pub fn filter_projects_by_name(
	projects: &[Project],
	package_names: &[String],
) -> anyhow::Result<Vec<Project>> {
	if package_names.is_empty() {
		return Ok(projects.to_vec());
	}

	package_names
		.iter()
		.map(|name| {
			projects
				.iter()
				.find(|p| p.name() == name)
				.cloned()
				.ok_or_else(|| anyhow::anyhow!("Unknown package: {name}"))
		})
		.collect()
}

/// Enumerates projects from multiple adapters and returns a flattened list.
///
/// This is the primary way to get [`Project`] instances. The returned projects
/// maintain a reference to the adapter that discovered them for further interaction.
///
/// # Arguments
///
/// * `adapters` - The package manager adapters to use.
///
/// # Errors
///
/// Returns an error if any adapter fails to enumerate its projects.
pub async fn enumerate_projects(
	adapters: impl IntoIterator<Item = Arc<dyn PackageManagerAdapter>>,
) -> anyhow::Result<Vec<Project>> {
	let mut all_projects = Vec::new();
	for adapter in adapters {
		let infos = adapter.enumerate_projects().await?;
		for info in infos {
			all_projects.push(Project {
				info,
				adapter: Arc::clone(&adapter),
			});
		}
	}
	Ok(all_projects)
}

/// Builds a dependency graph for the given projects.
///
/// This function uses cached dependency names from each project to construct
/// the dependency graph. The adjacency list maps each project to its dependencies.
///
/// # Arguments
///
/// * `projects` - All projects in the workspace to analyze.
///
/// # Errors
///
/// Returns an error if dependency analysis fails (e.g., manifest files cannot be read).
pub fn build_dependency_graph(projects: &[Project]) -> anyhow::Result<DependencyGraph> {
	let project_names: std::collections::HashSet<_> = projects.iter().map(|p| p.name()).collect();

	let mut adjacency = std::collections::HashMap::new();
	for project in projects {
		let mut dependencies = Vec::new();
		for dep_name in project.dependency_names() {
			// Only include intra-workspace dependencies
			if project_names.contains(dep_name.as_str()) {
				dependencies.push(dep_name.clone());
			}
		}
		adjacency.insert(project.name().to_string(), dependencies);
	}

	Ok(DependencyGraph::from_adjacency(adjacency))
}

#[cfg(test)]
mod tests {
	use std::path::Path;
	use std::sync::Arc;

	use crate::command::CommandRunner;
	use crate::command::test_support::RecordingCommandRunner;
	use crate::model::config::NpmConfig;

	use super::*;

	#[test]
	fn project_equality() {
		let p1 = Project::new_test("test", "/nonexistent/packages/test");
		let p2 = Project::new_test("test", "/nonexistent/packages/test");
		let p3 = Project::new_test("other", "/nonexistent/packages/other");

		assert_eq!(p1, p2);
		assert_ne!(p1, p3);
	}

	#[test]
	fn project_debug() {
		let project = Project::new_test("my-package", "/nonexistent/packages/my-package");
		let debug = format!("{:?}", project);
		assert!(debug.contains("my-package"));
	}

	#[test]
	fn project_clone() {
		let project = Project::new_test("test", "/nonexistent/src");
		let cloned = project.clone();
		assert_eq!(project, cloned);
	}

	#[test]
	fn project_getters() {
		let project = Project::new_test("my-pkg", "/nonexistent/packages/my-pkg");
		assert_eq!(project.name(), "my-pkg");
		assert_eq!(
			project.path().as_path(),
			Path::new("/nonexistent/packages/my-pkg")
		);
	}

	#[tokio::test]
	async fn project_registry_name_delegates_to_adapter() {
		// new_test uses NpmAdapter, which returns "npm"
		let project = Project::new_test("my-app", "/nonexistent");
		assert_eq!(project.registry_name().await, "npm");
	}

	#[tokio::test]
	async fn enumerate_projects_attaches_adapter() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("package.json"),
			r#"{"name": "test", "version": "0.1.0"}"#,
		)
		.unwrap();

		let adapter: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			AbsolutePath::new(dir.path()).unwrap(),
			crate::Env::new(
				Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>,
				Arc::new(crate::filesystem::LocalFilesystem),
				Arc::new(crate::git::GitWorkdir::new(
					Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>,
					crate::path::AbsolutePath::new("/tmp").unwrap(),
				)),
			),
		));
		let projects = enumerate_projects([adapter.clone()]).await.unwrap();

		assert_eq!(projects.len(), 1);
		assert_eq!(projects[0].name(), "test");
		// Verify the adapter is attached (Arc strong count > 1)
		assert!(Arc::strong_count(&projects[0].adapter) >= 2);
	}

	#[tokio::test]
	async fn enumerate_projects_flattens_multiple_adapters() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("package.json"),
			r#"{"name": "npm-pkg", "version": "0.1.0"}"#,
		)
		.unwrap();

		// Two adapters pointing at the same directory (both will find the package)
		let adapter1: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			AbsolutePath::new(dir.path()).unwrap(),
			crate::Env::new(
				Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>,
				Arc::new(crate::filesystem::LocalFilesystem),
				Arc::new(crate::git::GitWorkdir::new(
					Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>,
					crate::path::AbsolutePath::new("/tmp").unwrap(),
				)),
			),
		));
		let adapter2: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
			NpmConfig::default(),
			AbsolutePath::new(dir.path()).unwrap(),
			crate::Env::new(
				Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>,
				Arc::new(crate::filesystem::LocalFilesystem),
				Arc::new(crate::git::GitWorkdir::new(
					Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>,
					crate::path::AbsolutePath::new("/tmp").unwrap(),
				)),
			),
		));

		let projects = enumerate_projects([adapter1, adapter2]).await.unwrap();

		// Both adapters find the same package, so we get 2 projects
		assert_eq!(projects.len(), 2);
		assert_eq!(projects[0].name(), "npm-pkg");
		assert_eq!(projects[1].name(), "npm-pkg");
	}

	#[tokio::test]
	async fn enumerate_projects_empty_adapters_returns_empty() {
		let _dir = tempfile::tempdir().unwrap();
		let adapters: [Arc<dyn PackageManagerAdapter>; 0] = [];
		let projects = enumerate_projects(adapters).await.unwrap();
		assert!(projects.is_empty());
	}

	#[test]
	fn filter_projects_empty_names_returns_all() {
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("b", "/nonexistent/packages/b"),
		];
		let result = filter_projects_by_name(&projects, &[]).unwrap();
		assert_eq!(result.len(), 2);
	}

	#[test]
	fn filter_projects_selects_matching() {
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("b", "/nonexistent/packages/b"),
			Project::new_test("c", "/nonexistent/packages/c"),
		];
		let names = vec!["b".to_string(), "c".to_string()];
		let result = filter_projects_by_name(&projects, &names).unwrap();
		assert_eq!(result.len(), 2);
		assert_eq!(result[0].name(), "b");
		assert_eq!(result[1].name(), "c");
	}

	#[test]
	fn filter_projects_unknown_name_returns_error() {
		let projects = vec![Project::new_test("a", "/nonexistent/packages/a")];
		let names = vec!["nonexistent".to_string()];
		let result = filter_projects_by_name(&projects, &names);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("Unknown package: nonexistent")
		);
	}

	// ── validate_package_names ────────────────────────────────────────────────

	#[test]
	fn validate_package_names_all_known_returns_ok() {
		let projects = vec![
			Project::new_test("a", "/nonexistent/packages/a"),
			Project::new_test("b", "/nonexistent/packages/b"),
		];
		assert!(validate_package_names(&projects, &["a".to_string(), "b".to_string()]).is_ok());
	}

	#[test]
	fn validate_package_names_empty_list_returns_ok() {
		let projects = vec![Project::new_test("a", "/nonexistent/packages/a")];
		assert!(validate_package_names(&projects, &[]).is_ok());
	}

	#[test]
	fn validate_package_names_unknown_name_returns_error() {
		let projects = vec![Project::new_test("a", "/nonexistent/packages/a")];
		let result = validate_package_names(&projects, &["unknown".to_string()]);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("Unknown package: unknown")
		);
	}

	// ── is_releasable_under ───────────────────────────────────────────────────

	#[test]
	fn is_releasable_under_publishable_project_is_releasable() {
		let project = Project::new_test("my-lib", "/nonexistent/packages/my-lib");
		let config = crate::model::config::Config::new();
		assert!(project.is_releasable_under(&config));
	}

	#[test]
	fn is_releasable_under_non_publishable_not_listed_is_not_releasable() {
		let project =
			Project::new_test_not_publishable("private-tool", "/nonexistent/packages/private-tool");
		let config = crate::model::config::Config::new();
		assert!(!project.is_releasable_under(&config));
	}

	#[test]
	fn is_releasable_under_non_publishable_listed_is_releasable() {
		let project =
			Project::new_test_not_publishable("private-tool", "/nonexistent/packages/private-tool");
		let config = crate::model::config::Config::new().with_git(
			crate::model::config::GitConfig::default()
				.with_publish_private_packages(vec!["private-tool".to_string()]),
		);
		assert!(project.is_releasable_under(&config));
	}

	#[test]
	fn is_releasable_under_non_publishable_different_name_listed_is_not_releasable() {
		let project =
			Project::new_test_not_publishable("private-tool", "/nonexistent/packages/private-tool");
		let config = crate::model::config::Config::new().with_git(
			crate::model::config::GitConfig::default()
				.with_publish_private_packages(vec!["other-tool".to_string()]),
		);
		assert!(!project.is_releasable_under(&config));
	}

	// ── is_prepared_for_release ───────────────────────────────────────────────

	#[tokio::test]
	async fn is_prepared_for_release_returns_true_when_changelog_exists() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("CHANGELOG.md"), "# Changelog").unwrap();
		let project = Project::new_test("my-lib", dir.path().to_str().unwrap());
		let fs = crate::filesystem::LocalFilesystem;
		assert!(project.is_prepared_for_release(&fs).await.unwrap());
	}

	#[tokio::test]
	async fn is_prepared_for_release_returns_false_when_changelog_absent() {
		let dir = tempfile::tempdir().unwrap();
		let project = Project::new_test("my-lib", dir.path().to_str().unwrap());
		let fs = crate::filesystem::LocalFilesystem;
		assert!(!project.is_prepared_for_release(&fs).await.unwrap());
	}
}
