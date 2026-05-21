use crate::model::config::Strategy;
use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{ForgeChoice, InitResult, Screen, WizardState};

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

	/// Selects the git strategy and advances to the forge-choice prompt.
	///
	/// - `Push` → [`Screen::ChooseForge`] defaulting to [`ForgeChoice::Neither`],
	///   preserving the opt-in default established by ADR-005.
	/// - `Branch` → [`Screen::ChooseForge`] defaulting to [`ForgeChoice::GitHub`],
	///   preserving ADR-019's "Branch implies GitHub" default while still
	///   letting the user choose GitLab or Neither.
	fn on_confirm(
		self,
		mut state: WizardState,
	) -> anyhow::Result<KeyResult<(WizardState, Screen), InitResult>> {
		state.git_strategy = Some(self.strategy);
		let default = match self.strategy {
			Strategy::Push => ForgeChoice::Neither,
			Strategy::Branch => ForgeChoice::GitHub,
		};
		Ok(KeyResult::Continue((state, Screen::ChooseForge(default))))
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
			.draw(|frame| super::super::ui(frame, &state, &Screen::GitStrategy(Strategy::Push)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Push"));
		assert!(content.contains("Branch"));
	}
}
