use ratatui::prelude::*;

use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{InitResult, Screen, WizardState, edit_github::make_edit_github_screen};

/// Button screen state for the [`Screen::EnableGitHub`] screen.
pub(super) struct EnableGitHubButtons {
	pub(super) yes: bool,
}

impl ButtonScreen for EnableGitHubButtons {
	type State = WizardState;
	type Result = InitResult;
	type FullScreen = Screen;

	fn question(&self) -> String {
		crate::t!("enable-github-question")
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
		EnableGitHubButtons { yes: !self.yes }
	}

	fn prev(self) -> Self {
		EnableGitHubButtons { yes: !self.yes }
	}

	fn with_index(self, index: usize) -> Self {
		EnableGitHubButtons { yes: index == 0 }
	}

	fn into_continue(self, state: WizardState) -> (WizardState, Screen) {
		(state, Screen::EnableGitHub(self.yes))
	}

	fn on_confirm(
		self,
		mut state: WizardState,
	) -> anyhow::Result<KeyResult<(WizardState, Screen), InitResult>> {
		state.github_enabled = self.yes;
		if self.yes {
			let screen = make_edit_github_screen(&state);
			Ok(KeyResult::Continue((state, screen)))
		} else {
			Ok(KeyResult::Continue((state, Screen::OpenEditor(false))))
		}
	}
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};
	use super::*;

	#[test]
	fn enable_github_toggle() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (_, s) = unwrap_continue(handle_key(
			state,
			Screen::EnableGitHub(false),
			key(KeyCode::Tab),
		));
		assert!(matches!(s, Screen::EnableGitHub(true)));
	}

	#[test]
	fn enable_github_yes_advances_to_edit_github() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::EnableGitHub(true),
			key(KeyCode::Enter),
		));
		assert!(new_state.github_enabled);
		assert!(matches!(s, Screen::EditGitHub { .. }));
	}

	#[test]
	fn enable_github_no_advances_to_open_editor() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::EnableGitHub(false),
			key(KeyCode::Enter),
		));
		assert!(!new_state.github_enabled);
		assert!(matches!(s, Screen::OpenEditor(_)));
	}

	#[test]
	fn enable_github_esc_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		assert_cancelled(handle_key(
			state,
			Screen::EnableGitHub(false),
			key(KeyCode::Esc),
		));
	}

	#[test]
	fn enable_github_click_yes_button_advances_to_edit_github() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (new_state, s) = unwrap_continue(EnableGitHubButtons { yes: false }.handle_event(
			state,
			mouse_click(10, area.y + 5),
			area,
		));
		assert!(new_state.github_enabled);
		assert!(matches!(s, Screen::EditGitHub { .. }));
	}

	#[test]
	fn enable_github_click_no_button_advances_to_open_editor() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (new_state, s) = unwrap_continue(EnableGitHubButtons { yes: true }.handle_event(
			state,
			mouse_click(65, area.y + 5),
			area,
		));
		assert!(!new_state.github_enabled);
		assert!(matches!(s, Screen::OpenEditor(_)));
	}

	#[test]
	fn enable_github_click_outside_does_nothing() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (_, s) = unwrap_continue(EnableGitHubButtons { yes: false }.handle_event(
			state,
			mouse_click(10, area.y + 18),
			area,
		));
		assert!(matches!(s, Screen::EnableGitHub(false)));
	}

	#[test]
	fn ui_renders_enable_github() {
		crate::locale::set_locale("en");
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| super::super::ui(frame, &state, &Screen::EnableGitHub(false)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("GitHub Releases"));
	}
}
