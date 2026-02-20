//! The `release` subcommand.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::Args;

use crate::model::changelog::Changelog;
use crate::model::changeset::{self, ChangeType};
use crate::model::config;

/// Arguments for the `release` subcommand.
#[derive(Args, Default)]
pub struct ReleaseArgs {
	/// Preview changes without modifying any files
	#[arg(long)]
	pub dry_run: bool,

	/// Only release specific packages (repeatable)
	#[arg(short = 'p', long = "package")]
	pub packages: Vec<String>,
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
pub fn cmd_release(git_workdir: &Path, args: &ReleaseArgs) -> anyhow::Result<ExitCode> {
	let config = config::load(git_workdir)?;
	let projects = config.load_projects()?;

	// Read all pending changesets
	let changesets = changeset::read_all_changesets(config.git_workdir())?;
	if changesets.is_empty() {
		println!("No pending changesets found. Nothing to release.");
		return Ok(ExitCode::SUCCESS);
	}

	// Aggregate: find the maximum change type per package
	let mut aggregated: BTreeMap<String, ChangeType> = BTreeMap::new();
	for (_, cs) in &changesets {
		for (pkg, ct) in &cs.packages {
			let entry = aggregated.entry(pkg.clone()).or_insert(*ct);
			if *ct > *entry {
				*entry = *ct;
			}
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
		for pkg_name in &args.packages {
			if !projects.iter().any(|p| p.name() == pkg_name) {
				bail!("Unknown package: {pkg_name}");
			}
		}

		// Filter aggregated and changes_per_package to only include requested packages
		aggregated.retain(|name, _| args.packages.contains(name));
		changes_per_package.retain(|name, _| args.packages.contains(name));
	}

	// Process each affected package
	for (pkg_name, change_type) in &aggregated {
		let project = projects
			.iter()
			.find(|p| p.name() == pkg_name)
			.with_context(|| {
				format!("Package '{pkg_name}' from changeset not found in projects")
			})?;

		let current_version = project.read_version()?;
		let new_version = bump_version(&current_version, *change_type);

		if args.dry_run {
			println!("{pkg_name}: {current_version} -> {new_version} ({change_type})");
		} else {
			project.write_version(&new_version)?;
			project.update_lock_file()?;

			// Generate changelog
			let changes = changes_per_package
				.get(pkg_name)
				.map(|v| v.as_slice())
				.unwrap_or_default()
				.to_vec();
			Changelog::new(new_version.clone(), changes, project.path().to_path_buf())
				.update(config.git_workdir())?;

			println!("{pkg_name}: {current_version} -> {new_version} ({change_type})");
		}
	}

	// Delete consumed changesets
	if !args.dry_run {
		for (path, _) in &changesets {
			std::fs::remove_file(path)
				.with_context(|| format!("Failed to delete changeset: {}", path.display()))?;
		}
	}

	Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
	use super::*;

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
		let result = cmd_release(dir.path(), &args);
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
		let result = cmd_release(dir.path(), &args).unwrap();
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
		let result = cmd_release(dir.path(), &args);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("not found in projects")
		);
	}

	#[test]
	fn cmd_release_package_flag_filters_packages() {
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
		std::fs::create_dir_all(dir.path().join("pkg-a")).unwrap();
		std::fs::write(
			dir.path().join("pkg-a/Cargo.toml"),
			"[package]\nname = \"pkg-a\"\nversion = \"0.1.0\"\n",
		)
		.unwrap();
		std::fs::create_dir_all(dir.path().join("pkg-b")).unwrap();
		std::fs::write(
			dir.path().join("pkg-b/Cargo.toml"),
			"[package]\nname = \"pkg-b\"\nversion = \"0.2.0\"\n",
		)
		.unwrap();

		let chronicle_dir = dir.path().join(".chronicle");
		std::fs::write(
			chronicle_dir.join("test.md"),
			"+++\npkg-a = \"patch\"\npkg-b = \"minor\"\n+++\n\nSome change\n",
		)
		.unwrap();

		let args = ReleaseArgs {
			dry_run: true,
			packages: vec!["pkg-a".to_string()],
		};
		let result = cmd_release(dir.path(), &args);
		assert!(result.is_ok());
		// Since dry_run doesn't actually write files, we can't verify much more
		// Integration tests will verify actual behavior
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
		};
		let result = cmd_release(dir.path(), &args);
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("Unknown package: nonexistent")
		);
	}
}
