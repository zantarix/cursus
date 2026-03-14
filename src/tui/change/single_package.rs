//! Single-package change level selection screen.
//!
//! Used when there is exactly one project in the workspace.

use crate::model::changeset::ChangeType;
use crate::package_manager::Project;
use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{BackState, ChangeResult, Screen, enter_message};

/// Button screen state for the single-package change level selector.
pub(super) struct SinglePackageButtons {
	pub(super) level: ChangeType,
}

impl ButtonScreen for SinglePackageButtons {
	/// The full project list (typically one entry for single-package repos).
	type State = Vec<Project>;
	type Result = ChangeResult;
	type FullScreen = Screen;

	const QUESTION: &'static str = "What type of change is this?";

	fn buttons(&self) -> Vec<ButtonDef<'_>> {
		vec![
			ButtonDef {
				label: "Major",
				selected: self.level == ChangeType::Major,
				color: None,
			},
			ButtonDef {
				label: "Minor",
				selected: self.level == ChangeType::Minor,
				color: None,
			},
			ButtonDef {
				label: "Patch",
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

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;
	use ratatui::prelude::Rect;

	use crate::model::changeset::ChangeType;
	use crate::tui::screens::ButtonScreen;
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};

	use super::super::test_helpers::dummy_projects;
	use super::super::{Screen, handle_key};
	use super::*;

	fn single_package_screen(level: ChangeType) -> Screen {
		Screen::SinglePackage { level }
	}

	/// Unwrap a `Continue(SinglePackage { level })` result.
	fn unwrap_single_package(result: anyhow::Result<super::super::HandleResult>) -> ChangeType {
		match result.unwrap() {
			super::super::KeyResult::Continue(Screen::SinglePackage { level }) => level,
			_ => panic!("Expected Continue(SinglePackage)"),
		}
	}

	#[test]
	fn single_package_right_cycles_to_next_level() {
		let projects = dummy_projects(1);
		let level = unwrap_single_package(handle_key(
			single_package_screen(ChangeType::Patch),
			KeyCode::Right,
			&projects,
		));
		assert_eq!(level, ChangeType::Major);
	}

	#[test]
	fn single_package_left_cycles_to_prev_level() {
		let projects = dummy_projects(1);
		let level = unwrap_single_package(handle_key(
			single_package_screen(ChangeType::Patch),
			KeyCode::Left,
			&projects,
		));
		assert_eq!(level, ChangeType::Minor);
	}

	#[test]
	fn single_package_tab_cycles_forward() {
		let projects = dummy_projects(1);
		let level = unwrap_single_package(handle_key(
			single_package_screen(ChangeType::Major),
			KeyCode::Tab,
			&projects,
		));
		assert_eq!(level, ChangeType::Minor);
	}

	#[test]
	fn single_package_enter_advances_to_enter_message() {
		let projects = dummy_projects(1);
		let result = handle_key(
			single_package_screen(ChangeType::Minor),
			KeyCode::Enter,
			&projects,
		)
		.unwrap();
		match result {
			super::super::KeyResult::Continue(Screen::EnterMessage { projects: proj, .. }) => {
				assert_eq!(proj.len(), 1);
				assert_eq!(proj[0].0.name(), "project-0");
				assert_eq!(proj[0].1, ChangeType::Minor);
			}
			_ => panic!("Expected Continue(EnterMessage)"),
		}
	}

	#[test]
	fn single_package_esc_cancels() {
		let projects = dummy_projects(1);
		let result = handle_key(
			single_package_screen(ChangeType::Patch),
			KeyCode::Esc,
			&projects,
		)
		.unwrap();
		assert!(matches!(result, super::super::KeyResult::Cancelled));
	}

	#[test]
	fn single_package_next_cycles_all_three() {
		assert_eq!(
			SinglePackageButtons {
				level: ChangeType::Major
			}
			.next()
			.level,
			ChangeType::Minor
		);
		assert_eq!(
			SinglePackageButtons {
				level: ChangeType::Minor
			}
			.next()
			.level,
			ChangeType::Patch
		);
		assert_eq!(
			SinglePackageButtons {
				level: ChangeType::Patch
			}
			.next()
			.level,
			ChangeType::Major
		);
	}

	#[test]
	fn single_package_prev_cycles_all_three() {
		assert_eq!(
			SinglePackageButtons {
				level: ChangeType::Major
			}
			.prev()
			.level,
			ChangeType::Patch
		);
		assert_eq!(
			SinglePackageButtons {
				level: ChangeType::Minor
			}
			.prev()
			.level,
			ChangeType::Major
		);
		assert_eq!(
			SinglePackageButtons {
				level: ChangeType::Patch
			}
			.prev()
			.level,
			ChangeType::Minor
		);
	}

	#[test]
	fn single_package_with_index_maps_correctly() {
		assert_eq!(
			SinglePackageButtons {
				level: ChangeType::Patch
			}
			.with_index(0)
			.level,
			ChangeType::Major
		);
		assert_eq!(
			SinglePackageButtons {
				level: ChangeType::Patch
			}
			.with_index(1)
			.level,
			ChangeType::Minor
		);
		assert_eq!(
			SinglePackageButtons {
				level: ChangeType::Patch
			}
			.with_index(2)
			.level,
			ChangeType::Patch
		);
	}

	#[test]
	fn ui_renders_single_package_screen() {
		let mut terminal = create_test_terminal();
		let projects = dummy_projects(1);
		let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
		let screen = single_package_screen(ChangeType::Minor);
		terminal
			.draw(|frame| super::super::ui(frame, &screen, &names))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Major"));
		assert!(content.contains("Minor"));
		assert!(content.contains("Patch"));
		assert!(content.contains("What type of change"));
	}

	#[test]
	fn single_package_click_major_button_advances_to_enter_message() {
		use crate::tui::test_utils::mouse_click;
		let projects = dummy_projects(1);
		let area = Rect::new(0, 0, 80, 24);
		let buttons = SinglePackageButtons {
			level: ChangeType::Patch,
		};
		let result = buttons
			.handle_event(vec![projects[0].clone()], mouse_click(10, 5), area)
			.unwrap();
		match result {
			KeyResult::Continue((_, Screen::EnterMessage { projects: proj, .. })) => {
				assert_eq!(proj[0].1, ChangeType::Major);
			}
			_ => panic!("Expected Continue(EnterMessage)"),
		}
	}
}
