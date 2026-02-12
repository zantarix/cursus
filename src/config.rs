use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
	// Add config fields here as needed
}

fn path(git_root: &Path) -> PathBuf {
	git_root.join(".chronicle/config.toml")
}

pub fn exists(git_root: &Path) -> bool {
	path(git_root).exists()
}

pub fn load(git_root: &Path) -> anyhow::Result<Config> {
	let path = path(git_root);
	let contents = std::fs::read_to_string(&path)
		.with_context(|| format!("Failed to read config file: {}", path.display()))?;
	let config: Config =
		toml::from_str(&contents).with_context(|| "Failed to parse config.toml")?;
	Ok(config)
}

pub fn create(git_root: &Path) -> anyhow::Result<PathBuf> {
	let path = path(git_root);
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)
			.with_context(|| format!("Failed to create directory: {}", parent.display()))?;
	}
	std::fs::write(&path, "")
		.with_context(|| format!("Failed to create config: {}", path.display()))?;
	Ok(path)
}
