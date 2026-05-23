use crossterm::event::KeyCode;

use crate::tui::init::choose_forge::*;
use crate::tui::init::test_helpers::*;
use crate::tui::init::{Screen, handle_key};
use crate::tui::screens::ButtonScreen;

#[test]
fn cycle_forward_visits_each_choice() {
	let screen = ChooseForgeButtons {
		selected: ForgeChoice::GitHub,
	};
	assert_eq!(screen.next().selected, ForgeChoice::GitLab);
	assert_eq!(
		ChooseForgeButtons {
			selected: ForgeChoice::GitLab,
		}
		.next()
		.selected,
		ForgeChoice::Neither
	);
	assert_eq!(
		ChooseForgeButtons {
			selected: ForgeChoice::Neither,
		}
		.next()
		.selected,
		ForgeChoice::GitHub
	);
}

#[test]
fn cycle_backward_wraps() {
	let screen = ChooseForgeButtons {
		selected: ForgeChoice::GitHub,
	};
	assert_eq!(screen.prev().selected, ForgeChoice::Neither);
}

#[test]
fn tab_advances_to_next_choice() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (_, s) = unwrap_continue(handle_key(
		state,
		Screen::ChooseForge(ForgeChoice::GitHub),
		key(KeyCode::Tab),
	));
	assert!(matches!(s, Screen::ChooseForge(ForgeChoice::GitLab)));
}

#[test]
fn github_choice_enables_github_and_advances_to_edit_github() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, s) = unwrap_continue(handle_key(
		state,
		Screen::ChooseForge(ForgeChoice::GitHub),
		key(KeyCode::Enter),
	));
	assert!(new_state.github_enabled);
	assert!(!new_state.gitlab_enabled);
	assert!(matches!(s, Screen::EditGitHub { .. }));
}

#[test]
fn gitlab_choice_enables_gitlab_and_advances_to_edit_gitlab() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, s) = unwrap_continue(handle_key(
		state,
		Screen::ChooseForge(ForgeChoice::GitLab),
		key(KeyCode::Enter),
	));
	assert!(new_state.gitlab_enabled);
	assert!(!new_state.github_enabled);
	assert!(matches!(s, Screen::EditGitLab { .. }));
}

#[test]
fn neither_choice_advances_to_open_editor() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, s) = unwrap_continue(handle_key(
		state,
		Screen::ChooseForge(ForgeChoice::Neither),
		key(KeyCode::Enter),
	));
	assert!(!new_state.github_enabled);
	assert!(!new_state.gitlab_enabled);
	assert!(matches!(s, Screen::OpenEditor(_)));
}

#[test]
fn esc_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	assert_cancelled(handle_key(
		state,
		Screen::ChooseForge(ForgeChoice::Neither),
		key(KeyCode::Esc),
	));
}

#[test]
fn click_github_button_advances_to_edit_github() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (new_state, s) = unwrap_continue(
		ChooseForgeButtons {
			selected: ForgeChoice::Neither,
		}
		.handle_event(state, mouse_click(10, area.y + 5), area),
	);
	assert!(new_state.github_enabled);
	assert!(matches!(s, Screen::EditGitHub { .. }));
}

#[test]
fn click_outside_does_nothing() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (_, s) = unwrap_continue(
		ChooseForgeButtons {
			selected: ForgeChoice::Neither,
		}
		.handle_event(state, mouse_click(10, area.y + 18), area),
	);
	assert!(matches!(s, Screen::ChooseForge(ForgeChoice::Neither)));
}

#[test]
fn ui_renders_choose_forge() {
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let state = make_state(&dir);
	terminal
		.draw(|frame| {
			crate::tui::init::ui(frame, &state, &Screen::ChooseForge(ForgeChoice::Neither))
		})
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("forge"));
	assert!(content.contains("GitHub"));
	assert!(content.contains("GitLab"));
	assert!(content.contains("Neither"));
}
