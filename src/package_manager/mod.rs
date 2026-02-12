//! Package manager adapters for project enumeration and management.
//!
//! This module provides a trait-based abstraction over different package managers,
//! allowing Chronicle to work with various ecosystems (npm, Cargo, etc.) through
//! a unified interface.

mod npm;

pub use npm::NpmAdapter;

use std::path::Path;

/// Represents a project discovered by a package manager adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
	/// The name of the project (e.g., package name).
	pub name: String,
	/// The path to the project root, relative to the git root.
	pub path: std::path::PathBuf,
}

/// Trait for package manager adapters.
///
/// Implementations of this trait provide package-manager-specific functionality
/// for discovering and managing projects within a repository.
pub trait PackageManagerAdapter {
	/// Enumerates all projects managed by this package manager.
	///
	/// For single-package repositories, this returns a single project.
	/// For monorepos, this returns all workspace packages.
	///
	/// # Arguments
	///
	/// * `git_root` - The root directory of the git repository.
	///
	/// # Errors
	///
	/// Returns an error if project enumeration fails (e.g., invalid manifest files).
	fn enumerate_projects(&self, git_root: &Path) -> anyhow::Result<Vec<Project>>;
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn project_equality() {
		let p1 = Project {
			name: "test".to_string(),
			path: std::path::PathBuf::from("packages/test"),
		};
		let p2 = Project {
			name: "test".to_string(),
			path: std::path::PathBuf::from("packages/test"),
		};
		let p3 = Project {
			name: "other".to_string(),
			path: std::path::PathBuf::from("packages/other"),
		};

		assert_eq!(p1, p2);
		assert_ne!(p1, p3);
	}

	#[test]
	fn project_debug() {
		let project = Project {
			name: "my-package".to_string(),
			path: std::path::PathBuf::from("packages/my-package"),
		};
		let debug = format!("{:?}", project);
		assert!(debug.contains("my-package"));
	}

	#[test]
	fn project_clone() {
		let project = Project {
			name: "test".to_string(),
			path: std::path::PathBuf::from("src"),
		};
		let cloned = project.clone();
		assert_eq!(project, cloned);
	}
}
