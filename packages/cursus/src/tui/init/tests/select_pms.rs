use crossterm::event::KeyCode;

use crate::model::config::PackageManager;

use crate::tui::init::select_pms::*;
use crate::tui::init::test_helpers::*;
use crate::tui::init::{PmFocus, Screen, handle_key};

#[test]
fn select_pms_tab_moves_focus() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: true,
		npm: false,
		focus: PmFocus::Cargo,
	};
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Tab)));
	assert!(matches!(
		s,
		Screen::SelectPackageManagers {
			focus: PmFocus::Npm,
			..
		}
	));
}

#[test]
fn select_pms_space_toggles_focused_item() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: true,
		npm: false,
		focus: PmFocus::Cargo,
	};
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Char(' '))));
	assert!(matches!(
		s,
		Screen::SelectPackageManagers { cargo: false, .. }
	));
}

#[test]
fn select_pms_space_toggles_npm_when_focused() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: false,
		npm: false,
		focus: PmFocus::Npm,
	};
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Char(' '))));
	assert!(matches!(s, Screen::SelectPackageManagers { npm: true, .. }));
}

#[test]
fn select_pms_enter_with_none_selected_does_not_advance() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: false,
		npm: false,
		focus: PmFocus::Cargo,
	};
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(matches!(s, Screen::SelectPackageManagers { .. }));
}

#[test]
fn select_pms_enter_with_cargo_advances_to_enable_git_when_manifest_exists() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: true,
		npm: false,
		focus: PmFocus::Cargo,
	};
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(matches!(s, Screen::EnableGit(_)));
}

#[test]
fn select_pms_enter_with_cargo_shows_manifest_path_when_missing() {
	let dir = temp_dir(); // No Cargo.toml
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: true,
		npm: false,
		focus: PmFocus::Cargo,
	};
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert!(matches!(
		s,
		Screen::ManifestPath {
			pm: PackageManager::Cargo,
			..
		}
	));
}

#[test]
fn select_pms_esc_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: true,
		npm: false,
		focus: PmFocus::Cargo,
	};
	assert_cancelled(handle_key(state, screen, key(KeyCode::Esc)));
}

#[test]
fn select_pms_click_cargo_row_toggles_cargo() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	// wizard_layout margin=2: inner starts at y=area.y+2=5
	// question block: y=5, height=3
	// checkbox block: y=8, fills remaining → inner y_start=9
	// Cargo row (relative 0): absolute row = 9
	let (_, s) = unwrap_continue(handle_select_pms(
		state,
		false,
		false,
		PmFocus::Npm,
		mouse_click(10, area.y + 6),
		area,
	));
	assert!(matches!(
		s,
		Screen::SelectPackageManagers {
			cargo: true,
			focus: PmFocus::Cargo,
			..
		}
	));
}

#[test]
fn select_pms_click_npm_row_toggles_npm() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	// NPM row (relative 1): absolute row = 10 = area.y + 7
	let (_, s) = unwrap_continue(handle_select_pms(
		state,
		false,
		false,
		PmFocus::Cargo,
		mouse_click(10, area.y + 7),
		area,
	));
	assert!(matches!(
		s,
		Screen::SelectPackageManagers {
			npm: true,
			focus: PmFocus::Npm,
			..
		}
	));
}

#[test]
fn select_pms_click_outside_checkboxes_does_nothing() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (_, s) = unwrap_continue(handle_select_pms(
		state,
		false,
		true,
		PmFocus::Cargo,
		mouse_click(10, area.y + 20),
		area,
	));
	assert!(matches!(
		s,
		Screen::SelectPackageManagers {
			cargo: false,
			npm: true,
			focus: PmFocus::Cargo,
			..
		}
	));
}

#[test]
fn select_pms_click_empty_row_in_checkbox_block_does_nothing() {
	// The checkbox block fills remaining space with borders → many inner rows.
	// Only rows 0 (Cargo) and 1 (NPM) have content; the rest are blank.
	// Clicking a blank row must not toggle anything.
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	// Blank row 2 inside the checkbox inner area (area.y+2+3+1+2 = area.y+8)
	let (_, s) = unwrap_continue(handle_select_pms(
		state,
		false,
		true,
		PmFocus::Cargo,
		mouse_click(10, area.y + 8),
		area,
	));
	assert!(matches!(
		s,
		Screen::SelectPackageManagers {
			cargo: false,
			npm: true,
			focus: PmFocus::Cargo,
			..
		}
	));
}

/// Catches the `&&`→`||` mutation at line 31:
/// `if npm && !package_json.exists()` — when package.json already exists,
/// npm must NOT be queued for manifest prompting.
/// With `||`, `npm || !exists()` = `true || false = true` → queued (wrong).
#[test]
fn select_pms_npm_with_existing_package_json_skips_manifest_prompt() {
	let dir = temp_dir();
	std::fs::write(dir.path().join("package.json"), "{}").unwrap();
	let state = make_state(&dir);
	let screen = Screen::SelectPackageManagers {
		cargo: false,
		npm: true,
		focus: PmFocus::Npm,
	};
	let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	// Should advance to EnableGit, not ManifestPath
	assert!(
		matches!(s, Screen::EnableGit(_)),
		"package.json exists → no manifest prompt expected"
	);
}

#[test]
fn ui_renders_select_pms() {
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
				&Screen::SelectPackageManagers {
					cargo: true,
					npm: false,
					focus: PmFocus::Cargo,
				},
			)
		})
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Package Managers"));
	assert!(content.contains("Cargo"));
	assert!(content.contains("NPM"));
}
