mod matching;
mod name_validation;

use std::path::Path;
use std::sync::Arc;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::model::config::NpmConfig;
use crate::package_manager::*;

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
