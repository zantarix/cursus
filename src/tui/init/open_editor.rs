use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::prelude::*;

use crate::tui::widgets::{self, ButtonDef, KeyResult};

use super::{HandleResult, Screen, WizardState, complete};

const QUESTION: &str = "Open the config file in your editor after saving?";

/// Handles events for the [`Screen::OpenEditor`] screen.
pub(super) fn handle_open_editor(
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
			| KeyCode::Char('l') => Ok(KeyResult::Continue((state, Screen::OpenEditor(!yes)))),
			KeyCode::Enter => Ok(KeyResult::Complete(complete(state, yes))),
			KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
			_ => Ok(KeyResult::Continue((state, Screen::OpenEditor(yes)))),
		},
		Event::Mouse(me) if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) => {
			if let Some(idx) =
				widgets::button_click_index(content_area, QUESTION, 2, me.column, me.row)
			{
				let clicked_yes = idx == 0;
				Ok(KeyResult::Complete(complete(state, clicked_yes)))
			} else {
				Ok(KeyResult::Continue((state, Screen::OpenEditor(yes))))
			}
		}
		_ => Ok(KeyResult::Continue((state, Screen::OpenEditor(yes)))),
	}
}

/// Renders the [`Screen::OpenEditor`] screen.
pub(super) fn render_open_editor(frame: &mut Frame, area: Rect, yes: bool) {
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
				color: None,
			},
			ButtonDef {
				label: "No",
				selected: !yes,
				color: Some(Color::Red),
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
	fn open_editor_toggle() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (_, s) = unwrap_continue(handle_key(
			state,
			Screen::OpenEditor(false),
			key(KeyCode::Tab),
		));
		assert!(matches!(s, Screen::OpenEditor(true)));
	}

	#[test]
	fn open_editor_yes_completes_with_open_editor_true() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let result = unwrap_complete(handle_key(
			state,
			Screen::OpenEditor(true),
			key(KeyCode::Enter),
		));
		assert!(result.open_editor);
	}

	#[test]
	fn open_editor_no_completes_with_open_editor_false() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let result = unwrap_complete(handle_key(
			state,
			Screen::OpenEditor(false),
			key(KeyCode::Enter),
		));
		assert!(!result.open_editor);
	}

	#[test]
	fn open_editor_esc_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		assert_cancelled(handle_key(
			state,
			Screen::OpenEditor(false),
			key(KeyCode::Esc),
		));
	}

	#[test]
	fn open_editor_click_yes_button_completes_with_open_editor_true() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let result = unwrap_complete(handle_open_editor(
			state,
			false,
			mouse_click(10, area.y + 5),
			area,
		));
		assert!(result.open_editor);
	}

	#[test]
	fn open_editor_click_no_button_completes_with_open_editor_false() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let result = unwrap_complete(handle_open_editor(
			state,
			true,
			mouse_click(65, area.y + 5),
			area,
		));
		assert!(!result.open_editor);
	}

	#[test]
	fn open_editor_click_outside_does_nothing() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (_, s) = unwrap_continue(handle_open_editor(
			state,
			true,
			mouse_click(10, area.y + 18),
			area,
		));
		assert!(matches!(s, Screen::OpenEditor(true)));
	}

	#[test]
	fn ui_renders_open_editor() {
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| super::super::ui(frame, &state, &Screen::OpenEditor(false)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Yes"));
		assert!(content.contains("No"));
	}
}
