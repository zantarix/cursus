use std::sync::Arc;

use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::model::config::NpmConfig;
use crate::package_manager::{
	NpmAdapter, PackageManagerAdapter, build_dependency_graph, enumerate_projects,
};
use crate::path::AbsolutePath;

#[test]
fn build_dependency_graph_empty_for_single_package() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "test-package", "version": "0.1.0"}"#,
	)
	.unwrap();

	let adapter: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
		NpmConfig::default(),
		AbsolutePath::new(dir.path()).unwrap(),
		crate::Env::new(Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>),
	));
	let projects = enumerate_projects([adapter]).unwrap();
	let graph = build_dependency_graph(&projects).unwrap();

	// Single package with no dependencies should result in trivial sorting
	let sorted = graph.sort_leaves_first();
	assert_eq!(sorted, vec!["test-package"]);
}

#[test]
fn build_dependency_graph_with_workspace_dependencies() {
	let dir = tempfile::tempdir().unwrap();

	// Create workspace with dependencies
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/lib")).unwrap();
	std::fs::write(
		dir.path().join("packages/lib/package.json"),
		r#"{"name": "lib", "version": "0.1.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "app", "version": "0.1.0", "dependencies": {"lib": "0.1.0"}}"#,
	)
	.unwrap();

	let adapter: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
		NpmConfig::default(),
		AbsolutePath::new(dir.path()).unwrap(),
		crate::Env::new(Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>),
	));
	let projects = enumerate_projects([adapter]).unwrap();
	let graph = build_dependency_graph(&projects).unwrap();

	// app depends on lib, so lib should come before app
	let sorted = graph.sort_leaves_first();

	// lib should be published before app (lib is a dependency of app)
	let lib_index = sorted.iter().position(|n| n == "lib").unwrap();
	let app_index = sorted.iter().position(|n| n == "app").unwrap();
	assert!(lib_index < app_index, "lib should come before app");
}

#[test]
fn build_dependency_graph_excludes_external_dependencies() {
	let dir = tempfile::tempdir().unwrap();

	// Create workspace where app depends on both workspace lib and external react
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "0.1.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/lib")).unwrap();
	std::fs::write(
		dir.path().join("packages/lib/package.json"),
		r#"{"name": "lib", "version": "0.1.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "app", "version": "0.1.0", "dependencies": {"lib": "0.1.0", "react": "^18.0.0"}}"#,
	)
	.unwrap();

	let adapter: Arc<dyn PackageManagerAdapter> = Arc::new(NpmAdapter::new(
		NpmConfig::default(),
		AbsolutePath::new(dir.path()).unwrap(),
		crate::Env::new(Arc::new(RecordingCommandRunner::new(0)) as Arc<dyn CommandRunner>),
	));
	let projects = enumerate_projects([adapter]).unwrap();
	let graph = build_dependency_graph(&projects).unwrap();

	// Verify that app's adjacency list only includes lib, not react
	assert_eq!(
		graph.adjacency.get("app").unwrap(),
		&vec!["lib".to_string()]
	);

	// Verify react is not in the graph at all
	assert!(!graph.adjacency.contains_key("react"));

	// Verify topological sort still works correctly
	let sorted = graph.sort_leaves_first();

	// react should not appear in the sorted output
	assert!(!sorted.contains(&"react".to_string()));

	// lib should come before app
	let lib_index = sorted.iter().position(|n| n == "lib").unwrap();
	let app_index = sorted.iter().position(|n| n == "app").unwrap();
	assert!(lib_index < app_index, "lib should come before app");
}
