use ratatui::prelude::*;

use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{InitResult, PmFocus, Screen, WizardState, detect_package_managers};

/// Button screen state for the [`Screen::ConfirmOverwrite`] screen.
pub(crate) struct ConfirmOverwriteButtons {
	pub(crate) yes: bool,
}

impl ButtonScreen for ConfirmOverwriteButtons {
	type State = WizardState;
	type Result = InitResult;
	type FullScreen = Screen;

	fn question(&self) -> String {
		crate::t!("confirm-overwrite-question")
	}

	fn buttons(&self) -> Vec<ButtonDef> {
		vec![
			ButtonDef {
				label: crate::t!("button-yes"),
				selected: self.yes,
				color: Some(Color::Red),
			},
			ButtonDef {
				label: crate::t!("button-no"),
				selected: !self.yes,
				color: None,
			},
		]
	}

	fn next(self) -> Self {
		ConfirmOverwriteButtons { yes: !self.yes }
	}

	fn prev(self) -> Self {
		ConfirmOverwriteButtons { yes: !self.yes }
	}

	fn with_index(self, index: usize) -> Self {
		ConfirmOverwriteButtons { yes: index == 0 }
	}

	fn into_continue(self, state: WizardState) -> (WizardState, Screen) {
		(state, Screen::ConfirmOverwrite(self.yes))
	}

	fn on_confirm(
		self,
		state: WizardState,
	) -> anyhow::Result<KeyResult<(WizardState, Screen), InitResult>> {
		if self.yes {
			let (cargo, npm) = detect_package_managers(state.env.git().path());
			Ok(KeyResult::Continue((
				state,
				Screen::SelectPackageManagers {
					cargo,
					npm,
					focus: PmFocus::Cargo,
				},
			)))
		} else {
			Ok(KeyResult::Cancelled)
		}
	}
}
