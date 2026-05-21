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
				"# path = \"subdir/\"              # {}",
				crate::t!("cargo-path-comment")
			)?;
		}
	} else {
		writeln!(out, "# [cargo]")?;
		writeln!(out, "# enabled = false")?;
		writeln!(
			out,
			"# path = \"subdir/\"              # {}",
			crate::t!("cargo-path-comment")
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
				"# path = \"subdir/\"              # {}",
				crate::t!("npm-path-comment")
			)?;
		}
		writeln!(
			out,
			"# lock_command = \"npm install\"  # {}",
			crate::t!("npm-lock-command-comment")
		)?;
		writeln!(
			out,
			"# access = \"restricted\"         # {}",
			crate::t!("npm-access-comment")
		)?;
	} else {
		writeln!(out, "# [npm]")?;
		writeln!(out, "# enabled = false")?;
		writeln!(
			out,
			"# path = \"subdir/\"              # {}",
			crate::t!("npm-path-comment")
		)?;
		writeln!(
			out,
			"# lock_command = \"npm install\"  # {}",
			crate::t!("npm-lock-command-comment")
		)?;
		writeln!(
			out,
			"# access = \"restricted\"         # {}",
			crate::t!("npm-access-comment")
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
		"{prefix}# tag_format = \"auto\"                                       # {}",
		crate::t!("git-tag-format-comment")
	)?;
	writeln!(
		out,
		"{prefix}# release_branch_prefix = \"cursus-release/\"                 # {}",
		crate::t!("git-release-branch-prefix-comment")
	)?;
	writeln!(
		out,
		"{prefix}# extra_files = []                                          # {}",
		crate::t!("git-extra-files-comment")
	)?;
	writeln!(
		out,
		"{prefix}# prepare_commit_message = \"ci(release): version packages\"  # {}",
		crate::t!("git-prepare-commit-message-comment")
	)?;
	writeln!(
		out,
		"{prefix}# publish_private_packages = []                             # {}",
		crate::t!("git-publish-private-packages-comment")
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
		"# dependency_bump = \"auto\"      # {}",
		crate::t!("prepare-dependency-bump-comment")
	)?;
	writeln!(out)
}

/// Appends a fully commented-out `[linked-versions]` section.
///
/// This section is always emitted as comments so users can discover it without
/// consulting documentation; it is never enabled by the init wizard.
fn write_linked_versions_section(out: &mut String) -> fmt::Result {
	writeln!(out, "# [linked-versions]")?;
	writeln!(out, "# {}", crate::t!("linked-versions-global-comment"))?;
	writeln!(out, "# enabled = true")?;
	writeln!(out)?;
	writeln!(out, "# {}", crate::t!("linked-versions-groups-comment"))?;
	writeln!(out, "# [[linked-versions.groups]]")?;
	writeln!(out, "# packages = [\"@org/prefix-*\", \"@org/other\"]")?;
	writeln!(out)
}

fn write_github_advanced_comments(out: &mut String) -> fmt::Result {
	writeln!(
		out,
		"# build_command = \"\"                # {}",
		crate::t!("github-build-command-comment")
	)?;
	writeln!(
		out,
		"# pull_request_title = \"\"           # {}",
		crate::t!("github-pr-title-comment")
	)?;
	writeln!(
		out,
		"# [github.artifacts.<package-name>] # {}",
		crate::t!("github-artifacts-comment")
	)
}

fn write_owner_comment(out: &mut String, detected: &Option<String>) -> fmt::Result {
	match detected {
		Some(v) => writeln!(out, "# owner = {}", toml_quoted(v)),
		None => writeln!(
			out,
			"# owner = \"\"                        # {}",
			crate::t!("github-owner-auto-detect-comment")
		),
	}
}

fn write_repo_comment(out: &mut String, detected: &Option<String>) -> fmt::Result {
	match detected {
		Some(v) => writeln!(out, "# repo = {}", toml_quoted(v)),
		None => writeln!(
			out,
			"# repo = \"\"                         # {}",
			crate::t!("github-repo-auto-detect-comment")
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

fn write_gitlab_advanced_comments(out: &mut String) -> fmt::Result {
	writeln!(
		out,
		"# build_command = \"\"                # {}",
		crate::t!("gitlab-build-command-comment")
	)?;
	writeln!(
		out,
		"# merge_request_title = \"\"          # {}",
		crate::t!("gitlab-mr-title-comment")
	)?;
	writeln!(
		out,
		"# [gitlab.artifacts.<package-name>] # {}",
		crate::t!("gitlab-artifacts-comment")
	)
}

fn write_group_comment(out: &mut String, detected: &Option<String>) -> fmt::Result {
	match detected {
		Some(v) => writeln!(out, "# group = {}", toml_quoted(v)),
		None => writeln!(
			out,
			"# group = \"\"                        # {}",
			crate::t!("gitlab-group-auto-detect-comment")
		),
	}
}

fn write_project_comment(out: &mut String, detected: &Option<String>) -> fmt::Result {
	match detected {
		Some(v) => writeln!(out, "# project = {}", toml_quoted(v)),
		None => writeln!(
			out,
			"# project = \"\"                      # {}",
			crate::t!("gitlab-project-auto-detect-comment")
		),
	}
}

fn write_host_comment(out: &mut String) -> fmt::Result {
	// The host hint is always the empty placeholder: a non-gitlab.com detected
	// host is only meaningful when the user explicitly chose "self-managed",
	// in which case the value is emitted actively (not as a commented hint).
	writeln!(
		out,
		"# host = \"\"                         # {}",
		crate::t!("gitlab-host-comment")
	)
}

fn write_gitlab_section(out: &mut String, result: &InitResult) -> fmt::Result {
	if result.gitlab_enabled {
		writeln!(out, "[gitlab]")?;
		writeln!(out, "enabled = true")?;
		if let Some(g) = &result.gitlab_group {
			writeln!(out, "group = {}", toml_quoted(g))?;
		} else {
			write_group_comment(out, &result.detected_gitlab_group)?;
		}
		if let Some(p) = &result.gitlab_project {
			writeln!(out, "project = {}", toml_quoted(p))?;
		} else {
			write_project_comment(out, &result.detected_gitlab_project)?;
		}
		if let Some(h) = &result.gitlab_host {
			writeln!(out, "host = {}", toml_quoted(h))?;
		} else {
			write_host_comment(out)?;
		}
	} else {
		writeln!(out, "# [gitlab]")?;
		writeln!(out, "# enabled = false")?;
		write_group_comment(out, &result.detected_gitlab_group)?;
		write_project_comment(out, &result.detected_gitlab_project)?;
		write_host_comment(out)?;
	}
	write_gitlab_advanced_comments(out)?;
	writeln!(out)
}

/// Always-commented `[global]` header block.
///
/// Extracted so the section dispatcher in [`render_init_template`] can treat it
/// uniformly alongside the other sections.
fn write_global_section(out: &mut String) -> fmt::Result {
	writeln!(out, "# [global]")?;
	writeln!(
		out,
		"# disable_dependency_cycle_warnings = false  # {}",
		crate::t!("global-disable-dep-cycle-comment")
	)?;
	writeln!(
		out,
		"# ignore = [\"example-*\"]                     # {}",
		crate::t!("global-ignore-comment")
	)?;
	writeln!(out)
}

/// Identifier for each writable section, used to order output deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
	Global,
	Cargo,
	Npm,
	Prepare,
	LinkedVersions,
	Git,
	Github,
	Gitlab,
}

/// Canonical section order (also the relative order preserved within both the
/// active and commented groups when sections are reordered for output).
const SECTION_ORDER: [Section; 8] = [
	Section::Global,
	Section::Cargo,
	Section::Npm,
	Section::Prepare,
	Section::LinkedVersions,
	Section::Git,
	Section::Github,
	Section::Gitlab,
];

impl Section {
	/// Returns true when the user has opted this section in, so it should be
	/// emitted as active TOML rather than a commented-out template.
	///
	/// Sections that are always commented-out (`Global`, `Prepare`,
	/// `LinkedVersions`) return `false` here.
	fn is_active(self, result: &InitResult) -> bool {
		match self {
			Self::Global | Self::Prepare | Self::LinkedVersions => false,
			Self::Cargo => result.cargo_enabled,
			Self::Npm => result.npm_enabled,
			Self::Git => result.git_enabled,
			Self::Github => result.github_enabled,
			Self::Gitlab => result.gitlab_enabled,
		}
	}

	fn write(self, out: &mut String, result: &InitResult) -> fmt::Result {
		match self {
			Self::Global => write_global_section(out),
			Self::Cargo => write_cargo_section(out, result.cargo_enabled, &result.cargo_path),
			Self::Npm => write_npm_section(out, result.npm_enabled, &result.npm_path),
			Self::Prepare => write_prepare_section(out),
			Self::LinkedVersions => write_linked_versions_section(out),
			Self::Git => write_git_section(out, result.git_enabled, result.git_strategy),
			Self::Github => write_github_section(
				out,
				result.github_enabled,
				&result.github_owner,
				&result.github_repo,
				&result.detected_github_owner,
				&result.detected_github_repo,
			),
			Self::Gitlab => write_gitlab_section(out, result),
		}
	}
}

/// Renders a `.cursus/config.toml` string from the given [`InitResult`].
///
/// Sections the user has opted into (Cargo, npm, git, GitHub, GitLab) are
/// emitted first as active TOML, in the canonical section order. Disabled and
/// always-commented sections (`[global]`, `[prepare]`, `[linked-versions]`,
/// and any forge the user did not choose) follow as commented-out templates,
/// also in the canonical relative order.
///
/// Lifting the active sections to the top keeps the most-relevant configuration
/// visible without scrolling as the file grows; the canonical order within each
/// group keeps the output deterministic.
///
/// # Errors
///
/// Returns an error if writing to the internal string buffer fails.
pub(crate) fn render_init_template(result: &InitResult) -> anyhow::Result<String> {
	let mut out = String::new();
	let (active, commented): (Vec<Section>, Vec<Section>) = SECTION_ORDER
		.into_iter()
		.partition(|section| section.is_active(result));
	for section in active.into_iter().chain(commented) {
		section.write(&mut out, result)?;
	}
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
			gitlab_enabled: false,
			gitlab_group: None,
			gitlab_project: None,
			gitlab_host: None,
			detected_gitlab_group: None,
			detected_gitlab_project: None,
			detected_gitlab_host: None,
			open_editor: false,
		}
	}

	fn npm_only_result() -> InitResult {
		InitResult {
			cargo_enabled: false,
			npm_enabled: true,
			..cargo_only_result()
		}
	}

	fn both_pms_git_github_result() -> InitResult {
		InitResult {
			cargo_enabled: true,
			npm_enabled: true,
			git_enabled: true,
			git_strategy: Some(Strategy::Branch),
			github_enabled: true,
			github_owner: Some("acme".to_string()),
			github_repo: Some("my-app".to_string()),
			..cargo_only_result()
		}
	}

	fn gitlab_explicit_result() -> InitResult {
		InitResult {
			git_enabled: true,
			git_strategy: Some(Strategy::Push),
			gitlab_enabled: true,
			gitlab_group: Some("acme".to_string()),
			gitlab_project: Some("my-app".to_string()),
			..cargo_only_result()
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

	#[test]
	fn snapshot_gitlab_explicit_group_project() {
		insta::assert_snapshot!(render(&gitlab_explicit_result()));
	}

	/// When the user picked GitLab + unchecked self-managed (so `gitlab_host = None`),
	/// the host hint must be the empty placeholder even if a self-managed host was
	/// previously auto-detected — the user's explicit choice overrides the detection.
	#[test]
	fn gitlab_host_hint_ignores_detected_when_user_unchecked_self_managed() {
		let result = InitResult {
			gitlab_enabled: true,
			gitlab_group: Some("acme".to_string()),
			gitlab_project: Some("app".to_string()),
			gitlab_host: None,
			detected_gitlab_host: Some("gitlab.example.com".to_string()),
			..cargo_only_result()
		};
		let rendered = render(&result);
		assert!(
			!rendered.contains("gitlab.example.com"),
			"detected self-managed host must not leak into the hint when the user \
			 left gitlab_host = None:\n{rendered}"
		);
		assert!(
			rendered.contains("# host = \"\""),
			"empty placeholder hint must be emitted when gitlab_host is None"
		);
	}

	#[test]
	fn snapshot_gitlab_detected_values_as_hints() {
		let result = InitResult {
			gitlab_enabled: true,
			gitlab_group: None,
			gitlab_project: None,
			detected_gitlab_group: Some("acme".to_string()),
			detected_gitlab_project: Some("my-app".to_string()),
			detected_gitlab_host: Some("gitlab.com".to_string()),
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	#[test]
	fn snapshot_gitlab_self_managed_host() {
		let result = InitResult {
			gitlab_enabled: true,
			gitlab_group: Some("acme".to_string()),
			gitlab_project: Some("my-app".to_string()),
			gitlab_host: Some("gitlab.example.com".to_string()),
			detected_gitlab_host: Some("gitlab.example.com".to_string()),
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	// --- Reordering tests ---

	#[test]
	fn snapshot_active_sections_lifted_to_top_cargo_git_github() {
		let result = InitResult {
			git_enabled: true,
			git_strategy: Some(Strategy::Push),
			github_enabled: true,
			github_owner: Some("acme".to_string()),
			github_repo: Some("my-app".to_string()),
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	#[test]
	fn snapshot_nothing_active_preserves_canonical_order() {
		let result = InitResult {
			cargo_enabled: false,
			..cargo_only_result()
		};
		insta::assert_snapshot!(render(&result));
	}

	/// Active sections must precede every commented-out section header.
	#[test]
	fn active_sections_appear_before_any_commented_section() {
		let rendered = render(&InitResult {
			git_enabled: true,
			git_strategy: Some(Strategy::Push),
			gitlab_enabled: true,
			gitlab_group: Some("acme".to_string()),
			gitlab_project: Some("app".to_string()),
			..cargo_only_result()
		});
		let lines: Vec<&str> = rendered.lines().collect();
		let last_active = lines
			.iter()
			.rposition(|l| l.starts_with('['))
			.expect("expected at least one active section header");
		let first_commented = lines
			.iter()
			.position(|l| l.starts_with("# ["))
			.expect("expected at least one commented section header");
		assert!(
			last_active < first_commented,
			"all active sections must precede all commented sections;\n\
			 last active line index: {last_active}, first commented line index: {first_commented}\n\
			 rendered:\n{rendered}"
		);
	}

	/// Within the active group, sections appear in the canonical order
	/// (`cargo`, `npm`, `git`, `github`, `gitlab`).
	#[test]
	fn relative_order_within_active_group_is_canonical() {
		let rendered = render(&InitResult {
			cargo_enabled: true,
			npm_enabled: true,
			git_enabled: true,
			git_strategy: Some(Strategy::Push),
			..cargo_only_result()
		});
		let cargo_pos = rendered.find("[cargo]").expect("[cargo] must be present");
		let npm_pos = rendered.find("[npm]").expect("[npm] must be present");
		let git_pos = rendered.find("[git]").expect("[git] must be present");
		assert!(cargo_pos < npm_pos, "[cargo] must precede [npm]");
		assert!(npm_pos < git_pos, "[npm] must precede [git]");
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
