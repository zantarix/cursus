//! The `release` subcommand.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use clap::Args;

use crate::command::CommandRunner;
use crate::git::{self, ReleaseInfo};
use crate::model::changelog::Changelog;
use crate::model::changeset::{ChangeType, Changeset};
use crate::model::config;
use crate::package_manager::filter_projects_by_name;
use crate::utils::today_iso_date;

/// Arguments for the `release` subcommand.
#[derive(Args, Default)]
pub struct ReleaseArgs {
	/// Preview changes without modifying any files
	#[arg(long)]
	pub dry_run: bool,

	/// Only release specific packages (repeatable)
	#[arg(short = 'p', long = "package")]
	pub packages: Vec<String>,

	/// Skip git lifecycle automation even if enabled in config
	#[arg(long)]
	pub no_git: bool,
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

/// Runs the `release` subcommand.
pub fn cmd_release(
	git_workdir: &Path,
	args: &ReleaseArgs,
	runner: Arc<dyn CommandRunner>,
) -> anyhow::Result<ExitCode> {
	let config = config::load(git_workdir)?;
	let adapters = config.create_adapters(Arc::clone(&runner));
	let projects = config.load_projects_for_adapters(&adapters)?;

	// Read all pending changesets
	let changesets = Changeset::read_all(config.git_workdir())?;
	if changesets.is_empty() {
		println!("No pending changesets found. Nothing to release.");
		return Ok(ExitCode::SUCCESS);
	}

	// Aggregate: find the maximum change type per package
	let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
	for (_, cs) in &changesets {
		for (pkg, ct) in &cs.packages {
			aggregated
				.entry(pkg.clone())
				.and_modify(|e| *e = (*e).max(*ct))
				.or_insert(*ct);
		}
	}

	// Collect changes per package for changelog: (ChangeType, Option<message>)
	let mut changes_per_package: BTreeMap<String, Vec<(ChangeType, Option<String>)>> =
		BTreeMap::new();
	for (_, cs) in &changesets {
		for (pkg, ct) in &cs.packages {
			changes_per_package
				.entry(pkg.clone())
				.or_default()
				.push((*ct, cs.message.clone()));
		}
	}

	// Filter by --package flags if specified
	if !args.packages.is_empty() {
		// Validate all requested packages exist
		filter_projects_by_name(&projects, &args.packages)?;

		// Filter aggregated and changes_per_package to only include requested packages
		aggregated.retain(|name, _| args.packages.contains(name));
		changes_per_package.retain(|name, _| args.packages.contains(name));
	}

	let mut release_infos: Vec<ReleaseInfo> = Vec::new();
	let mut modified_files: Vec<PathBuf> = Vec::new();

	// Process each affected package
	for (pkg_name, change_type) in &aggregated {
		let project = projects
			.iter()
			.find(|p| p.name() == pkg_name)
			.with_context(|| {
				format!("Package '{pkg_name}' from changeset not found in projects")
			})?;

		let current_version = project.version();
		let new_version = bump_version(current_version, *change_type);

		// Always track which files would be staged (used for git lifecycle and dry-run display).
		modified_files.push(project.manifest_path(git_workdir));
		modified_files.push(git_workdir.join(project.path()).join("CHANGELOG.md"));

		if args.dry_run {
			println!("{pkg_name}: {current_version} -> {new_version} ({change_type})");
		} else {
			project.write_version(&new_version)?;

			// Generate changelog
			let changes = changes_per_package
				.get(pkg_name)
				.map(|v| v.as_slice())
				.unwrap_or_default()
				.to_vec();
			Changelog::new(
				new_version.clone(),
				today_iso_date(),
				changes,
				project.path().to_path_buf(),
			)
			.update(config.git_workdir())?;

			println!("{pkg_name}: {current_version} -> {new_version} ({change_type})");
		}

		release_infos.push(ReleaseInfo {
			package_name: pkg_name.clone(),
			new_version,
		});
	}

	// Build map of bumped package names → new versions for dependency propagation.
	let bumped_versions: BTreeMap<String, semver::Version> = release_infos
		.iter()
		.map(|info| (info.package_name.clone(), info.new_version.clone()))
		.collect();

	// Update intra-workspace dependency references for all projects.
	for project in &projects {
		for dep_name in project.dependency_names() {
			if let Some(new_version) = bumped_versions.get(dep_name.as_str()) {
				if args.dry_run {
					println!(
						"  {}: would update dependency {} to {}",
						project.name(),
						dep_name,
						new_version
					);
					// Predict the manifest that would be modified so git lifecycle
					// dry-run can report it as a file that would be staged.
					modified_files.push(project.manifest_path(git_workdir));
				} else {
					let paths = project.update_dependency_version(dep_name, new_version)?;
					modified_files.extend(paths);
				}
			}
		}
	}

	// Collect lock file paths. During dry-run, use lock_file_path() to predict which
	// file would be updated without running the update command.
	for adapter in &adapters {
		if args.dry_run {
			if let Some(lock_path) = adapter.lock_file_path() {
				modified_files.push(lock_path);
			}
		} else if let Some(lock_path) = adapter.update_lock_file()? {
			modified_files.push(lock_path);
		}
	}

	// Consume changesets: delete fully consumed, rewrite partially consumed.
	// Always track which changesets would be staged, but only consume during a real release.
	let released: BTreeSet<String> = aggregated.keys().cloned().collect();
	for (path, cs) in &changesets {
		// Only stage changesets that touch at least one released package
		if cs.packages.keys().any(|name| released.contains(name)) {
			modified_files.push(path.clone());
		}
		if !args.dry_run {
			cs.consume(path, &released)?;
		}
	}

	// Deduplicate modified files (e.g. workspace root Cargo.toml updated by multiple projects)
	modified_files.sort();
	modified_files.dedup();

	// Run git lifecycle if enabled and not suppressed
	if config.git.enabled && !args.no_git {
		git::run_git_lifecycle(
			git_workdir,
			&config.git,
			&release_infos,
			&modified_files,
			projects.len(),
			args.dry_run,
			runner.as_ref(),
		)?;
	}

	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use crate::command::test_support::RecordingCommandRunner;

	use super::*;

	fn make_runner() -> Arc<dyn CommandRunner> {
		Arc::new(RecordingCommandRunner::new(0))
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
	fn cmd_release_no_config_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let args = ReleaseArgs::default();
		let result = cmd_release(dir.path(), &args, make_runner());
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("No configuration found")
		);
	}

	#[test]
	fn cmd_release_no_changesets_succeeds() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let config = crate::model::config::Config::new(dir.path())
			.with_cargo(crate::package_manager::CargoConfig::enabled());
		config.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();

		let args = ReleaseArgs::default();
		let result = cmd_release(dir.path(), &args, make_runner()).unwrap();
		assert_eq!(result, ExitCode::SUCCESS);
	}

	#[test]
	fn cmd_release_unknown_package_in_changeset_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let config = crate::model::config::Config::new(dir.path())
			.with_cargo(crate::package_manager::CargoConfig::enabled());
		config.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"real-project\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();
		// Changeset references a package that doesn't exist
		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::write(
			chronicle_dir.join("test.md"),
			"+++\nnonexistent-package = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let args = ReleaseArgs::default();
		let result = cmd_release(dir.path(), &args, make_runner());
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
		let config = crate::model::config::Config::new(dir.path())
			.with_cargo(crate::package_manager::CargoConfig::enabled());
		config.save().unwrap();
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
	fn cmd_release_package_flag_filters_packages() {
		let dir = setup_two_package_workspace();

		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		let changeset_path = chronicle_dir.join("test.md");
		std::fs::write(
			&changeset_path,
			"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let args = ReleaseArgs {
			dry_run: false,
			packages: vec!["pkg-a".to_string()],
			no_git: true,
		};
		let result = cmd_release(dir.path(), &args, make_runner());
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
	fn cmd_release_package_flag_with_dry_run_leaves_changeset_untouched() {
		let dir = setup_two_package_workspace();

		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::create_dir_all(&chronicle_dir).unwrap();
		let changeset_path = chronicle_dir.join("test.md");
		let original = "+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n";
		std::fs::write(&changeset_path, original).unwrap();

		let args = ReleaseArgs {
			dry_run: true,
			packages: vec!["pkg-a".to_string()],
			no_git: true,
		};
		let result = cmd_release(dir.path(), &args, make_runner());
		assert!(result.is_ok());

		// Dry-run must not touch the changeset even when scoped
		let content = std::fs::read_to_string(&changeset_path).unwrap();
		assert_eq!(
			content, original,
			"Changeset should be untouched in dry-run"
		);
	}

	#[test]
	fn cmd_release_unknown_package_flag_fails() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join(".git")).unwrap();
		let config = crate::model::config::Config::new(dir.path())
			.with_cargo(crate::package_manager::CargoConfig::enabled());
		config.save().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			"[package]\nname = \"real-project\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();

		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::write(
			chronicle_dir.join("test.md"),
			"+++\nreal-project = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let args = ReleaseArgs {
			dry_run: false,
			packages: vec!["nonexistent".to_string()],
			no_git: true,
		};
		let result = cmd_release(dir.path(), &args, make_runner());
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("Unknown package: nonexistent")
		);
	}
}
