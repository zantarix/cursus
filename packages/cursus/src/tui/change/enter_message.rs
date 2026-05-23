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

/// Creates a new blank [`TextArea`] with standard wizard styling.
///
/// Returns a [`Box`] because `TextArea` is large (reducing `Screen` variant
/// size) and does not implement `Clone` or `PartialEq`.
pub(crate) fn initial_textarea() -> Box<TextArea<'static>> {
	let mut textarea = TextArea::default();
	textarea.set_block(
		Block::default()
			.borders(Borders::ALL)
			.title(crate::t!("enter-message-title")),
	);
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
pub(crate) fn handle_event_enter_message(
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
	let question = crate::t!("enter-message-question");
	let help = crate::t!("enter-message-help");
	let chunks = widgets::wizard_layout(
		area,
		&[
			Constraint::Length(widgets::paragraph_height(&question, area.width, 2)),
			Constraint::Min(3),
			Constraint::Length(1),
		],
	);
	widgets::render_question(frame, chunks[0], &question, Color::Yellow);
	frame.render_widget(textarea, chunks[1]);
	widgets::render_help(frame, chunks[2], &help);
}
