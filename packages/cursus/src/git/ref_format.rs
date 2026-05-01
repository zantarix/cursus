//! Validation for git ref names, tag names, and revision specifiers.
//!
//! The sole security goal is preventing leading-`-` inputs from being
//! interpreted as option flags by the git binary (argv-smuggling), and
//! rejecting ASCII control characters that would be pathological in any
//! subprocess argument.
//!
//! These validators deliberately do NOT enforce git's `check-ref-format`
//! naming conventions. Since tags are generated from package names and package
//! names can be any valid identifier (including Unicode), the validators must
//! accept the same wide set of names that the package managers accept.
//!
//! The trailing `--` separators added to some git invocations are belt-and-
//! suspenders for rev-vs-pathspec disambiguation; the actual argv-smuggling
//! defence is the leading-`-` check here.

use anyhow::bail;

/// Validates a git branch name.
///
/// Rejects empty names, names starting with `-`, and names containing ASCII
/// control characters. All other characters (Unicode, spaces, symbols) are
/// permitted — git is permissive and exec-mode invocations pass the value
/// literally.
///
/// # Errors
///
/// Returns an error describing the first violation found.
pub fn validate_branch_name(name: &str) -> anyhow::Result<()> {
	validate_git_arg(name, "branch name")
}

/// Validates a git tag name.
///
/// Applies the same rules as [`validate_branch_name`]. Tags are generated from
/// package names, so this validator must accept anything a package name
/// validator accepts.
///
/// # Errors
///
/// Returns an error describing the first violation found.
pub fn validate_tag_name(name: &str) -> anyhow::Result<()> {
	validate_git_arg(name, "tag name")
}

/// Validates a git revision specifier (SHA, ref, or range).
///
/// Applies the same rules as [`validate_branch_name`]. Ranges (`..`), relative
/// specifiers (`~`, `^`), and all other revision syntax are permitted.
///
/// # Errors
///
/// Returns an error describing the first violation found.
pub fn validate_revision(rev: &str) -> anyhow::Result<()> {
	validate_git_arg(rev, "revision")
}

const MAX_BYTES: usize = 1024;

fn validate_git_arg(value: &str, kind: &str) -> anyhow::Result<()> {
	if value.is_empty() {
		bail!("{kind} must not be empty");
	}
	if value.len() > MAX_BYTES {
		bail!(
			"{kind} is too long (max {MAX_BYTES} bytes): {} bytes",
			value.len()
		);
	}
	if value.starts_with('-') {
		bail!("{kind} must not start with '-': {value:?}");
	}
	if let Some(c) = value.chars().find(|c| c.is_ascii_control()) {
		bail!("{kind} contains control character {c:?}: {value:?}");
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	// ── validate_branch_name ─────────────────────────────────────────────────

	#[test]
	fn branch_accepts_typical_names() {
		assert!(validate_branch_name("main").is_ok());
		assert!(validate_branch_name("feature/foo").is_ok());
		assert!(validate_branch_name("cursus-release/main").is_ok());
		assert!(validate_branch_name("release-1.2.3").is_ok());
	}

	#[test]
	fn branch_accepts_unicode() {
		assert!(validate_branch_name("fonctionnalité/ajout").is_ok());
	}

	#[test]
	fn branch_rejects_empty() {
		let err = validate_branch_name("").unwrap_err();
		assert!(err.to_string().contains("must not be empty"));
	}

	#[test]
	fn branch_rejects_leading_dash() {
		let err = validate_branch_name("--upload-pack=evil").unwrap_err();
		assert!(err.to_string().contains("must not start with '-'"));
	}

	#[test]
	fn branch_rejects_single_dash() {
		assert!(validate_branch_name("-").is_err());
	}

	#[test]
	fn branch_rejects_control_char() {
		assert!(validate_branch_name("feat\x07ure").is_err());
		assert!(validate_branch_name("feat\x00ure").is_err());
	}

	// ── validate_tag_name ────────────────────────────────────────────────────

	#[test]
	fn tag_accepts_typical_names() {
		assert!(validate_tag_name("v1.2.3").is_ok());
		assert!(validate_tag_name("my-crate@1.2.3").is_ok());
		assert!(validate_tag_name("@scope/pkg@1.0.0").is_ok());
		assert!(validate_tag_name("v1.0.0+build.1").is_ok());
	}

	#[test]
	fn tag_accepts_unicode() {
		assert!(validate_tag_name("données@1.0.0").is_ok());
	}

	#[test]
	fn tag_rejects_empty() {
		assert!(validate_tag_name("").is_err());
	}

	#[test]
	fn tag_rejects_leading_dash() {
		let err = validate_tag_name("--upload-pack=evil").unwrap_err();
		assert!(err.to_string().contains("must not start with '-'"));
	}

	#[test]
	fn tag_rejects_control_char() {
		assert!(validate_tag_name("v1.0\x00.0").is_err());
	}

	// ── validate_revision ────────────────────────────────────────────────────

	#[test]
	fn revision_accepts_typical_values() {
		assert!(validate_revision("HEAD").is_ok());
		assert!(validate_revision("origin/HEAD..HEAD").is_ok());
		assert!(validate_revision("HEAD~3").is_ok());
		assert!(validate_revision("abc1234def567890").is_ok());
		assert!(validate_revision("main").is_ok());
	}

	#[test]
	fn revision_rejects_empty() {
		assert!(validate_revision("").is_err());
	}

	#[test]
	fn revision_rejects_leading_dash() {
		let err = validate_revision("--exec=evil").unwrap_err();
		assert!(err.to_string().contains("must not start with '-'"));
	}

	#[test]
	fn revision_rejects_control_char() {
		assert!(validate_revision("HEAD\x00evil").is_err());
		assert!(validate_revision("HEAD\nevil").is_err());
	}

	#[test]
	fn rejects_over_1kb() {
		let long = "a".repeat(MAX_BYTES + 1);
		assert!(validate_branch_name(&long).is_err());
		assert!(validate_tag_name(&long).is_err());
		assert!(validate_revision(&long).is_err());
	}

	#[test]
	fn accepts_exactly_1kb() {
		let at_limit = "a".repeat(MAX_BYTES);
		assert!(validate_branch_name(&at_limit).is_ok());
	}
}
