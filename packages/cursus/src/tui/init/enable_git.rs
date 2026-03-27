use ratatui::prelude::*;

use crate::model::config::Strategy;
use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{InitResult, Screen, WizardState};

/// Button screen state for the [`Screen::EnableGit`] screen.
pub(super) struct EnableGitButtons {
	pub(super) yes: bool,
}

impl ButtonScreen for EnableGitButtons {
	type State = WizardState;
	type Result = InitResult;
	type FullScreen = Screen;

	fn question(&self) -> String {
		crate::t!("enable-git-question")
	}

	fn buttons(&self) -> Vec<ButtonDef> {
		vec![
			ButtonDef {
				label: crate::t!("button-yes"),
				selected: self.yes,
				color: None,
			},
			ButtonDef {
				label: crate::t!("button-no"),
				selected: !self.yes,
				color: Some(Color::Red),
			},
		]
	}

	fn next(self) -> Self {
		EnableGitButtons { yes: !self.yes }
	}

	fn prev(self) -> Self {
		EnableGitButtons { yes: !self.yes }
	}

	fn with_index(self, index: usize) -> Self {
		EnableGitButtons { yes: index == 0 }
	}

	fn into_continue(self, state: WizardState) -> (WizardState, Screen) {
		(state, Screen::EnableGit(self.yes))
	}

	fn on_confirm(
		self,
		mut state: WizardState,
	) -> anyhow::Result<KeyResult<(WizardState, Screen), InitResult>> {
		state.git_enabled = self.yes;
		if self.yes {
			Ok(KeyResult::Continue((
				state,
				Screen::GitStrategy(Strategy::Push),
			)))
		} else {
			Ok(KeyResult::Continue((state, Screen::OpenEditor(false))))
		}
	}
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use crate::model::config::Strategy;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};
	use super::*;

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
	fn ui_renders_enable_git() {
		crate::locale::set_locale("en");
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| super::super::ui(frame, &state, &Screen::EnableGit(true)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Yes"));
		assert!(content.contains("No"));
	}
}
