//! Shared test helpers for integration tests.

#![allow(dead_code)]

/// Runs chronicle with a default (empty) environment, returning the result.
///
/// This is the standard way to invoke `chronicle::run` from integration tests.
/// It passes `Env::default()` so that no real environment variables are read.
pub fn run_chronicle(
	args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
	cwd: &std::path::Path,
) -> anyhow::Result<std::process::ExitCode> {
	chronicle::run(args, cwd, chronicle::Env::default())
}

use chronicle::model::config::{Config, PackageManager};
use chronicle::package_manager::{CargoConfig, NpmConfig};
use tempfile::TempDir;

/// Creates a temporary directory with a `.git` folder to simulate a git repository.
pub fn temp_git_repo() -> TempDir {
	let dir = tempfile::tempdir().expect("Failed to create temp dir");
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	dir
}

/// Creates a temporary git repository with a Chronicle config file.
pub fn temp_git_repo_with_config(pm: PackageManager) -> TempDir {
	let dir = temp_git_repo();
	let config = match pm {
		PackageManager::Npm => Config::new(dir.path()).with_npm(NpmConfig::enabled()),
		PackageManager::Cargo => Config::new(dir.path()).with_cargo(CargoConfig::enabled()),
	};
	config.save().unwrap();
	dir
}

/// Creates a temporary git repository with a config and matching package manifest.
pub fn temp_git_repo_with_project(pm: PackageManager) -> TempDir {
	let dir = temp_git_repo();
	let config = match pm {
		PackageManager::Npm => Config::new(dir.path()).with_npm(NpmConfig::enabled()),
		PackageManager::Cargo => Config::new(dir.path()).with_cargo(CargoConfig::enabled()),
	};
	config.save().unwrap();
	match pm {
		PackageManager::Npm => {
			std::fs::write(
				dir.path().join("package.json"),
				r#"{"name": "test-project", "version": "0.1.0"}"#,
			)
			.unwrap();
		}
		PackageManager::Cargo => {
			std::fs::write(
				dir.path().join("Cargo.toml"),
				"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
			)
			.unwrap();
			// Create src/lib.rs so cargo can generate a valid Cargo.lock
			std::fs::create_dir_all(dir.path().join("src")).unwrap();
			std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
		}
	}
	dir
}

/// Creates a temporary git repository with a Cargo workspace containing named packages.
///
/// Each entry in `members` is a `(name, version)` pair. The workspace root
/// `Cargo.toml` lists all members, and each gets its own `Cargo.toml` and
/// an empty `src/lib.rs`.
pub fn temp_git_repo_with_cargo_workspace(members: &[(&str, &str)]) -> TempDir {
	let dir = temp_git_repo();
	let config = Config::new(dir.path()).with_cargo(CargoConfig::enabled());
	config.save().unwrap();

	let member_list = members
		.iter()
		.map(|(name, _)| format!("\"{name}\""))
		.collect::<Vec<_>>()
		.join(", ");
	std::fs::write(
		dir.path().join("Cargo.toml"),
		format!("[workspace]\nmembers = [{member_list}]\n"),
	)
	.unwrap();

	for (name, version) in members {
		let pkg_dir = dir.path().join(name);
		std::fs::create_dir_all(pkg_dir.join("src")).unwrap();
		std::fs::write(
			pkg_dir.join("Cargo.toml"),
			format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
		)
		.unwrap();
		std::fs::write(pkg_dir.join("src/lib.rs"), "").unwrap();
	}

	dir
}

/// Creates a temporary git repository with a config and package manifest in a subfolder.
pub fn temp_git_repo_with_project_in_subfolder(pm: PackageManager, subfolder: &str) -> TempDir {
	let dir = temp_git_repo();
	let mut config = match pm {
		PackageManager::Npm => Config::new(dir.path()).with_npm(NpmConfig::enabled()),
		PackageManager::Cargo => Config::new(dir.path()).with_cargo(CargoConfig::enabled()),
	};
	match pm {
		PackageManager::Npm => config.npm.path = Some(subfolder.to_string()),
		PackageManager::Cargo => config.cargo.path = Some(subfolder.to_string()),
	}
	config.save().unwrap();
	let sub_path = dir.path().join(subfolder);
	std::fs::create_dir_all(&sub_path).unwrap();
	match pm {
		PackageManager::Npm => {
			std::fs::write(
				sub_path.join("package.json"),
				r#"{"name": "test-project", "version": "0.1.0"}"#,
			)
			.unwrap();
		}
		PackageManager::Cargo => {
			std::fs::write(
				sub_path.join("Cargo.toml"),
				"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
			)
			.unwrap();
		}
	}
	dir
}
