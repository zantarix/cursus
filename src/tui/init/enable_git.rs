use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;

use crate::model::config::Strategy;
use crate::tui::widgets::{self, ButtonDef, KeyResult};

use super::{HandleResult, Screen, WizardState};

/// Handles key events for the [`Screen::EnableGit`] screen.
pub(super) fn handle_enable_git(mut state: WizardState, yes: bool, key: KeyEvent) -> HandleResult {
	match key.code {
		KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::Char('h') | KeyCode::Char('l') => {
			Ok(KeyResult::Continue((state, Screen::EnableGit(!yes))))
		}
		KeyCode::Enter => {
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
		KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
		_ => Ok(KeyResult::Continue((state, Screen::EnableGit(yes)))),
	}
}

/// Renders the [`Screen::EnableGit`] screen.
pub(super) fn render_enable_git(frame: &mut Frame, area: Rect, yes: bool) {
	let question = "Enable git automation? (commits, tags, push/branch on prepare and publish)";
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
		"Use ←/→ or Tab to switch, Enter to confirm, Esc to cancel",
	);
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use crate::model::config::Strategy;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};

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
