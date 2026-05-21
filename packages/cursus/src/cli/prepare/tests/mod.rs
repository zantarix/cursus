mod changeset;
mod git_lifecycle;
mod github;
mod linked_versions;
mod propagation;
mod release_files;
mod version;

use std::sync::Arc;

use crate::cli::prepare::*;
use crate::command::CommandRunner;
use crate::command::test_support::RecordingCommandRunner;
use crate::filesystem::LocalFilesystem;
use crate::model::config;

fn make_runner() -> Arc<dyn CommandRunner> {
	Arc::new(RecordingCommandRunner::new(0))
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
async fn cmd_prepare_no_changesets_succeeds() {
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
		"[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let args = PrepareArgs::default();
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
	let result = cmd_prepare(&args, false, &env, config).await.unwrap();
	assert_eq!(result, std::process::ExitCode::SUCCESS);
}

#[tokio::test]
async fn cmd_prepare_unknown_package_in_changeset_fails() {
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
	let result = cmd_prepare(&args, false, &env, config).await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("not found in projects")
	);
}

#[tokio::test]
async fn cmd_prepare_unknown_package_flag_fails() {
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
		packages: vec!["nonexistent".to_string()],
		no_git: true,
		..PrepareArgs::default()
	};
	let result = cmd_prepare(&args, false, &env, config).await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("Unknown package: nonexistent")
	);
}
