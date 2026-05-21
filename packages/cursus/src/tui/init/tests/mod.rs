mod choose_forge;
mod confirm_overwrite;
mod edit_github;
mod edit_gitlab;
mod enable_git;
mod git_strategy;
mod manifest_path;
mod open_editor;
mod select_pms;

use crossterm::event::KeyCode;

use crate::model::config::Strategy;
use crate::tui::init::choose_forge::ForgeChoice;
use crate::tui::init::test_helpers::*;
use crate::tui::init::*;
use crate::tui::widgets::TabStatus;

// --- tab_states ---

#[test]
fn tab_states_managers_screens_show_current_managers() {
	let [m, g, gh] = tab_states(&Screen::SelectPackageManagers {
		cargo: true,
		npm: false,
		focus: PmFocus::Cargo,
	});
	assert_eq!(m, TabStatus::Current);
	assert_eq!(g, TabStatus::Future);
	assert_eq!(gh, TabStatus::Future);
}

#[test]
fn tab_states_git_screens_show_completed_managers() {
	let [m, g, gh] = tab_states(&Screen::EnableGit(true));
	assert_eq!(m, TabStatus::Completed);
	assert_eq!(g, TabStatus::Current);
	assert_eq!(gh, TabStatus::Future);
}

#[test]
fn tab_states_github_screens_show_completed_git() {
	let [m, g, gh] = tab_states(&Screen::OpenEditor(false));
	assert_eq!(m, TabStatus::Completed);
	assert_eq!(g, TabStatus::Completed);
	assert_eq!(gh, TabStatus::Current);
}

// --- detect_package_managers ---

#[test]
fn detect_package_managers_defaults_to_neither() {
	let dir = temp_dir();
	let (cargo, npm) =
		detect_package_managers(&crate::path::AbsolutePath::new(dir.path()).unwrap());
	assert!(!cargo);
	assert!(!npm);
}

#[test]
fn detect_package_managers_detects_cargo() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
	let (cargo, npm) =
		detect_package_managers(&crate::path::AbsolutePath::new(dir.path()).unwrap());
	assert!(cargo);
	assert!(!npm);
}

#[test]
fn detect_package_managers_detects_npm() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("package.json"), "{}").unwrap();
	let (cargo, npm) =
		detect_package_managers(&crate::path::AbsolutePath::new(dir.path()).unwrap());
	assert!(!cargo);
	assert!(npm);
}

#[test]
fn detect_package_managers_detects_both() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
	std::fs::write(dir.path().join("package.json"), "{}").unwrap();
	let (cargo, npm) =
		detect_package_managers(&crate::path::AbsolutePath::new(dir.path()).unwrap());
	assert!(cargo);
	assert!(npm);
}

// --- Workflow tests ---

#[test]
fn workflow_cargo_only_git_disabled() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: true,
		npm: false,
		focus: PmFocus::Cargo,
	};

	let (state, screen) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(matches!(screen, Screen::EnableGit(_)));

	let (state, screen) = unwrap_continue(handle_key(
		state,
		Screen::EnableGit(false),
		key(KeyCode::Enter),
	));
	assert!(matches!(screen, Screen::OpenEditor(_)));

	let result = unwrap_complete(handle_key(
		state,
		Screen::OpenEditor(false),
		key(KeyCode::Enter),
	));
	assert!(result.cargo_enabled);
	assert!(!result.npm_enabled);
	assert!(!result.git_enabled);
	assert!(!result.github_enabled);
	assert!(!result.open_editor);
}

#[test]
fn workflow_branch_strategy_defaults_to_choose_forge_github() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
	let mut state = make_state(&dir);
	state.git_enabled = true;

	let (state, screen) = unwrap_continue(handle_key(
		state,
		Screen::GitStrategy(Strategy::Branch),
		key(KeyCode::Enter),
	));
	// Branch defaults the forge choice to GitHub but does not enable it yet —
	// the user can still pick GitLab or Neither at the prompt.
	assert!(!state.github_enabled);
	assert!(matches!(screen, Screen::ChooseForge(ForgeChoice::GitHub)));

	let (state, screen) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(state.github_enabled);
	assert!(matches!(screen, Screen::EditGitHub { .. }));

	let (_, screen) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(matches!(screen, Screen::OpenEditor(_)));
}

#[test]
fn workflow_push_then_choose_forge_neither_skips_to_open_editor() {
	let dir = temp_dir();
	let mut state = make_state(&dir);
	state.git_enabled = true;

	let (state, screen) = unwrap_continue(handle_key(
		state,
		Screen::GitStrategy(Strategy::Push),
		key(KeyCode::Enter),
	));
	assert!(matches!(screen, Screen::ChooseForge(ForgeChoice::Neither)));

	let (state, screen) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(!state.github_enabled);
	assert!(!state.gitlab_enabled);
	assert!(matches!(screen, Screen::OpenEditor(_)));
}

#[test]
fn workflow_push_then_choose_forge_gitlab_lands_on_edit_gitlab() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (state, screen) = unwrap_continue(handle_key(
		state,
		Screen::ChooseForge(ForgeChoice::GitLab),
		key(KeyCode::Enter),
	));
	assert!(state.gitlab_enabled);
	assert!(!state.github_enabled);
	assert!(matches!(screen, Screen::EditGitLab { .. }));
}

#[test]
fn workflow_branch_then_choose_gitlab_overrides_default() {
	let dir = temp_dir();
	let state = make_state(&dir);
	// Land on the Branch-default ChooseForge prompt, then actively pick GitLab.
	let (state, screen) = unwrap_continue(handle_key(
		state,
		Screen::GitStrategy(Strategy::Branch),
		key(KeyCode::Enter),
	));
	assert!(matches!(screen, Screen::ChooseForge(ForgeChoice::GitHub)));

	// Tab once to GitLab.
	let (state, screen) = unwrap_continue(handle_key(state, screen, key(KeyCode::Tab)));
	assert!(matches!(screen, Screen::ChooseForge(ForgeChoice::GitLab)));

	let (state, screen) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(state.gitlab_enabled);
	assert!(!state.github_enabled);
	assert!(matches!(screen, Screen::EditGitLab { .. }));
}

#[test]
fn workflow_complete_state_preserved_in_result() {
	let dir = temp_dir();
	let mut state = make_state(&dir);
	state.cargo_enabled = true;
	state.npm_enabled = true;
	state.git_enabled = true;
	state.git_strategy = Some(Strategy::Push);
	state.github_enabled = true;
	state.github_owner = Some("acme".to_string());
	state.github_repo = Some("my-app".to_string());

	let result = unwrap_complete(handle_key(
		state,
		Screen::OpenEditor(true),
		key(KeyCode::Enter),
	));
	assert!(result.cargo_enabled);
	assert!(result.npm_enabled);
	assert!(result.git_enabled);
	assert_eq!(result.git_strategy, Some(Strategy::Push));
	assert!(result.github_enabled);
	assert_eq!(result.github_owner, Some("acme".to_string()));
	assert_eq!(result.github_repo, Some("my-app".to_string()));
	assert!(result.open_editor);
}

#[test]
fn workflow_full_gitlab_state_preserved_in_result() {
	let dir = temp_dir();
	let mut state = make_state(&dir);
	state.cargo_enabled = true;
	state.git_enabled = true;
	state.git_strategy = Some(Strategy::Push);
	state.gitlab_enabled = true;
	state.gitlab_group = Some("acme".to_string());
	state.gitlab_project = Some("my-app".to_string());
	state.gitlab_host = Some("gitlab.example.com".to_string());

	let result = unwrap_complete(handle_key(
		state,
		Screen::OpenEditor(false),
		key(KeyCode::Enter),
	));
	assert!(result.gitlab_enabled);
	assert_eq!(result.gitlab_group, Some("acme".to_string()));
	assert_eq!(result.gitlab_project, Some("my-app".to_string()));
	assert_eq!(result.gitlab_host, Some("gitlab.example.com".to_string()));
}

/// Catches the `state.dry_run` guard deletion at line 209: when dry_run is set,
/// transitioning to OpenEditor should immediately Complete rather than Continue.
#[test]
fn dry_run_skips_open_editor_and_completes() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
	let mut state = make_state(&dir);
	state.dry_run = true;
	// EnableGit(false) + Enter → would normally give OpenEditor, but dry_run
	// should intercept and immediately Complete.
	let result = handle_key(state, Screen::EnableGit(false), key(KeyCode::Enter));
	let init_result = unwrap_complete(result);
	assert!(!init_result.open_editor, "dry_run must skip the editor");
}

/// Catches the `>= 72`→`> 72` or `>= 72`→`>= 73` mutation at line 242:
/// a terminal narrower than 72 columns must show the short label "Managers".
#[test]
fn narrow_terminal_uses_short_managers_label() {
	use ratatui::Terminal;
	use ratatui::backend::TestBackend;
	// Width 71 is below the 72-column threshold
	let backend = TestBackend::new(71, 24);
	let mut terminal = Terminal::new(backend).unwrap();
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: true,
		npm: false,
		focus: PmFocus::Cargo,
	};
	terminal.draw(|frame| ui(frame, &state, &screen)).unwrap();
	let content = crate::tui::test_utils::buffer_to_string(terminal.backend().buffer());
	// Only inspect the tab bar (first TAB_HEIGHT rows); the screen body can say
	// "Package Managers" independently of the tab label.
	let tab_area: String = content
		.lines()
		.take(TAB_HEIGHT as usize)
		.collect::<Vec<_>>()
		.join("\n");
	assert!(
		tab_area.contains("Managers"),
		"Short label must appear in tab on narrow terminal, got: {tab_area:?}"
	);
	assert!(
		!tab_area.contains("Package Managers"),
		"Long label must not appear in tab on narrow terminal, got: {tab_area:?}"
	);
}
