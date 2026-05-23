//! Validation for package names from Cargo and npm manifests.
//!
//! Malicious package names (e.g. `--upload-pack=evil`) can reach `git tag`
//! argv positionally. Validating at manifest parse time prevents this at the
//! deepest point in the data pipeline.
//!
//! Validators are security-focused: they reject leading `-` (argv-smuggling)
//! and ASCII control characters (null-byte/newline injection), but otherwise
//! accept any name the respective ecosystem allows — including Unicode
//! identifiers for Cargo and scoped names for npm.

use anyhow::bail;

/// Validates a Cargo package name.
///
/// Rejects empty names, names starting with `-`, and names containing ASCII
/// control characters. Does not enforce crates.io naming conventions.
///
/// # Errors
///
/// Returns an error describing the first violation found.
pub(crate) const MAX_BYTES: usize = 1024;

pub fn validate_cargo_package_name(name: &str) -> anyhow::Result<()> {
	if name.is_empty() {
		bail!("Cargo package name must not be empty");
	}
	if name.len() > MAX_BYTES {
		bail!(
			"Cargo package name is too long (max {MAX_BYTES} bytes): {} bytes",
			name.len()
		);
	}
	if name.starts_with('-') {
		bail!("Cargo package name must not start with '-': {name:?}");
	}
	if let Some(c) = name.chars().find(|c| c.is_ascii_control()) {
		bail!("Cargo package name contains control character {c:?}: {name:?}");
	}
	Ok(())
}

/// Validates an npm package name.
///
/// Accepts unscoped names and scoped names of the form `@scope/name`.
/// Rejects empty names, names starting with `-`, and names containing ASCII
/// control characters. Does not enforce npm registry naming conventions.
///
/// # Errors
///
/// Returns an error describing the first violation found.
pub fn validate_npm_package_name(name: &str) -> anyhow::Result<()> {
	if name.is_empty() {
		bail!("npm package name must not be empty");
	}
	if name.len() > MAX_BYTES {
		bail!(
			"npm package name is too long (max {MAX_BYTES} bytes): {} bytes",
			name.len()
		);
	}
	if let Some(rest) = name.strip_prefix('@') {
		// Scoped package: validate "@scope/name"
		let Some((scope, pkg)) = rest.split_once('/') else {
			bail!("scoped npm package name must have the form @scope/name: {name:?}");
		};
		validate_npm_segment(scope, "scope", name)?;
		validate_npm_segment(pkg, "package", name)?;
	} else {
		validate_npm_segment(name, "package", name)?;
	}
	Ok(())
}

fn validate_npm_segment(segment: &str, label: &str, full_name: &str) -> anyhow::Result<()> {
	if segment.is_empty() {
		bail!("npm {label} name segment must not be empty: {full_name:?}");
	}
	if segment.starts_with('-') {
		bail!("npm {label} name must not start with '-': {full_name:?}");
	}
	if let Some(c) = segment.chars().find(|c| c.is_ascii_control()) {
		bail!("npm {label} name contains control character {c:?}: {full_name:?}");
	}
	Ok(())
}
