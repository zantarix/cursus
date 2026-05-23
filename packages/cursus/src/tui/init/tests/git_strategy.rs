use crossterm::event::KeyCode;

use crate::model::config::Strategy;

use crate::tui::init::choose_forge::ForgeChoice;
use crate::tui::init::git_strategy::*;
use crate::tui::init::test_helpers::*;
use crate::tui::init::{Screen, handle_key};
use crate::tui::screens::ButtonScreen;

#[test]
fn git_strategy_toggle() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (_, s) = unwrap_continue(handle_key(
		state,
		Screen::GitStrategy(Strategy::Push),
		key(KeyCode::Tab),
	));
	assert!(matches!(s, Screen::GitStrategy(Strategy::Branch)));
}

#[test]
fn git_strategy_push_advances_to_choose_forge_neither() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, s) = unwrap_continue(handle_key(
		state,
		Screen::GitStrategy(Strategy::Push),
		key(KeyCode::Enter),
	));
	assert_eq!(new_state.git_strategy, Some(Strategy::Push));
	assert!(matches!(s, Screen::ChooseForge(ForgeChoice::Neither)));
	// Branch implies forge is not yet enabled — that decision is deferred to ChooseForge.
	assert!(!new_state.github_enabled);
	assert!(!new_state.gitlab_enabled);
}

#[test]
fn git_strategy_branch_advances_to_choose_forge_github_default() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, s) = unwrap_continue(handle_key(
		state,
		Screen::GitStrategy(Strategy::Branch),
		key(KeyCode::Enter),
	));
	assert_eq!(new_state.git_strategy, Some(Strategy::Branch));
	// Branch defaults the forge prompt to GitHub but does not enable it yet —
	// the user can still pick GitLab or Neither.
	assert!(matches!(s, Screen::ChooseForge(ForgeChoice::GitHub)));
	assert!(!new_state.github_enabled);
	assert!(!new_state.gitlab_enabled);
}

#[test]
fn git_strategy_esc_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	assert_cancelled(handle_key(
		state,
		Screen::GitStrategy(Strategy::Push),
		key(KeyCode::Esc),
	));
}

#[test]
fn git_strategy_click_push_button_advances_to_choose_forge() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (new_state, s) = unwrap_continue(
		GitStrategyButtons {
			strategy: Strategy::Branch,
		}
		.handle_event(state, mouse_click(10, area.y + 7), area),
	);
	assert_eq!(new_state.git_strategy, Some(Strategy::Push));
	assert!(matches!(s, Screen::ChooseForge(ForgeChoice::Neither)));
}

#[test]
fn git_strategy_click_branch_button_advances_to_choose_forge_github() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (new_state, s) = unwrap_continue(
		GitStrategyButtons {
			strategy: Strategy::Push,
		}
		.handle_event(state, mouse_click(65, area.y + 7), area),
	);
	assert_eq!(new_state.git_strategy, Some(Strategy::Branch));
	assert!(matches!(s, Screen::ChooseForge(ForgeChoice::GitHub)));
}

#[test]
fn git_strategy_click_outside_does_nothing() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let area = test_content_area();
	let (_, s) = unwrap_continue(
		GitStrategyButtons {
			strategy: Strategy::Push,
		}
		.handle_event(state, mouse_click(10, area.y + 18), area),
	);
	assert!(matches!(s, Screen::GitStrategy(Strategy::Push)));
}

#[test]
fn ui_renders_git_strategy() {
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let state = make_state(&dir);
	terminal
		.draw(|frame| crate::tui::init::ui(frame, &state, &Screen::GitStrategy(Strategy::Push)))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Push"));
	assert!(content.contains("Branch"));
}
