use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::prelude::*;

use crate::model::config::Strategy;
use crate::tui::widgets::{self, ButtonDef, KeyResult};

use super::{HandleResult, Screen, WizardState};

const QUESTION: &str = "Enable git automation? (commits, tags, push/branch on prepare and publish)";

fn enter_action(mut state: WizardState, yes: bool) -> HandleResult {
	state.git_enabled = yes;
	if yes {
		Ok(KeyResult::Continue((
			state,
			Screen::GitStrategy(Strategy::Push),
		)))
	} else {
		Ok(KeyResult::Continue((state, Screen::OpenEditor(false))))
	}
}

/// Handles events for the [`Screen::EnableGit`] screen.
pub(super) fn handle_enable_git(
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
			| KeyCode::Char('l') => Ok(KeyResult::Continue((state, Screen::EnableGit(!yes)))),
			KeyCode::Enter => enter_action(state, yes),
			KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
			_ => Ok(KeyResult::Continue((state, Screen::EnableGit(yes)))),
		},
		Event::Mouse(me) if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) => {
			if let Some(idx) =
				widgets::button_click_index(content_area, QUESTION, 2, me.column, me.row)
			{
				let clicked_yes = idx == 0;
				enter_action(state, clicked_yes)
			} else {
				Ok(KeyResult::Continue((state, Screen::EnableGit(yes))))
			}
		}
		_ => Ok(KeyResult::Continue((state, Screen::EnableGit(yes)))),
	}
}

/// Renders the [`Screen::EnableGit`] screen.
pub(super) fn render_enable_git(frame: &mut Frame, area: Rect, yes: bool) {
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

	use crate::model::config::Strategy;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};
	use super::*;

	#[test]
	fn enable_git_toggle() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (_, s) = unwrap_continue(handle_key(
			state,
			Screen::EnableGit(true),
			key(KeyCode::Tab),
		));
		assert!(matches!(s, Screen::EnableGit(false)));
	}

	#[test]
	fn enable_git_yes_advances_to_git_strategy() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::EnableGit(true),
			key(KeyCode::Enter),
		));
		assert!(new_state.git_enabled);
		assert!(matches!(s, Screen::GitStrategy(Strategy::Push)));
	}

	#[test]
	fn enable_git_no_advances_to_open_editor() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::EnableGit(false),
			key(KeyCode::Enter),
		));
		assert!(!new_state.git_enabled);
		assert!(matches!(s, Screen::OpenEditor(_)));
	}

	#[test]
	fn enable_git_esc_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		assert_cancelled(handle_key(
			state,
			Screen::EnableGit(true),
			key(KeyCode::Esc),
		));
	}

	#[test]
	fn enable_git_click_yes_button_advances_to_git_strategy() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (new_state, s) = unwrap_continue(handle_enable_git(
			state,
			false,
			mouse_click(10, area.y + 6),
			area,
		));
		assert!(new_state.git_enabled);
		assert!(matches!(s, Screen::GitStrategy(_)));
	}

	#[test]
	fn enable_git_click_no_button_advances_to_open_editor() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (new_state, s) = unwrap_continue(handle_enable_git(
			state,
			true,
			mouse_click(65, area.y + 6),
			area,
		));
		assert!(!new_state.git_enabled);
		assert!(matches!(s, Screen::OpenEditor(_)));
	}

	#[test]
	fn enable_git_click_outside_does_nothing() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (_, s) = unwrap_continue(handle_enable_git(
			state,
			true,
			mouse_click(10, area.y + 18),
			area,
		));
		assert!(matches!(s, Screen::EnableGit(true)));
	}

	#[test]
	fn ui_renders_enable_git() {
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| super::super::ui(frame, &state, &Screen::EnableGit(true)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Yes"));
		assert!(content.contains("No"));
	}
}
