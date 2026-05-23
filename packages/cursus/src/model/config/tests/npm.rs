use crate::model::config::npm::*;
use crate::path::AbsolutePath;

#[test]
fn npm_config_defaults_to_disabled() {
	let config = NpmConfig::default();
	assert!(!config.enabled);
	assert_eq!(config.path, None);
	assert_eq!(config.lock_command, None);
	assert_eq!(config.access(), NpmAccess::Restricted);
}

#[test]
fn npm_config_enabled_creates_enabled_config() {
	let config = NpmConfig::enabled();
	assert!(config.enabled);
	assert_eq!(config.path, None);
	assert_eq!(config.lock_command, None);
	assert_eq!(config.access(), NpmAccess::Restricted);
}

#[tokio::test]
async fn npm_config_resolve_root_without_path() {
	let config = NpmConfig {
		enabled: true,
		path: None,
		lock_command: None,
		access: None,
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
async fn npm_config_resolve_root_with_path() {
	let dir = tempfile::tempdir().unwrap();
	let subdir = dir.path().join("frontend");
	std::fs::create_dir(&subdir).unwrap();
	let config = NpmConfig {
		enabled: true,
		path: Some("frontend".to_string()),
		lock_command: None,
		access: None,
	};
	let git_workdir = AbsolutePath::new(dir.path()).unwrap();
	let resolved = config
		.resolve_root(&git_workdir, &crate::filesystem::LocalFilesystem)
		.await
		.unwrap();
	assert_eq!(*resolved, *AbsolutePath::new(&subdir).unwrap());
}

#[tokio::test]
async fn npm_config_resolve_root_rejects_traversal() {
	let outer = tempfile::tempdir().unwrap();
	let repo = outer.path().join("repo");
	std::fs::create_dir(&repo).unwrap();
	let escape_dir = outer.path().join("escape");
	std::fs::create_dir(&escape_dir).unwrap();
	let config = NpmConfig {
		enabled: true,
		path: Some("../escape".to_string()),
		lock_command: None,
		access: None,
	};
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
