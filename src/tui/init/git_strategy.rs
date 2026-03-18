use crate::model::config::Strategy;
use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{InitResult, Screen, WizardState, edit_github::make_edit_github_screen};

/// Button screen state for the [`Screen::GitStrategy`] screen.
pub(super) struct GitStrategyButtons {
	pub(super) strategy: Strategy,
}

impl ButtonScreen for GitStrategyButtons {
	type State = WizardState;
	type Result = InitResult;
	type FullScreen = Screen;

	fn question(&self) -> String {
		crate::t!("git-strategy-question")
	}

	fn buttons(&self) -> Vec<ButtonDef> {
		vec![
			ButtonDef {
				label: crate::t!("button-push"),
				selected: self.strategy == Strategy::Push,
				color: None,
			},
			ButtonDef {
				label: crate::t!("button-branch"),
				selected: self.strategy == Strategy::Branch,
				color: None,
			},
		]
	}

	fn next(self) -> Self {
		let strategy = match self.strategy {
			Strategy::Push => Strategy::Branch,
			Strategy::Branch => Strategy::Push,
		};
		GitStrategyButtons { strategy }
	}

	fn prev(self) -> Self {
		self.next()
	}

	fn with_index(self, index: usize) -> Self {
		let strategy = if index == 0 {
			Strategy::Push
		} else {
			Strategy::Branch
		};
		GitStrategyButtons { strategy }
	}

	fn into_continue(self, state: WizardState) -> (WizardState, Screen) {
		(state, Screen::GitStrategy(self.strategy))
	}

	/// Selects the git strategy and advances:
	/// - `Push` → [`Screen::EnableGitHub`]
	/// - `Branch` → auto-enables GitHub and skips to [`Screen::EditGitHub`]
	fn on_confirm(
		self,
		mut state: WizardState,
	) -> anyhow::Result<KeyResult<(WizardState, Screen), InitResult>> {
		state.git_strategy = Some(self.strategy);
		match self.strategy {
			Strategy::Push => Ok(KeyResult::Continue((state, Screen::EnableGitHub(false)))),
			Strategy::Branch => {
				// Branch implies GitHub enabled
				state.github_enabled = true;
				let screen = make_edit_github_screen(&state);
				Ok(KeyResult::Continue((state, screen)))
			}
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
	fn git_strategy_push_advances_to_enable_github() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::GitStrategy(Strategy::Push),
			key(KeyCode::Enter),
		));
		assert_eq!(new_state.git_strategy, Some(Strategy::Push));
		assert!(matches!(s, Screen::EnableGitHub(_)));
	}

	#[test]
	fn git_strategy_branch_skips_enable_github_and_enables_it() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::GitStrategy(Strategy::Branch),
			key(KeyCode::Enter),
		));
		assert_eq!(new_state.git_strategy, Some(Strategy::Branch));
		assert!(new_state.github_enabled);
		assert!(matches!(s, Screen::EditGitHub { .. }));
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
	fn git_strategy_click_push_button_advances_to_enable_github() {
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
		assert!(matches!(s, Screen::EnableGitHub(_)));
	}

	#[test]
	fn git_strategy_click_branch_button_enables_github_and_goes_to_edit_github() {
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
		assert!(new_state.github_enabled);
		assert!(matches!(s, Screen::EditGitHub { .. }));
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
			.draw(|frame| super::super::ui(frame, &state, &Screen::GitStrategy(Strategy::Push)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Push"));
		assert!(content.contains("Branch"));
	}
}
