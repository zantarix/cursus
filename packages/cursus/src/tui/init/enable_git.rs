use ratatui::prelude::*;

use crate::model::config::Strategy;
use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{InitResult, Screen, WizardState};

/// Button screen state for the [`Screen::EnableGit`] screen.
pub(crate) struct EnableGitButtons {
	pub(crate) yes: bool,
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
