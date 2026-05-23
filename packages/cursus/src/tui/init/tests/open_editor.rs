use crossterm::event::KeyCode;

use crate::tui::init::open_editor::*;
use crate::tui::init::test_helpers::*;
use crate::tui::init::{Screen, handle_key};
use crate::tui::screens::ButtonScreen;

#[test]
fn open_editor_toggle() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (_, s) = unwrap_continue(handle_key(
		state,
		Screen::OpenEditor(false),
		key(KeyCode::Tab),
	));
	assert!(matches!(s, Screen::OpenEditor(true)));
}

#[test]
fn open_editor_yes_completes_with_open_editor_true() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let result = unwrap_complete(handle_key(
		state,
		Screen::OpenEditor(true),
		key(KeyCode::Enter),
	));
	assert!(result.open_editor);
}

#[test]
fn open_editor_no_completes_with_open_editor_false() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let result = unwrap_complete(handle_key(
		state,
		Screen::OpenEditor(false),
		key(KeyCode::Enter),
	));
	assert!(!result.open_editor);
}

#[test]
fn open_editor_esc_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	assert_cancelled(handle_key(
		state,
		Screen::OpenEditor(false),
		key(KeyCode::Esc),
	));
}

#[test]
fn open_editor_click_yes_button_completes_with_open_editor_true() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let result = unwrap_complete(OpenEditorButtons { yes: false }.handle_event(
		state,
		mouse_click(10, area.y + 5),
		area,
	));
	assert!(result.open_editor);
}

#[test]
fn open_editor_click_no_button_completes_with_open_editor_false() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let result = unwrap_complete(OpenEditorButtons { yes: true }.handle_event(
		state,
		mouse_click(65, area.y + 5),
		area,
	));
	assert!(!result.open_editor);
}

#[test]
fn open_editor_click_outside_does_nothing() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (_, s) = unwrap_continue(OpenEditorButtons { yes: true }.handle_event(
		state,
		mouse_click(10, area.y + 18),
		area,
	));
	assert!(matches!(s, Screen::OpenEditor(true)));
}

#[test]
fn ui_renders_open_editor() {
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let state = make_state(&dir);
	terminal
		.draw(|frame| crate::tui::init::ui(frame, &state, &Screen::OpenEditor(false)))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Yes"));
	assert!(content.contains("No"));
}
