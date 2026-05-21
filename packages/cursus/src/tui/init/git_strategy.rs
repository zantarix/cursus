use crate::model::config::Strategy;
use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{ForgeChoice, InitResult, Screen, WizardState};

/// Button screen state for the [`Screen::GitStrategy`] screen.
pub(crate) struct GitStrategyButtons {
	pub(crate) strategy: Strategy,
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
