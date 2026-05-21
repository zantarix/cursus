use std::sync::Arc;

use crate::cli::prepare::release_files::*;
use crate::cli::prepare::{PrepareArgs, ReleaseInfo, cmd_prepare};
use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::filesystem::LocalFilesystem;
use crate::model::config;

fn make_runner() -> Arc<dyn CommandRunner> {
	Arc::new(RecordingCommandRunner::new(0))
}

/// Loads all Cargo projects from a temporary workspace directory.
async fn load_projects_for_dir(dir: &tempfile::TempDir) -> Vec<crate::package_manager::Project> {
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
	let args = PrepareArgs {
		packages: vec!["pkg-a".to_string()],
		no_git: true,
		..PrepareArgs::default()
	};
	let result = cmd_prepare(&args, false, &env, config).await;
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
	let release_infos = vec![ReleaseInfo {
		package_name: "pkg-a".to_string(),
		new_version: "1.0.1".parse().unwrap(),
		changelog_entry: String::new(),
	}];
	let modified_paths = propagate_dependency_updates(&projects, &release_infos, false)
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
	let args = PrepareArgs {
		packages: vec!["pkg-a".to_string()],
		no_git: true,
		..PrepareArgs::default()
	};
	let result = cmd_prepare(&args, true, &env, config).await;
	assert!(result.is_ok());

	// Dry-run must not touch the changeset even when scoped
	let content = std::fs::read_to_string(&changeset_path).unwrap();
	assert_eq!(
		content, original,
		"Changeset should be untouched in dry-run"
	);
}

/// Sets up a Cargo workspace where both members inherit `version.workspace = true`
/// with global linked-versions enabled (required by `validate_workspace_version_linking`).
async fn setup_workspace_version_workspace_true() -> tempfile::TempDir {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let setup_env = make_test_env(dir.path());
	crate::model::config::Config::new()
		.with_cargo(crate::model::config::CargoConfig::enabled())
		.with_linked_versions(crate::model::config::LinkedVersionsConfig {
			enabled: Some(true),
			groups: Vec::new(),
		})
		.save(setup_env.fs(), setup_env.git().path())
		.await
		.unwrap();
	// Workspace root Cargo.toml — defines [workspace.package].version
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[workspace]\nmembers = [\"pkg-a\", \"pkg-b\"]\n\n[workspace.package]\nversion = \"0.1.0\"\n",
	)
	.unwrap();
	for pkg in ["pkg-a", "pkg-b"] {
		std::fs::create_dir_all(dir.path().join(pkg).join("src")).unwrap();
		std::fs::write(
			dir.path().join(pkg).join("Cargo.toml"),
			format!("[package]\nname = \"{pkg}\"\nversion.workspace = true\nedition = \"2024\"\n"),
		)
		.unwrap();
		std::fs::write(dir.path().join(pkg).join("src").join("lib.rs"), "").unwrap();
	}
	dir
}

/// Regression test: when Cargo workspace members use `version.workspace = true`,
/// `bump_versions_and_generate_changelogs` must include the workspace root
/// `Cargo.toml` (not the member manifests) in its returned modified-files list.
///
/// Before the fix, the member path was pushed unconditionally and `write_version`
/// wrote silently to the workspace root without reporting it — causing the root
/// `Cargo.toml` to be omitted from the release commit.
#[tokio::test]
async fn bump_versions_includes_workspace_root_when_version_workspace_true() {
	let dir = setup_workspace_version_workspace_true().await;
	let projects = load_projects_for_dir(&dir).await;

	// Both packages bump by patch
	let aggregated: std::collections::BTreeMap<String, crate::model::changeset::ChangeType> = [
		(
			"pkg-a".to_string(),
			crate::model::changeset::ChangeType::Patch,
		),
		(
			"pkg-b".to_string(),
			crate::model::changeset::ChangeType::Patch,
		),
	]
	.into_iter()
	.collect();

	let (_, modified_files) = bump_versions_and_generate_changelogs(
		&aggregated,
		&std::collections::BTreeMap::new(),
		&projects,
		&std::collections::BTreeMap::new(),
		&std::collections::BTreeMap::new(),
		false,
		&crate::filesystem::LocalFilesystem,
	)
	.await
	.unwrap();

	let root_cargo = dir.path().join("Cargo.toml");
	assert!(
		modified_files.contains(&root_cargo),
		"Workspace root Cargo.toml must be in modified_files so it is staged for git.\n\
		 Got: {modified_files:?}"
	);

	// Member manifests must NOT appear — they only declare `version.workspace = true`
	// and were not written to.
	for pkg in ["pkg-a", "pkg-b"] {
		let member_cargo = dir.path().join(pkg).join("Cargo.toml");
		assert!(
			!modified_files.contains(&member_cargo),
			"Member Cargo.toml ({pkg}) must not be in modified_files — it was not modified.\n\
			 Got: {modified_files:?}"
		);
	}
}
