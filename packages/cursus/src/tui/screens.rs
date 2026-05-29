//! Reusable TUI screen traits for wizard implementations.

use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::prelude::*;

use super::widgets::{
	ButtonDef, KeyResult, button_click_index, paragraph_height, render_buttons, render_help,
	render_question, wizard_layout,
};

/// The [`KeyResult`] type for [`ButtonScreen`] event handlers.
///
/// Either continues with `(State, FullScreen)` or completes with `Result`.
pub(super) type ButtonKeyResult<State, FullScreen, Result> = KeyResult<(State, FullScreen), Result>;

/// A TUI screen that displays a question and N buttons.
///
/// Implementors provide the question text, button definitions, and state
/// transitions. The default [`ButtonScreen::handle_event`] and
/// [`ButtonScreen::render`] implementations cover shared keyboard handling,
/// mouse handling, and the standard wizard rendering layout.
pub(super) trait ButtonScreen: Sized {
	/// The wizard state type threaded through screens.
	type State;
	/// The wizard result type returned on completion.
	type Result;
	/// The full screen enum that wraps this screen's state.
	type FullScreen;

	/// The question text displayed at the top of the screen.
	fn question(&self) -> String;

	/// Foreground color for the question text. Defaults to [`Color::Yellow`].
	const QUESTION_COLOR: Color = Color::Yellow;

	/// Returns the button definitions for the current selection state.
	fn buttons(&self) -> Vec<ButtonDef>;

	/// Returns a copy of `self` with the selection advanced to the next option.
	fn next(self) -> Self;

	/// Returns a copy of `self` with the selection moved to the previous option.
	fn prev(self) -> Self;

	/// Returns a copy of `self` with the selection set to `index`.
	///
	/// Called when the user clicks a button. `index` is `0` for the first
	/// button, `1` for the second, and so on.
	fn with_index(self, index: usize) -> Self;

	/// Wraps `self` back into a `Continue` result without any state change.
	///
	/// Used for no-op key presses and mouse misses.
	fn into_continue(self, state: Self::State) -> (Self::State, Self::FullScreen);

	/// Handles the confirm action (Enter key or button click).
	///
	/// # Errors
	///
	/// Returns an error if any state transition fails.
	fn on_confirm(
		self,
		state: Self::State,
	) -> anyhow::Result<ButtonKeyResult<Self::State, Self::FullScreen, Self::Result>>;

	/// Handles an input event and returns the next state.
	///
	/// Dispatches cycle, confirm, cancel, mouse, and no-op events using the
	/// standard button wizard key bindings:
	/// - `←`/`h` → prev
	/// - `→`/`l`/`Tab` → next
	/// - `Enter` → confirm
	/// - `Esc`/`q` → cancel
	/// - Mouse left-click on a button → confirm that button
	/// - Any other mouse event → no-op (non-left-click events are ignored)
	///
	/// # Errors
	///
	/// Returns an error if `on_confirm` fails.
	fn handle_event(
		self,
		state: Self::State,
		event: Event,
		content_area: Rect,
	) -> anyhow::Result<ButtonKeyResult<Self::State, Self::FullScreen, Self::Result>> {
		let question = self.question();
		match event {
			Event::Key(KeyEvent { code, .. }) => match code {
				KeyCode::Left | KeyCode::Char('h') => {
					Ok(KeyResult::Continue(self.prev().into_continue(state)))
				}
				KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => {
					Ok(KeyResult::Continue(self.next().into_continue(state)))
				}
				KeyCode::Enter => self.on_confirm(state),
				KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
				_ => Ok(KeyResult::Continue(self.into_continue(state))),
			},
			Event::Mouse(me) if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) => {
				let Ok(n) = u16::try_from(self.buttons().len()) else {
					return Ok(KeyResult::Continue(self.into_continue(state)));
				};
				if let Some(idx) = button_click_index(content_area, &question, n, me.column, me.row)
				{
					self.with_index(idx).on_confirm(state)
				} else {
					Ok(KeyResult::Continue(self.into_continue(state)))
				}
			}
			// All other events (non-left-click mouse, resize, focus, etc.) are no-ops.
			_ => Ok(KeyResult::Continue(self.into_continue(state))),
		}
	}

	/// Renders the standard question + buttons + spacer + help layout.
	///
	/// `area` should be the content area below any tab bar. The question is
	/// rendered in [`Self::QUESTION_COLOR`] inside a bordered block, followed
	/// by the buttons and a help line at the bottom.
	fn render(&self, frame: &mut Frame, area: Rect) {
		let question = self.question();
		let chunks = wizard_layout(
			area,
			&[
				Constraint::Length(paragraph_height(&question, area.width, 2)),
				Constraint::Length(3),
				Constraint::Length(1),
				Constraint::Min(1),
			],
		);
		render_question(frame, chunks[0], &question, Self::QUESTION_COLOR);
		render_buttons(frame, chunks[1], &self.buttons());
		render_help(frame, chunks[3], &crate::t!("button-screen-help"));
	}
}
