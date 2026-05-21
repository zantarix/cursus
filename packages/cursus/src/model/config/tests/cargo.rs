use crate::model::config::cargo::*;
use crate::path::AbsolutePath;

#[test]
fn cargo_config_defaults_to_disabled() {
	let config = CargoConfig::default();
	assert!(!config.enabled);
	assert_eq!(config.path, None);
}

#[test]
fn cargo_config_enabled_creates_enabled_config() {
	let config = CargoConfig::enabled();
	assert!(config.enabled);
	assert_eq!(config.path, None);
}

#[tokio::test]
async fn cargo_config_resolve_root_without_path() {
	let config = CargoConfig {
		enabled: true,
		path: None,
	};
	let dir = tempfile::tempdir().unwrap();
	let git_workdir = AbsolutePath::new(dir.path()).unwrap();
	let resolved = config
		.resolve_root(&git_workdir, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();
	assert_eq!(resolved, git_workdir);
}

#[tokio::test]
async fn cargo_config_resolve_root_with_path() {
	let dir = tempfile::tempdir().unwrap();
	let subdir = dir.path().join("rust-workspace");
	std::fs::create_dir(&subdir).unwrap();
	let config = CargoConfig {
		enabled: true,
		path: Some("rust-workspace".to_string()),
	};
	let git_workdir = AbsolutePath::new(dir.path()).unwrap();
	let resolved = config
		.resolve_root(&git_workdir, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();
	assert_eq!(*resolved, *AbsolutePath::new(&subdir).unwrap());
}

#[tokio::test]
async fn cargo_config_resolve_root_rejects_traversal() {
	let outer = tempfile::tempdir().unwrap();
	let repo = outer.path().join("repo");
	std::fs::create_dir(&repo).unwrap();
	let config = CargoConfig {
		enabled: true,
		path: Some("../escape".to_string()),
	};
	let escape_dir = outer.path().join("escape");
	std::fs::create_dir(&escape_dir).unwrap();
	let git_workdir = AbsolutePath::new(&repo).unwrap();
	let result = config
		.resolve_root(&git_workdir, &crate::filesystem::LocalFilesystem)
		.await;
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("escapes repository root")
	);
}
