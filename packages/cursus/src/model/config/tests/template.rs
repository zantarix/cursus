use crate::model::config::Strategy;
use crate::model::config::template::*;
use crate::tui::init::InitResult;

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
	toml::from_str::<toml::Value>(&active).expect("Active TOML lines should parse as valid TOML");
}

#[test]
fn cargo_only_active_toml_is_valid() {
	let active = strip_comments(&render(&cargo_only_result()));
	toml::from_str::<toml::Value>(&active).expect("Active TOML lines should parse as valid TOML");
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
