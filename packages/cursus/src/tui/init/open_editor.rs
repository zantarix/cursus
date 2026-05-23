use ratatui::prelude::*;

use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{InitResult, Screen, WizardState, complete};

/// Button screen state for the [`Screen::OpenEditor`] screen.
pub(crate) struct OpenEditorButtons {
	pub(crate) yes: bool,
}

impl ButtonScreen for OpenEditorButtons {
	type State = WizardState;
	type Result = InitResult;
	type FullScreen = Screen;

	fn question(&self) -> String {
		crate::t!("open-editor-question")
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
		OpenEditorButtons { yes: !self.yes }
	}

	fn prev(self) -> Self {
		OpenEditorButtons { yes: !self.yes }
	}

	fn with_index(self, index: usize) -> Self {
		OpenEditorButtons { yes: index == 0 }
	}

	fn into_continue(self, state: WizardState) -> (WizardState, Screen) {
		(state, Screen::OpenEditor(self.yes))
	}

	fn on_confirm(
		self,
		state: WizardState,
	) -> anyhow::Result<KeyResult<(WizardState, Screen), InitResult>> {
		Ok(KeyResult::Complete(complete(state, self.yes)))
	}
}
