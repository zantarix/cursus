use ratatui::prelude::*;

use crate::tui::screens::ButtonScreen;
use crate::tui::widgets::{ButtonDef, KeyResult};

use super::{InitResult, PmFocus, Screen, WizardState, detect_package_managers};

/// Button screen state for the [`Screen::ConfirmOverwrite`] screen.
pub(super) struct ConfirmOverwriteButtons {
	pub(super) yes: bool,
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
			let git_workdir_abs = crate::path::AbsolutePath::new(&state.git_workdir)
				.expect("git_workdir is absolute");
			let (cargo, npm) =
				detect_package_managers(&git_workdir_abs, &crate::filesystem::LocalFilesystem);
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

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};
	use super::*;

	#[test]
	fn confirm_overwrite_toggle() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ConfirmOverwrite(false);
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Left)));
		assert!(matches!(s, Screen::ConfirmOverwrite(true)));
	}

	#[test]
	fn confirm_overwrite_tab_toggles() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ConfirmOverwrite(true);
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Tab)));
		assert!(matches!(s, Screen::ConfirmOverwrite(false)));
	}

	#[test]
	fn confirm_overwrite_yes_advances_to_select_pms() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ConfirmOverwrite(true);
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert!(matches!(s, Screen::SelectPackageManagers { .. }));
	}

	#[test]
	fn confirm_overwrite_no_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ConfirmOverwrite(false);
		assert_cancelled(handle_key(state, screen, key(KeyCode::Enter)));
	}

	#[test]
	fn confirm_overwrite_esc_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ConfirmOverwrite(true);
		assert_cancelled(handle_key(state, screen, key(KeyCode::Esc)));
	}

	#[test]
	fn confirm_overwrite_q_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ConfirmOverwrite(true);
		assert_cancelled(handle_key(state, screen, key(KeyCode::Char('q'))));
	}

	#[test]
	fn confirm_overwrite_other_keys_do_nothing() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ConfirmOverwrite(true);
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Char('x'))));
		assert!(matches!(s, Screen::ConfirmOverwrite(true)));
	}

	#[test]
	fn confirm_overwrite_click_yes_button_advances_to_select_pms() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (_, s) = unwrap_continue(ConfirmOverwriteButtons { yes: false }.handle_event(
			state,
			mouse_click(10, area.y + 6),
			area,
		));
		assert!(matches!(s, Screen::SelectPackageManagers { .. }));
	}

	#[test]
	fn confirm_overwrite_click_no_button_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		assert_cancelled(ConfirmOverwriteButtons { yes: true }.handle_event(
			state,
			mouse_click(65, area.y + 6),
			area,
		));
	}

	#[test]
	fn confirm_overwrite_click_outside_buttons_does_nothing() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (_, s) = unwrap_continue(ConfirmOverwriteButtons { yes: false }.handle_event(
			state,
			mouse_click(10, area.y + 15),
			area,
		));
		assert!(matches!(s, Screen::ConfirmOverwrite(false)));
	}

	#[test]
	fn confirm_overwrite_yes_with_no_manifests_selects_nothing() {
		let dir = temp_dir(); // empty dir — no Cargo.toml, no package.json
		let state = make_state(&dir);
		let screen = Screen::ConfirmOverwrite(true);
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		match s {
			Screen::SelectPackageManagers { cargo, npm, .. } => {
				assert!(!cargo, "no Cargo.toml → cargo should be false");
				assert!(!npm, "no package.json → npm should be false");
			}
			other => panic!("Expected SelectPackageManagers, got {other:?}"),
		}
	}

	#[test]
	fn ui_renders_confirm_overwrite() {
		crate::locale::set_locale("en");
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| super::super::ui(frame, &state, &Screen::ConfirmOverwrite(false)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Overwrite"));
		assert!(content.contains("Yes"));
		assert!(content.contains("No"));
	}
}
