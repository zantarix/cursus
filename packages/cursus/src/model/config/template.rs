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
