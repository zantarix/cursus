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
const MAX_BYTES: usize = 1024;

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

#[cfg(test)]
mod tests {
	use super::*;

	// ── validate_cargo_package_name ──────────────────────────────────────────

	#[test]
	fn cargo_accepts_typical_names() {
		assert!(validate_cargo_package_name("my-crate").is_ok());
		assert!(validate_cargo_package_name("my_crate").is_ok());
		assert!(validate_cargo_package_name("MyCrate").is_ok());
		assert!(validate_cargo_package_name("_internal").is_ok());
		assert!(validate_cargo_package_name("1st-crate").is_ok());
	}

	#[test]
	fn cargo_accepts_unicode_names() {
		assert!(validate_cargo_package_name("données").is_ok());
		assert!(validate_cargo_package_name("bibliothèque").is_ok());
	}

	#[test]
	fn cargo_rejects_empty() {
		let err = validate_cargo_package_name("").unwrap_err();
		assert!(err.to_string().contains("must not be empty"));
	}

	#[test]
	fn cargo_rejects_leading_dash() {
		let err = validate_cargo_package_name("--upload-pack=evil").unwrap_err();
		assert!(err.to_string().contains("must not start with '-'"));
	}

	#[test]
	fn cargo_rejects_control_char() {
		assert!(validate_cargo_package_name("my\x00crate").is_err());
		assert!(validate_cargo_package_name("my\ncrate").is_err());
	}

	// ── validate_npm_package_name ────────────────────────────────────────────

	#[test]
	fn npm_accepts_typical_names() {
		assert!(validate_npm_package_name("my-app").is_ok());
		assert!(validate_npm_package_name("my.app").is_ok());
		assert!(validate_npm_package_name("MyApp").is_ok());
		assert!(validate_npm_package_name("cursus").is_ok());
	}

	#[test]
	fn npm_accepts_scoped_names() {
		assert!(validate_npm_package_name("@my-org/my-app").is_ok());
		assert!(validate_npm_package_name("@cursus-test/app").is_ok());
		assert!(validate_npm_package_name("@scope/pkg-a").is_ok());
		assert!(validate_npm_package_name("@test/utils").is_ok());
	}

	#[test]
	fn npm_rejects_empty() {
		let err = validate_npm_package_name("").unwrap_err();
		assert!(err.to_string().contains("must not be empty"));
	}

	#[test]
	fn npm_rejects_leading_dash() {
		let err = validate_npm_package_name("--exec=evil").unwrap_err();
		assert!(err.to_string().contains("must not start with '-'"));
	}

	#[test]
	fn npm_rejects_control_char() {
		assert!(validate_npm_package_name("my\x00pkg").is_err());
		assert!(validate_npm_package_name("my\npkg").is_err());
	}

	#[test]
	fn npm_rejects_scoped_missing_slash() {
		let err = validate_npm_package_name("@scope-no-slash").unwrap_err();
		assert!(err.to_string().contains("@scope/name"));
	}

	#[test]
	fn npm_rejects_scoped_empty_scope() {
		assert!(validate_npm_package_name("@/pkg").is_err());
	}

	#[test]
	fn npm_rejects_scoped_empty_name() {
		assert!(validate_npm_package_name("@scope/").is_err());
	}

	#[test]
	fn npm_rejects_scoped_leading_dash_in_scope() {
		assert!(validate_npm_package_name("@-scope/pkg").is_err());
	}

	#[test]
	fn npm_rejects_scoped_leading_dash_in_name() {
		assert!(validate_npm_package_name("@scope/-pkg").is_err());
	}

	#[test]
	fn rejects_over_1kb() {
		let long = "a".repeat(MAX_BYTES + 1);
		assert!(validate_cargo_package_name(&long).is_err());
		assert!(validate_npm_package_name(&long).is_err());
	}

	#[test]
	fn cargo_accepts_exactly_1kb() {
		let at_limit = "a".repeat(MAX_BYTES);
		assert!(validate_cargo_package_name(&at_limit).is_ok());
	}
}
