use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;

use crate::tui::widgets::{self, ButtonDef, KeyResult};

use super::{HandleResult, Screen, WizardState, complete};

/// Handles key events for the [`Screen::OpenEditor`] screen.
pub(super) fn handle_open_editor(state: WizardState, yes: bool, key: KeyEvent) -> HandleResult {
	match key.code {
		KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::Char('h') | KeyCode::Char('l') => {
			Ok(KeyResult::Continue((state, Screen::OpenEditor(!yes))))
		}
		KeyCode::Enter => Ok(KeyResult::Complete(complete(state, yes))),
		KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
		_ => Ok(KeyResult::Continue((state, Screen::OpenEditor(yes)))),
	}
}

/// Renders the [`Screen::OpenEditor`] screen.
pub(super) fn render_open_editor(frame: &mut Frame, yes: bool) {
	let chunks = widgets::wizard_layout(
		frame,
		&[
			Constraint::Length(3),
			Constraint::Length(3),
			Constraint::Min(1),
		],
	);
	widgets::render_question(
		frame,
		chunks[0],
		"Open the config file in your editor after saving?",
		Color::Yellow,
	);
	widgets::render_button_row(
		frame,
		chunks[1],
		"Open Editor",
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
		chunks[2],
		"Use ←/→ or Tab to switch, Enter to confirm, Esc to cancel",
	);
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};

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
	fn ui_renders_open_editor() {
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| super::super::ui(frame, &state, &Screen::OpenEditor(false)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Open Editor"));
	}
}
