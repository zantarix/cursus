use crossterm::event::KeyCode;

use crate::model::config::Strategy;

use crate::tui::init::enable_git::*;
use crate::tui::init::test_helpers::*;
use crate::tui::init::{Screen, handle_key};
use crate::tui::screens::ButtonScreen;

#[test]
fn enable_git_toggle() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (_, s) = unwrap_continue(handle_key(
		state,
		Screen::EnableGit(true),
		key(KeyCode::Tab),
	));
	assert!(matches!(s, Screen::EnableGit(false)));
}

#[test]
fn enable_git_yes_advances_to_git_strategy() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, s) = unwrap_continue(handle_key(
		state,
		Screen::EnableGit(true),
		key(KeyCode::Enter),
	));
	assert!(new_state.git_enabled);
	assert!(matches!(s, Screen::GitStrategy(Strategy::Push)));
}

#[test]
fn enable_git_no_advances_to_open_editor() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, s) = unwrap_continue(handle_key(
		state,
		Screen::EnableGit(false),
		key(KeyCode::Enter),
	));
	assert!(!new_state.git_enabled);
	assert!(matches!(s, Screen::OpenEditor(_)));
}

#[test]
fn enable_git_esc_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	assert_cancelled(handle_key(
		state,
		Screen::EnableGit(true),
		key(KeyCode::Esc),
	));
}

#[test]
fn enable_git_click_yes_button_advances_to_git_strategy() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (new_state, s) = unwrap_continue(EnableGitButtons { yes: false }.handle_event(
		state,
		mouse_click(10, area.y + 6),
		area,
	));
	assert!(new_state.git_enabled);
	assert!(matches!(s, Screen::GitStrategy(_)));
}

#[test]
fn enable_git_click_no_button_advances_to_open_editor() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (new_state, s) = unwrap_continue(EnableGitButtons { yes: true }.handle_event(
		state,
		mouse_click(65, area.y + 6),
		area,
	));
	assert!(!new_state.git_enabled);
	assert!(matches!(s, Screen::OpenEditor(_)));
}

#[test]
fn enable_git_click_outside_does_nothing() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (_, s) = unwrap_continue(EnableGitButtons { yes: true }.handle_event(
		state,
		mouse_click(10, area.y + 18),
		area,
	));
	assert!(matches!(s, Screen::EnableGit(true)));
}

#[test]
fn enable_git_with_index_zero_selects_yes() {
	// Direct unit-level coverage of the `with_index` ButtonScreen impl;
	// the handle_key paths exercise `next`/`prev` but not `with_index`.
	let buttons = EnableGitButtons { yes: false }.with_index(0);
	assert!(buttons.yes);
	let buttons = EnableGitButtons { yes: true }.with_index(1);
	assert!(!buttons.yes);
}

#[test]
fn ui_renders_enable_git() {
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let state = make_state(&dir);
	terminal
		.draw(|frame| crate::tui::init::ui(frame, &state, &Screen::EnableGit(true)))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Yes"));
	assert!(content.contains("No"));
}
