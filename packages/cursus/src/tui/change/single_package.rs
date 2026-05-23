//! Single-package change level selection screen.
//!
//! Used when there is exactly one project in the workspace.

use crate::model::changeset::ChangeType;
use crate::package_manager::Project;
use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{BackState, ChangeResult, Screen, enter_message};

/// Button screen state for the single-package change level selector.
pub(crate) struct SinglePackageButtons {
	pub(crate) level: ChangeType,
}

impl ButtonScreen for SinglePackageButtons {
	/// The full project list (typically one entry for single-package repos).
	type State = Vec<Project>;
	type Result = ChangeResult;
	type FullScreen = Screen;

	fn question(&self) -> String {
		crate::t!("single-package-question")
	}

	fn buttons(&self) -> Vec<ButtonDef> {
		vec![
			ButtonDef {
				label: crate::t!("button-major"),
				selected: self.level == ChangeType::Major,
				color: None,
			},
			ButtonDef {
				label: crate::t!("button-minor"),
				selected: self.level == ChangeType::Minor,
				color: None,
			},
			ButtonDef {
				label: crate::t!("button-patch"),
				selected: self.level == ChangeType::Patch,
				color: None,
			},
		]
	}

	fn next(self) -> Self {
		SinglePackageButtons {
			level: self.level.next(),
		}
	}

	fn prev(self) -> Self {
		SinglePackageButtons {
			level: self.level.prev(),
		}
	}

	fn with_index(self, index: usize) -> Self {
		let level = match index {
			0 => ChangeType::Major,
			1 => ChangeType::Minor,
			_ => ChangeType::Patch,
		};
		SinglePackageButtons { level }
	}

	fn into_continue(self, state: Vec<Project>) -> (Vec<Project>, Screen) {
		(state, Screen::SinglePackage { level: self.level })
	}

	fn on_confirm(
		self,
		state: Vec<Project>,
	) -> anyhow::Result<KeyResult<(Vec<Project>, Screen), ChangeResult>> {
		let project = state
			.into_iter()
			.next()
			.ok_or_else(|| anyhow::anyhow!("SinglePackage requires at least one project"))?;
		let projects = vec![(project, self.level)];
		let back = BackState::SinglePackage { level: self.level };
		let textarea = enter_message::initial_textarea();
		// The empty vec is the State returned to the dispatcher; it is
		// discarded because the real project data lives in EnterMessage.
		Ok(KeyResult::Continue((
			vec![],
			Screen::EnterMessage {
				textarea,
				projects,
				back,
			},
		)))
	}
}
