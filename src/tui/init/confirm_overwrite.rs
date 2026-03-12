use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;

use crate::tui::widgets::{self, ButtonDef, KeyResult};

use super::{HandleResult, PmFocus, Screen, WizardState, detect_package_managers};

/// Handles key events for the [`Screen::ConfirmOverwrite`] screen.
pub(super) fn handle_confirm_overwrite(
	state: WizardState,
	yes: bool,
	key: KeyEvent,
) -> HandleResult {
	match key.code {
		KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::Char('h') | KeyCode::Char('l') => {
			Ok(KeyResult::Continue((state, Screen::ConfirmOverwrite(!yes))))
		}
		KeyCode::Enter => {
			if yes {
				let (cargo, npm_detected) = detect_package_managers(&state.git_workdir);
				let npm = npm_detected || !cargo;
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
		KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
		_ => Ok(KeyResult::Continue((state, Screen::ConfirmOverwrite(yes)))),
	}
}

/// Renders the [`Screen::ConfirmOverwrite`] screen.
pub(super) fn render_confirm_overwrite(frame: &mut Frame, area: Rect, yes: bool) {
	let question = "Config already exists. Overwrite?";
	let chunks = widgets::wizard_layout(
		area,
		&[
			Constraint::Length(widgets::question_height(question, area.width)),
			Constraint::Length(3),
			Constraint::Length(1),
			Constraint::Min(1),
		],
	);
	widgets::render_question(frame, chunks[0], question, Color::Yellow);
	widgets::render_yes_no_buttons(
		frame,
		chunks[1],
		&[
			ButtonDef {
				label: "Yes",
				selected: yes,
				color: Some(Color::Red),
			},
			ButtonDef {
				label: "No",
				selected: !yes,
				color: None,
			},
		],
	);
	widgets::render_help(
		frame,
		chunks[3],
		"Use ←/→ or Tab to switch, Enter to confirm, Esc to cancel",
	);
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};

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
	fn ui_renders_confirm_overwrite() {
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
