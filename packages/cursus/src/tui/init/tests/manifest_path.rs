use crossterm::event::KeyCode;
use ratatui_textarea::TextArea;

use crate::model::config::PackageManager;

use crate::tui::init::test_helpers::*;
use crate::tui::init::{Screen, handle_key};

#[test]
fn manifest_path_enter_stores_path_and_advances() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let mut textarea = TextArea::default();
	textarea.insert_str("backend/");
	let screen = Screen::ManifestPath {
		pm: PackageManager::Cargo,
		textarea,
	};
	let (new_state, next) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert_eq!(new_state.cargo_path, Some("backend/".to_string()));
	assert!(matches!(next, Screen::EnableGit(_)));
}

#[test]
fn manifest_path_enter_with_empty_text_stores_none() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ManifestPath {
		pm: PackageManager::Npm,
		textarea: TextArea::default(),
	};
	let (new_state, _) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert_eq!(new_state.npm_path, None);
}

#[test]
fn manifest_path_esc_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ManifestPath {
		pm: PackageManager::Cargo,
		textarea: TextArea::default(),
	};
	assert_cancelled(handle_key(state, screen, key(KeyCode::Esc)));
}

#[test]
fn manifest_path_q_key_types_character_not_cancel() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::ManifestPath {
		pm: PackageManager::Cargo,
		textarea: TextArea::default(),
	};
	let (_, next) = unwrap_continue(handle_key(state, screen, key(KeyCode::Char('q'))));
	assert!(matches!(next, Screen::ManifestPath { .. }));
}

#[test]
fn manifest_path_advances_to_second_pm_when_both_missing() {
	let dir = temp_dir();
	let mut state = make_state(&dir);
	state.remaining_manifest_pms = vec![PackageManager::Npm];
	let screen = Screen::ManifestPath {
		pm: PackageManager::Cargo,
		textarea: TextArea::default(),
	};
	let (_, next) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(matches!(
		next,
		Screen::ManifestPath {
			pm: PackageManager::Npm,
			..
		}
	));
}

#[test]
fn ui_renders_manifest_path() {
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let state = make_state(&dir);
	terminal
		.draw(|frame| {
			crate::tui::init::ui(
				frame,
				&state,
				&Screen::ManifestPath {
					pm: PackageManager::Cargo,
					textarea: TextArea::default(),
				},
			)
		})
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Cargo.toml"));
}
