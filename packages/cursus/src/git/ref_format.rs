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

pub(crate) const MAX_BYTES: usize = 1024;

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
