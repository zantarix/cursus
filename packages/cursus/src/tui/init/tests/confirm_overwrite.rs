use crossterm::event::KeyCode;

use crate::tui::init::confirm_overwrite::*;
use crate::tui::init::test_helpers::*;
use crate::tui::init::{Screen, handle_key};
use crate::tui::screens::ButtonScreen;

#[test]
fn confirm_overwrite_toggle() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ConfirmOverwrite(false);
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Left)));
	assert!(matches!(s, Screen::ConfirmOverwrite(true)));
}

#[test]
fn confirm_overwrite_tab_toggles() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ConfirmOverwrite(true);
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Tab)));
	assert!(matches!(s, Screen::ConfirmOverwrite(false)));
}

#[test]
fn confirm_overwrite_yes_advances_to_select_pms() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ConfirmOverwrite(true);
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(matches!(s, Screen::SelectPackageManagers { .. }));
}

#[test]
fn confirm_overwrite_no_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ConfirmOverwrite(false);
	assert_cancelled(handle_key(state, screen, key(KeyCode::Enter)));
}

#[test]
fn confirm_overwrite_esc_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ConfirmOverwrite(true);
	assert_cancelled(handle_key(state, screen, key(KeyCode::Esc)));
}

#[test]
fn confirm_overwrite_q_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ConfirmOverwrite(true);
	assert_cancelled(handle_key(state, screen, key(KeyCode::Char('q'))));
}

#[test]
fn confirm_overwrite_other_keys_do_nothing() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ConfirmOverwrite(true);
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Char('x'))));
	assert!(matches!(s, Screen::ConfirmOverwrite(true)));
}

#[test]
fn confirm_overwrite_click_yes_button_advances_to_select_pms() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (_, s) = unwrap_continue(ConfirmOverwriteButtons { yes: false }.handle_event(
		state,
		mouse_click(10, area.y + 6),
		area,
	));
	assert!(matches!(s, Screen::SelectPackageManagers { .. }));
}

#[test]
fn confirm_overwrite_click_no_button_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	assert_cancelled(ConfirmOverwriteButtons { yes: true }.handle_event(
		state,
		mouse_click(65, area.y + 6),
		area,
	));
}

#[test]
fn confirm_overwrite_click_outside_buttons_does_nothing() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (_, s) = unwrap_continue(ConfirmOverwriteButtons { yes: false }.handle_event(
		state,
		mouse_click(10, area.y + 15),
		area,
	));
	assert!(matches!(s, Screen::ConfirmOverwrite(false)));
}

#[test]
fn confirm_overwrite_yes_with_no_manifests_selects_nothing() {
	let dir = temp_dir(); // empty dir — no Cargo.toml, no package.json
	let state = make_state(&dir);
	let screen = Screen::ConfirmOverwrite(true);
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	match s {
		Screen::SelectPackageManagers { cargo, npm, .. } => {
			assert!(!cargo, "no Cargo.toml → cargo should be false");
			assert!(!npm, "no package.json → npm should be false");
		}
		other => panic!("Expected SelectPackageManagers, got {other:?}"),
	}
}

#[test]
fn ui_renders_confirm_overwrite() {
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let state = make_state(&dir);
	terminal
		.draw(|frame| crate::tui::init::ui(frame, &state, &Screen::ConfirmOverwrite(false)))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Overwrite"));
	assert!(content.contains("Yes"));
	assert!(content.contains("No"));
}
