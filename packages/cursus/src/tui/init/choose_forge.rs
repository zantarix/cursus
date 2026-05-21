use ratatui::prelude::*;

use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::edit_github::make_edit_github_screen;
use super::edit_gitlab::make_edit_gitlab_screen;
use super::{InitResult, Screen, WizardState};

/// User's forge choice on the [`Screen::ChooseForge`] screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::init) enum ForgeChoice {
	GitHub,
	GitLab,
	Neither,
}

impl ForgeChoice {
	const ALL: [Self; 3] = [Self::GitHub, Self::GitLab, Self::Neither];

	fn index(self) -> usize {
		match self {
			Self::GitHub => 0,
			Self::GitLab => 1,
			Self::Neither => 2,
		}
	}

	fn from_index(index: usize) -> Self {
		Self::ALL.get(index).copied().unwrap_or(Self::Neither)
	}
}

/// Button screen state for the [`Screen::ChooseForge`] screen.
pub(super) struct ChooseForgeButtons {
	pub(super) selected: ForgeChoice,
}

impl ButtonScreen for ChooseForgeButtons {
	type State = WizardState;
	type Result = InitResult;
	type FullScreen = Screen;

	fn question(&self) -> String {
		crate::t!("choose-forge-question")
	}

	fn buttons(&self) -> Vec<ButtonDef> {
		ForgeChoice::ALL
			.iter()
			.map(|choice| {
				let (label, color) = match choice {
					ForgeChoice::GitHub => (crate::t!("button-github"), None),
					ForgeChoice::GitLab => (crate::t!("button-gitlab"), None),
					ForgeChoice::Neither => (crate::t!("button-neither"), Some(Color::Red)),
				};
				ButtonDef {
					label,
					selected: *choice == self.selected,
					color,
				}
			})
			.collect()
	}

	fn next(self) -> Self {
		let next_index = (self.selected.index() + 1) % ForgeChoice::ALL.len();
		ChooseForgeButtons {
			selected: ForgeChoice::from_index(next_index),
		}
	}

	fn prev(self) -> Self {
		let len = ForgeChoice::ALL.len();
		let prev_index = (self.selected.index() + len - 1) % len;
		ChooseForgeButtons {
			selected: ForgeChoice::from_index(prev_index),
		}
	}

	fn with_index(self, index: usize) -> Self {
		ChooseForgeButtons {
			selected: ForgeChoice::from_index(index),
		}
	}

	fn into_continue(self, state: WizardState) -> (WizardState, Screen) {
		(state, Screen::ChooseForge(self.selected))
	}

	fn on_confirm(
		self,
		mut state: WizardState,
	) -> anyhow::Result<KeyResult<(WizardState, Screen), InitResult>> {
		match self.selected {
			ForgeChoice::GitHub => {
				state.github_enabled = true;
				let screen = make_edit_github_screen(&state);
				Ok(KeyResult::Continue((state, screen)))
			}
			ForgeChoice::GitLab => {
				state.gitlab_enabled = true;
				let screen = make_edit_gitlab_screen(&state);
				Ok(KeyResult::Continue((state, screen)))
			}
			ForgeChoice::Neither => Ok(KeyResult::Continue((state, Screen::OpenEditor(false)))),
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
				super::super::ui(frame, &state, &Screen::ChooseForge(ForgeChoice::Neither))
			})
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("forge"));
		assert!(content.contains("GitHub"));
		assert!(content.contains("GitLab"));
		assert!(content.contains("Neither"));
	}
}
