//! Template-based config file generation for `cursus init`.
//!
//! Generates a `.cursus/config.toml` string from an [`InitResult`], emitting
//! active values as normal TOML and advanced/disabled options as commented-out
//! blocks with inline documentation.

use std::fmt::{self, Write as _};

use crate::model::config::Strategy;
use crate::tui::init::InitResult;

/// Returns a TOML-encoded quoted string (e.g. `"foo\"bar"`) using `toml_edit`
/// so that any special characters in `s` are correctly escaped.
fn toml_quoted(s: &str) -> toml_edit::Value {
	toml_edit::Value::from(s)
}

fn write_cargo_section(out: &mut String, enabled: bool, path: &Option<String>) -> fmt::Result {
	if enabled {
		writeln!(out, "[cargo]")?;
		writeln!(out, "enabled = true")?;
		if let Some(p) = path {
			writeln!(out, "path = {}", toml_quoted(p))?;
		} else {
			writeln!(
				out,
				"# path = \"subdir/\"              # Subdirectory for Cargo.toml (relative to git root)"
			)?;
		}
	} else {
		writeln!(out, "# [cargo]")?;
		writeln!(out, "# enabled = false")?;
		writeln!(
			out,
			"# path = \"subdir/\"              # Subdirectory for Cargo.toml (relative to git root)"
		)?;
	}
	writeln!(out)
}

fn write_npm_section(out: &mut String, enabled: bool, path: &Option<String>) -> fmt::Result {
	if enabled {
		writeln!(out, "[npm]")?;
		writeln!(out, "enabled = true")?;
		if let Some(p) = path {
			writeln!(out, "path = {}", toml_quoted(p))?;
		} else {
			writeln!(
				out,
				"# path = \"subdir/\"              # Subdirectory for package.json (relative to git root)"
			)?;
		}
		writeln!(
			out,
			"# lock_command = \"npm install\"  # Custom command to update the lock file"
		)?;
		writeln!(
			out,
			"# access = \"restricted\"         # Access level for scoped packages: \"public\" or \"restricted\" (default: \"restricted\")"
		)?;
	} else {
		writeln!(out, "# [npm]")?;
		writeln!(out, "# enabled = false")?;
		writeln!(
			out,
			"# path = \"subdir/\"              # Subdirectory for package.json (relative to git root)"
		)?;
		writeln!(
			out,
			"# lock_command = \"npm install\"  # Custom command to update the lock file"
		)?;
		writeln!(
			out,
			"# access = \"restricted\"         # Access level for scoped packages: \"public\" or \"restricted\" (default: \"restricted\")"
		)?;
	}
	writeln!(out)
}

fn write_git_section(out: &mut String, enabled: bool, strategy: Option<Strategy>) -> fmt::Result {
	let strategy_str = match strategy {
		Some(Strategy::Branch) => "branch",
		_ => "push",
	};
	if enabled {
		writeln!(out, "[git]")?;
		writeln!(out, "enabled = true")?;
		writeln!(out, "strategy = \"{strategy_str}\"")?;
	} else {
		writeln!(out, "# [git]")?;
		writeln!(out, "# enabled = false")?;
		writeln!(out, "# strategy = \"{strategy_str}\"")?;
	}
	let prefix = if enabled { "" } else { "# " };
	writeln!(
		out,
		"{prefix}# tag_format = \"auto\"                                       # Tag format: \"auto\", \"prefixed\", or \"simple\""
	)?;
	writeln!(
		out,
		"{prefix}# release_branch_prefix = \"cursus-release/\"                 # Prefix for release branches (branch strategy)"
	)?;
	writeln!(
		out,
		"{prefix}# extra_files = []                                          # Additional files to stage before committing"
	)?;
	writeln!(
		out,
		"{prefix}# prepare_commit_message = \"ci(release): version packages\"  # Commit message for the prepare step"
	)?;
	writeln!(out)
}

/// Appends a fully commented-out `[prepare]` section.
///
/// Always emitted as comments so users can discover the `dependency_bump`
/// option without consulting documentation; it is never enabled by the init wizard.
fn write_prepare_section(out: &mut String) -> fmt::Result {
	writeln!(out, "# [prepare]")?;
	writeln!(
		out,
		"# dependency_bump = \"auto\"      # Bump level for dependents: \"auto\" (default), \"match\", \"patch\", \"minor\", or \"major\""
	)?;
	writeln!(out)
}

/// Appends a fully commented-out `[linked-versions]` section.
///
/// This section is always emitted as comments so users can discover it without
/// consulting documentation; it is never enabled by the init wizard.
fn write_linked_versions_section(out: &mut String) -> fmt::Result {
	writeln!(out, "# [linked-versions]")?;
	writeln!(
		out,
		"# Keep all packages at the same version (global linking)."
	)?;
	writeln!(out, "# enabled = true")?;
	writeln!(out)?;
	writeln!(
		out,
		"# Or define groups of packages that should share a version:"
	)?;
	writeln!(out, "# [[linked-versions.groups]]")?;
	writeln!(out, "# packages = [\"@org/prefix-*\", \"@org/other\"]")?;
	writeln!(out)
}

fn write_github_advanced_comments(out: &mut String) -> fmt::Result {
	writeln!(
		out,
		"# build_command = \"\"                # Shell command to build release artifacts"
	)?;
	writeln!(
		out,
		"# pull_request_title = \"\"           # Custom PR title (default: \"Release updates\")"
	)?;
	writeln!(
		out,
		"# [github.artifacts]                # Map of display name -> file path for release assets"
	)
}

fn write_owner_comment(out: &mut String, detected: &Option<String>) -> fmt::Result {
	match detected {
		Some(v) => writeln!(out, "# owner = {}", toml_quoted(v)),
		None => writeln!(
			out,
			"# owner = \"\"                        # GitHub owner (auto-detected from remote if omitted)"
		),
	}
}

fn write_repo_comment(out: &mut String, detected: &Option<String>) -> fmt::Result {
	match detected {
		Some(v) => writeln!(out, "# repo = {}", toml_quoted(v)),
		None => writeln!(
			out,
			"# repo = \"\"                         # GitHub repo (auto-detected from remote if omitted)"
		),
	}
}

fn write_github_section(
	out: &mut String,
	enabled: bool,
	owner: &Option<String>,
	repo: &Option<String>,
	detected_owner: &Option<String>,
	detected_repo: &Option<String>,
) -> fmt::Result {
	if enabled {
		writeln!(out, "[github]")?;
		writeln!(out, "enabled = true")?;
		if let Some(o) = owner {
			writeln!(out, "owner = {}", toml_quoted(o))?;
		} else {
			write_owner_comment(out, detected_owner)?;
		}
		if let Some(r) = repo {
			writeln!(out, "repo = {}", toml_quoted(r))?;
		} else {
			write_repo_comment(out, detected_repo)?;
		}
		write_github_advanced_comments(out)?;
	} else {
		writeln!(out, "# [github]")?;
		writeln!(out, "# enabled = false")?;
		write_owner_comment(out, detected_owner)?;
		write_repo_comment(out, detected_repo)?;
		write_github_advanced_comments(out)?;
	}
	writeln!(out)
}

/// Renders a `.cursus/config.toml` string from the given [`InitResult`].
///
/// Enabled sections are emitted as active TOML. Disabled or advanced options
/// are included as commented-out blocks so users can discover and uncomment them
/// without consulting documentation.
///
/// Section ordering: `[global]`, cargo, npm (enabled first as active TOML,
/// disabled as comments), `[prepare]`, `[linked-versions]`, `[git]`, `[github]`.
///
/// # Errors
///
/// Returns an error if writing to the internal string buffer fails.
pub(crate) fn render_init_template(result: &InitResult) -> anyhow::Result<String> {
	let mut out = String::new();
	writeln!(out, "# [global]")?;
	writeln!(
		out,
		"# disable_dependency_cycle_warnings = false  # Suppress circular dependency warnings"
	)?;
	writeln!(
		out,
		"# ignore = [\"example-*\"]                     # Glob patterns for packages to exclude from enumeration"
	)?;
	writeln!(out)?;
	write_cargo_section(&mut out, result.cargo_enabled, &result.cargo_path)?;
	write_npm_section(&mut out, result.npm_enabled, &result.npm_path)?;
	write_prepare_section(&mut out)?;
	write_linked_versions_section(&mut out)?;
	write_git_section(&mut out, result.git_enabled, result.git_strategy)?;
	write_github_section(
		&mut out,
		result.github_enabled,
		&result.github_owner,
		&result.github_repo,
		&result.detected_github_owner,
		&result.detected_github_repo,
	)?;
	Ok(out)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn render(result: &InitResult) -> String {
		render_init_template(result).expect("render_init_template should not fail")
	}

	fn cargo_only_result() -> InitResult {
		InitResult {
			cargo_enabled: true,
			npm_enabled: false,
			cargo_path: None,
			npm_path: None,
			git_enabled: false,
			git_strategy: None,
			github_enabled: false,
			github_owner: None,
			github_repo: None,
			detected_github_owner: None,
			detected_github_repo: None,
			open_editor: false,
		}
	}

	fn npm_only_result() -> InitResult {
		InitResult {
			cargo_enabled: false,
			npm_enabled: true,
			cargo_path: None,
			npm_path: None,
			git_enabled: false,
			git_strategy: None,
			github_enabled: false,
			github_owner: None,
			github_repo: None,
			detected_github_owner: None,
			detected_github_repo: None,
			open_editor: false,
		}
	}

	fn both_pms_git_github_result() -> InitResult {
		InitResult {
			cargo_enabled: true,
			npm_enabled: true,
			cargo_path: None,
			npm_path: None,
			git_enabled: true,
			git_strategy: Some(Strategy::Branch),
			github_enabled: true,
			github_owner: Some("acme".to_string()),
			github_repo: Some("my-app".to_string()),
			detected_github_owner: None,
			detected_github_repo: None,
			open_editor: false,
		}
	}

	fn strip_comments(s: &str) -> String {
		s.lines()
			.filter(|l| !l.trim_start().starts_with('#'))
			.filter(|l| !l.trim().is_empty())
			.map(|l| format!("{l}\n"))
			.collect()
	}

	// --- Snapshot tests ---

	#[test]
	fn snapshot_cargo_only() {
		insta::assert_snapshot!(render(&cargo_only_result()));
	}

	#[test]
	fn snapshot_npm_only() {
		insta::assert_snapshot!(render(&npm_only_result()));
	}

	#[test]
	fn snapshot_both_pms_git_github() {
		insta::assert_snapshot!(render(&both_pms_git_github_result()));
	}

	#[test]
	fn snapshot_cargo_with_path() {
		let result = InitResult {
			cargo_path: Some("rust/".to_string()),
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	#[test]
	fn snapshot_npm_with_path() {
		let result = InitResult {
			npm_enabled: true,
			cargo_enabled: false,
			npm_path: Some("frontend/".to_string()),
			..npm_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	#[test]
	fn snapshot_git_push_strategy() {
		let result = InitResult {
			git_enabled: true,
			git_strategy: Some(Strategy::Push),
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	/// `git_strategy: None` should render identically to `Some(Strategy::Push)`.
	#[test]
	fn snapshot_git_none_strategy_defaults_to_push() {
		let result = InitResult {
			git_enabled: true,
			git_strategy: None,
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	#[test]
	fn snapshot_github_no_owner_repo_no_detection() {
		let result = InitResult {
			git_enabled: true,
			git_strategy: Some(Strategy::Push),
			github_enabled: true,
			github_owner: None,
			github_repo: None,
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	#[test]
	fn snapshot_github_detected_values_as_hints() {
		let result = InitResult {
			github_enabled: true,
			github_owner: None,
			github_repo: None,
			detected_github_owner: Some("acme".to_string()),
			detected_github_repo: Some("my-app".to_string()),
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	#[test]
	fn snapshot_github_explicit_owner_repo() {
		let result = InitResult {
			github_enabled: true,
			github_owner: Some("acme".to_string()),
			github_repo: Some("my-app".to_string()),
			detected_github_owner: Some("acme".to_string()),
			detected_github_repo: Some("my-app".to_string()),
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	// --- TOML validity tests (behavioural, not snapshot) ---

	#[test]
	fn both_pms_git_github_active_toml_is_valid() {
		let active = strip_comments(&render(&both_pms_git_github_result()));
		toml::from_str::<toml::Value>(&active)
			.expect("Active TOML lines should parse as valid TOML");
	}

	#[test]
	fn cargo_only_active_toml_is_valid() {
		let active = strip_comments(&render(&cargo_only_result()));
		toml::from_str::<toml::Value>(&active)
			.expect("Active TOML lines should parse as valid TOML");
	}

	/// Values containing `"` or `\` must be escaped so the output is valid TOML.
	#[test]
	fn special_chars_in_user_values_produce_valid_toml() {
		let result = InitResult {
			cargo_enabled: true,
			cargo_path: Some("sub/\"evil\"\\ path/".to_string()),
			npm_enabled: true,
			npm_path: Some("front\"end\\".to_string()),
			github_enabled: true,
			github_owner: Some("ac\"me".to_string()),
			github_repo: Some("my\\app".to_string()),
			..both_pms_git_github_result()
		};
		let active = strip_comments(&render(&result));
		toml::from_str::<toml::Value>(&active)
			.expect("Special characters must be escaped so the TOML is still valid");
	}
}
