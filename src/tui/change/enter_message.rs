//! Message input screen for the change wizard.
//!
//! Provides a TextArea-based screen for the user to enter a changeset
//! description. Enter submits, Shift+Enter inserts a newline, Ctrl+E
//! hands off to an external editor, and Esc navigates back.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders},
};
use ratatui_textarea::TextArea;

use crate::model::changeset::ChangeType;
use crate::package_manager::Project;
use crate::tui::widgets::{self, KeyResult};

use super::{BackState, ChangeResult, HandleResult, Screen};

const QUESTION: &str = "Describe this change:";
const HELP: &str =
	"Enter: submit | Alt+Enter (or Shift+Enter): newline | Ctrl+E: open editor | Esc: back";

/// Creates a new blank [`TextArea`] with standard wizard styling.
///
/// Returns a [`Box`] because `TextArea` is large (reducing `Screen` variant
/// size) and does not implement `Clone` or `PartialEq`.
pub(super) fn initial_textarea() -> Box<TextArea<'static>> {
	let mut textarea = TextArea::default();
	textarea.set_block(Block::default().borders(Borders::ALL).title("Message"));
	Box::new(textarea)
}

fn back_to_screen(back: BackState) -> Screen {
	match back {
		BackState::MultiPackage(mut state) => {
			state.error = false;
			Screen::SelectProjects(state)
		}
		BackState::SinglePackage { level } => Screen::SinglePackage { level },
	}
}

/// Handles events for the [`Screen::EnterMessage`] screen.
pub(super) fn handle_event_enter_message(
	mut textarea: Box<TextArea<'static>>,
	projects: Vec<(Project, ChangeType)>,
	back: BackState,
	event: Event,
) -> anyhow::Result<HandleResult> {
	match event {
		// Bare Enter → submit; Alt+Enter or Shift+Enter → newline (falls through to textarea)
		Event::Key(KeyEvent {
			code: KeyCode::Enter,
			modifiers,
			..
		}) if !modifiers.contains(KeyModifiers::ALT)
			&& !modifiers.contains(KeyModifiers::SHIFT) =>
		{
			let message = textarea.lines().join("\n");
			Ok(KeyResult::Complete(ChangeResult {
				projects,
				message: Some(message),
			}))
		}
		// Ctrl+E → hand off to external editor
		Event::Key(KeyEvent {
			code: KeyCode::Char('e'),
			modifiers,
			..
		}) if modifiers.contains(KeyModifiers::CONTROL) => Ok(KeyResult::Complete(ChangeResult {
			projects,
			message: None,
		})),
		// Esc → navigate back to previous screen
		Event::Key(KeyEvent {
			code: KeyCode::Esc, ..
		}) => Ok(KeyResult::Continue(back_to_screen(back))),
		// All other keys → forward to textarea
		other => {
			textarea.input(other);
			Ok(KeyResult::Continue(Screen::EnterMessage {
				textarea,
				projects,
				back,
			}))
		}
	}
}

/// Renders the [`Screen::EnterMessage`] screen.
pub(super) fn render_enter_message(frame: &mut Frame, area: Rect, textarea: &TextArea<'static>) {
	let chunks = widgets::wizard_layout(
		area,
		&[
			Constraint::Length(widgets::paragraph_height(QUESTION, area.width, 2)),
			Constraint::Min(3),
			Constraint::Length(1),
		],
	);
	widgets::render_question(frame, chunks[0], QUESTION, Color::Yellow);
	frame.render_widget(textarea, chunks[1]);
	widgets::render_help(frame, chunks[2], HELP);
}

#[cfg(test)]
mod tests {
	use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

	use crate::model::changeset::ChangeType;
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};

	use super::super::test_helpers::dummy_projects;
	use super::super::{BackState, Screen};
	use super::*;

	fn key(code: KeyCode) -> Event {
		Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
	}

	fn key_with_mods(code: KeyCode, mods: KeyModifiers) -> Event {
		Event::Key(KeyEvent::new(code, mods))
	}

	fn empty_back() -> BackState {
		BackState::SinglePackage {
			level: ChangeType::Patch,
		}
	}

	fn projects_with_patch() -> Vec<(crate::package_manager::Project, ChangeType)> {
		let p = dummy_projects(1);
		vec![(p.into_iter().next().unwrap(), ChangeType::Patch)]
	}

	#[test]
	fn enter_submits_with_current_text() {
		let mut ta = initial_textarea();
		ta.insert_str("My change description");
		let result = handle_event_enter_message(
			ta,
			projects_with_patch(),
			empty_back(),
			key(KeyCode::Enter),
		)
		.unwrap();
		match result {
			KeyResult::Complete(ChangeResult {
				message: Some(msg), ..
			}) => {
				assert_eq!(msg, "My change description");
			}
			_ => panic!("Expected Complete with message"),
		}
	}

	#[test]
	fn enter_submits_with_empty_text() {
		let ta = initial_textarea();
		let result = handle_event_enter_message(
			ta,
			projects_with_patch(),
			empty_back(),
			key(KeyCode::Enter),
		)
		.unwrap();
		match result {
			KeyResult::Complete(ChangeResult {
				message: Some(msg), ..
			}) => {
				assert_eq!(msg, "");
			}
			_ => panic!("Expected Complete with empty message"),
		}
	}

	#[test]
	fn ctrl_e_completes_with_none_message() {
		let ta = initial_textarea();
		let result = handle_event_enter_message(
			ta,
			projects_with_patch(),
			empty_back(),
			key_with_mods(KeyCode::Char('e'), KeyModifiers::CONTROL),
		)
		.unwrap();
		match result {
			KeyResult::Complete(ChangeResult { message: None, .. }) => {}
			_ => panic!("Expected Complete with None message"),
		}
	}

	#[test]
	fn esc_goes_back_to_single_package() {
		let ta = initial_textarea();
		let back = BackState::SinglePackage {
			level: ChangeType::Minor,
		};
		let result =
			handle_event_enter_message(ta, projects_with_patch(), back, key(KeyCode::Esc)).unwrap();
		match result {
			KeyResult::Continue(Screen::SinglePackage { level }) => {
				assert_eq!(level, ChangeType::Minor);
			}
			_ => panic!("Expected Continue(SinglePackage)"),
		}
	}

	#[test]
	fn esc_goes_back_to_select_projects() {
		use crate::tui::change::SelectProjectsState;
		let ta = initial_textarea();
		let back = BackState::MultiPackage(SelectProjectsState {
			selected: vec![true, false],
			levels: vec![ChangeType::Major, ChangeType::Patch],
			cursor: 0,
			error: false,
			changed_count: 1,
		});
		let result =
			handle_event_enter_message(ta, projects_with_patch(), back, key(KeyCode::Esc)).unwrap();
		match result {
			KeyResult::Continue(Screen::SelectProjects(SelectProjectsState {
				selected,
				levels,
				cursor,
				error,
				changed_count,
			})) => {
				assert_eq!(selected, vec![true, false]);
				assert_eq!(levels, vec![ChangeType::Major, ChangeType::Patch]);
				assert_eq!(cursor, 0);
				assert!(!error);
				assert_eq!(changed_count, 1);
			}
			_ => panic!("Expected Continue(SelectProjects)"),
		}
	}

	#[test]
	fn shift_enter_inserts_newline() {
		let ta = initial_textarea();
		let event = key_with_mods(KeyCode::Enter, KeyModifiers::SHIFT);
		let result =
			handle_event_enter_message(ta, projects_with_patch(), empty_back(), event).unwrap();
		match result {
			KeyResult::Continue(Screen::EnterMessage { textarea, .. }) => {
				// TextArea should have 2 lines after a newline insertion
				assert_eq!(textarea.lines().len(), 2);
			}
			_ => panic!("Expected Continue(EnterMessage)"),
		}
	}

	#[test]
	fn other_keys_are_forwarded_to_textarea() {
		let ta = initial_textarea();
		let result = handle_event_enter_message(
			ta,
			projects_with_patch(),
			empty_back(),
			key(KeyCode::Char('x')),
		)
		.unwrap();
		match result {
			KeyResult::Continue(Screen::EnterMessage { textarea, .. }) => {
				assert_eq!(textarea.lines()[0], "x");
			}
			_ => panic!("Expected Continue(EnterMessage)"),
		}
	}

	#[test]
	fn enter_preserves_projects() {
		let projects = {
			let p = dummy_projects(2);
			vec![
				(p[0].clone(), ChangeType::Major),
				(p[1].clone(), ChangeType::Minor),
			]
		};
		let ta = initial_textarea();
		let result =
			handle_event_enter_message(ta, projects, empty_back(), key(KeyCode::Enter)).unwrap();
		match result {
			KeyResult::Complete(ChangeResult { projects: proj, .. }) => {
				assert_eq!(proj.len(), 2);
				assert_eq!(proj[0].1, ChangeType::Major);
				assert_eq!(proj[1].1, ChangeType::Minor);
			}
			_ => panic!("Expected Complete"),
		}
	}

	#[test]
	fn ui_renders_enter_message_screen() {
		let mut terminal = create_test_terminal();
		let projects = dummy_projects(1);
		let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
		let ta = initial_textarea();
		let screen = Screen::EnterMessage {
			textarea: ta,
			projects: vec![(projects[0].clone(), ChangeType::Patch)],
			back: empty_back(),
		};
		terminal
			.draw(|frame| super::super::ui(frame, &screen, &names))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Describe this change"));
		assert!(content.contains("Message"));
	}
}
