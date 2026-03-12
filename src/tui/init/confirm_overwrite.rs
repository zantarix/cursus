use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::prelude::*;

use crate::tui::widgets::{self, ButtonDef, KeyResult};

use super::{HandleResult, PmFocus, Screen, WizardState, detect_package_managers};

const QUESTION: &str = "Config already exists. Overwrite?";

fn enter_action(state: WizardState, yes: bool) -> HandleResult {
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

/// Handles events for the [`Screen::ConfirmOverwrite`] screen.
pub(super) fn handle_confirm_overwrite(
	state: WizardState,
	yes: bool,
	event: Event,
	content_area: Rect,
) -> HandleResult {
	match event {
		Event::Key(KeyEvent { code, .. }) => match code {
			KeyCode::Left
			| KeyCode::Right
			| KeyCode::Tab
			| KeyCode::Char('h')
			| KeyCode::Char('l') => Ok(KeyResult::Continue((state, Screen::ConfirmOverwrite(!yes)))),
			KeyCode::Enter => enter_action(state, yes),
			KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
			_ => Ok(KeyResult::Continue((state, Screen::ConfirmOverwrite(yes)))),
		},
		Event::Mouse(me) if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) => {
			if let Some(idx) =
				widgets::button_click_index(content_area, QUESTION, 2, me.column, me.row)
			{
				let clicked_yes = idx == 0;
				enter_action(state, clicked_yes)
			} else {
				Ok(KeyResult::Continue((state, Screen::ConfirmOverwrite(yes))))
			}
		}
		_ => Ok(KeyResult::Continue((state, Screen::ConfirmOverwrite(yes)))),
	}
}

/// Renders the [`Screen::ConfirmOverwrite`] screen.
pub(super) fn render_confirm_overwrite(frame: &mut Frame, area: Rect, yes: bool) {
	let chunks = widgets::wizard_layout(
		area,
		&[
			Constraint::Length(widgets::paragraph_height(QUESTION, area.width, 2)),
			Constraint::Length(3),
			Constraint::Length(1),
			Constraint::Min(1),
		],
	);
	widgets::render_question(frame, chunks[0], QUESTION, Color::Yellow);
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
		"←/→/Tab or click to switch, Enter or click to confirm, Esc to cancel",
	);
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
		let (_, s) = unwrap_continue(handle_confirm_overwrite(
			state,
			false,
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
		assert_cancelled(handle_confirm_overwrite(
			state,
			true,
			mouse_click(65, area.y + 6),
			area,
		));
	}

	#[test]
	fn confirm_overwrite_click_outside_buttons_does_nothing() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (_, s) = unwrap_continue(handle_confirm_overwrite(
			state,
			false,
			mouse_click(10, area.y + 15),
			area,
		));
		assert!(matches!(s, Screen::ConfirmOverwrite(false)));
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
