use ratatui::prelude::*;

use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::edit_github::make_edit_github_screen;
use super::edit_gitlab::make_edit_gitlab_screen;
use super::{InitResult, Screen, WizardState};

/// User's forge choice on the [`Screen::ChooseForge`] screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForgeChoice {
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
pub(crate) struct ChooseForgeButtons {
	pub(crate) selected: ForgeChoice,
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
