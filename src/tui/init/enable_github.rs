use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::prelude::*;

use crate::tui::widgets::{self, ButtonDef, KeyResult};

use super::{HandleResult, Screen, WizardState, edit_github::make_edit_github_screen};

const QUESTION: &str = "Enable GitHub Releases? (creates a release on GitHub after publish)";

fn enter_action(mut state: WizardState, yes: bool) -> HandleResult {
	state.github_enabled = yes;
	if yes {
		let screen = make_edit_github_screen(&state);
		Ok(KeyResult::Continue((state, screen)))
	} else {
		Ok(KeyResult::Continue((state, Screen::OpenEditor(false))))
	}
}

/// Handles events for the [`Screen::EnableGitHub`] screen.
pub(super) fn handle_enable_github(
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
			| KeyCode::Char('l') => Ok(KeyResult::Continue((state, Screen::EnableGitHub(!yes)))),
			KeyCode::Enter => enter_action(state, yes),
			KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
			_ => Ok(KeyResult::Continue((state, Screen::EnableGitHub(yes)))),
		},
		Event::Mouse(me) if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) => {
			if let Some(idx) =
				widgets::button_click_index(content_area, QUESTION, 2, me.column, me.row)
			{
				let clicked_yes = idx == 0;
				enter_action(state, clicked_yes)
			} else {
				Ok(KeyResult::Continue((state, Screen::EnableGitHub(yes))))
			}
		}
		_ => Ok(KeyResult::Continue((state, Screen::EnableGitHub(yes)))),
	}
}

/// Renders the [`Screen::EnableGitHub`] screen.
pub(super) fn render_enable_github(frame: &mut Frame, area: Rect, yes: bool) {
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
	fn enable_github_toggle() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (_, s) = unwrap_continue(handle_key(
			state,
			Screen::EnableGitHub(false),
			key(KeyCode::Tab),
		));
		assert!(matches!(s, Screen::EnableGitHub(true)));
	}

	#[test]
	fn enable_github_yes_advances_to_edit_github() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::EnableGitHub(true),
			key(KeyCode::Enter),
		));
		assert!(new_state.github_enabled);
		assert!(matches!(s, Screen::EditGitHub { .. }));
	}

	#[test]
	fn enable_github_no_advances_to_open_editor() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::EnableGitHub(false),
			key(KeyCode::Enter),
		));
		assert!(!new_state.github_enabled);
		assert!(matches!(s, Screen::OpenEditor(_)));
	}

	#[test]
	fn enable_github_esc_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		assert_cancelled(handle_key(
			state,
			Screen::EnableGitHub(false),
			key(KeyCode::Esc),
		));
	}

	#[test]
	fn enable_github_click_yes_button_advances_to_edit_github() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (new_state, s) = unwrap_continue(handle_enable_github(
			state,
			false,
			mouse_click(10, area.y + 5),
			area,
		));
		assert!(new_state.github_enabled);
		assert!(matches!(s, Screen::EditGitHub { .. }));
	}

	#[test]
	fn enable_github_click_no_button_advances_to_open_editor() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (new_state, s) = unwrap_continue(handle_enable_github(
			state,
			true,
			mouse_click(65, area.y + 5),
			area,
		));
		assert!(!new_state.github_enabled);
		assert!(matches!(s, Screen::OpenEditor(_)));
	}

	#[test]
	fn enable_github_click_outside_does_nothing() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (_, s) = unwrap_continue(handle_enable_github(
			state,
			false,
			mouse_click(10, area.y + 18),
			area,
		));
		assert!(matches!(s, Screen::EnableGitHub(false)));
	}

	#[test]
	fn ui_renders_enable_github() {
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| super::super::ui(frame, &state, &Screen::EnableGitHub(false)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("GitHub Releases"));
	}
}
