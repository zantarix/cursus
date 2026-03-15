//! Reusable TUI screen traits for wizard implementations.

use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::prelude::*;

use super::widgets::{
	ButtonDef, KeyResult, button_click_index, paragraph_height, render_help, render_question,
	render_yes_no_buttons, wizard_layout,
};

/// The [`KeyResult`] type for [`ButtonScreen`] event handlers.
///
/// Either continues with `(State, FullScreen)` or completes with `Result`.
pub type ButtonKeyResult<State, FullScreen, Result> = KeyResult<(State, FullScreen), Result>;

/// A TUI screen that displays a question and N buttons.
///
/// Implementors provide the question text, button definitions, and state
/// transitions. The default [`ButtonScreen::handle_event`] and
/// [`ButtonScreen::render`] implementations cover shared keyboard handling,
/// mouse handling, and the standard wizard rendering layout.
pub trait ButtonScreen: Sized {
	/// The wizard state type threaded through screens.
	type State;
	/// The wizard result type returned on completion.
	type Result;
	/// The full screen enum that wraps this screen's state.
	type FullScreen;

	/// The question text displayed at the top of the screen.
	///
	/// Defined as a `const` to avoid borrow-after-move: mouse handling reads
	/// the question text and then consumes `self` for `on_confirm`.
	const QUESTION: &'static str;

	/// Foreground color for the question text. Defaults to [`Color::Yellow`].
	const QUESTION_COLOR: Color = Color::Yellow;

	/// Returns the button definitions for the current selection state.
	fn buttons(&self) -> Vec<ButtonDef<'_>>;

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
				let n = self.buttons().len() as u16;
				if let Some(idx) =
					button_click_index(content_area, Self::QUESTION, n, me.column, me.row)
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
		let chunks = wizard_layout(
			area,
			&[
				Constraint::Length(paragraph_height(Self::QUESTION, area.width, 2)),
				Constraint::Length(3),
				Constraint::Length(1),
				Constraint::Min(1),
			],
		);
		render_question(frame, chunks[0], Self::QUESTION, Self::QUESTION_COLOR);
		render_yes_no_buttons(frame, chunks[1], &self.buttons());
		render_help(
			frame,
			chunks[3],
			"←/→ or click to switch, Enter or click to confirm, Esc to cancel",
		);
	}
}

#[cfg(test)]
mod tests {
	use ratatui::backend::TestBackend;

	use super::super::test_utils::render_to_string;
	use super::*;

	/// Minimal implementor used to test `ButtonScreen` default methods.
	#[derive(Clone, PartialEq, Debug)]
	struct TestButtons {
		yes: bool,
	}

	#[derive(Clone, PartialEq, Debug)]
	enum TestScreen {
		Test(bool),
	}

	#[derive(Clone, PartialEq, Debug)]
	struct TestState;

	#[derive(Clone, PartialEq, Debug)]
	struct TestResult(bool);

	impl ButtonScreen for TestButtons {
		type State = TestState;
		type Result = TestResult;
		type FullScreen = TestScreen;

		const QUESTION: &'static str = "Test question?";

		fn buttons(&self) -> Vec<ButtonDef<'_>> {
			vec![
				ButtonDef {
					label: "Yes",
					selected: self.yes,
					color: None,
				},
				ButtonDef {
					label: "No",
					selected: !self.yes,
					color: None,
				},
			]
		}

		fn next(self) -> Self {
			TestButtons { yes: !self.yes }
		}

		fn prev(self) -> Self {
			TestButtons { yes: !self.yes }
		}

		fn with_index(self, index: usize) -> Self {
			TestButtons { yes: index == 0 }
		}

		fn into_continue(self, state: TestState) -> (TestState, TestScreen) {
			(state, TestScreen::Test(self.yes))
		}

		fn on_confirm(
			self,
			_state: TestState,
		) -> anyhow::Result<KeyResult<(TestState, TestScreen), TestResult>> {
			Ok(KeyResult::Complete(TestResult(self.yes)))
		}
	}

	fn test_key(code: KeyCode) -> Event {
		Event::Key(KeyEvent::new(code, crossterm::event::KeyModifiers::NONE))
	}

	fn test_area() -> Rect {
		Rect::new(0, 0, 80, 24)
	}

	#[test]
	fn button_screen_prev_on_left() {
		let result = TestButtons { yes: true }
			.handle_event(TestState, test_key(KeyCode::Left), test_area())
			.unwrap();
		assert_eq!(
			result,
			KeyResult::Continue((TestState, TestScreen::Test(false)))
		);
	}

	#[test]
	fn button_screen_next_on_right() {
		let result = TestButtons { yes: false }
			.handle_event(TestState, test_key(KeyCode::Right), test_area())
			.unwrap();
		assert_eq!(
			result,
			KeyResult::Continue((TestState, TestScreen::Test(true)))
		);
	}

	#[test]
	fn button_screen_next_on_tab() {
		let result = TestButtons { yes: true }
			.handle_event(TestState, test_key(KeyCode::Tab), test_area())
			.unwrap();
		assert_eq!(
			result,
			KeyResult::Continue((TestState, TestScreen::Test(false)))
		);
	}

	#[test]
	fn button_screen_prev_on_h() {
		let result = TestButtons { yes: true }
			.handle_event(TestState, test_key(KeyCode::Char('h')), test_area())
			.unwrap();
		assert_eq!(
			result,
			KeyResult::Continue((TestState, TestScreen::Test(false)))
		);
	}

	#[test]
	fn button_screen_next_on_l() {
		let result = TestButtons { yes: false }
			.handle_event(TestState, test_key(KeyCode::Char('l')), test_area())
			.unwrap();
		assert_eq!(
			result,
			KeyResult::Continue((TestState, TestScreen::Test(true)))
		);
	}

	#[test]
	fn button_screen_confirm_on_enter() {
		let result = TestButtons { yes: true }
			.handle_event(TestState, test_key(KeyCode::Enter), test_area())
			.unwrap();
		assert_eq!(result, KeyResult::Complete(TestResult(true)));
	}

	#[test]
	fn button_screen_cancel_on_esc() {
		let result = TestButtons { yes: true }
			.handle_event(TestState, test_key(KeyCode::Esc), test_area())
			.unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn button_screen_cancel_on_q() {
		let result = TestButtons { yes: true }
			.handle_event(TestState, test_key(KeyCode::Char('q')), test_area())
			.unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn button_screen_noop_on_other_key() {
		let result = TestButtons { yes: true }
			.handle_event(TestState, test_key(KeyCode::Char('x')), test_area())
			.unwrap();
		assert_eq!(
			result,
			KeyResult::Continue((TestState, TestScreen::Test(true)))
		);
	}

	#[test]
	fn button_screen_click_first_button_confirms() {
		use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
		// question "Test question?" at width 80: height=3, buttons at y=5..8
		let event = Event::Mouse(MouseEvent {
			kind: MouseEventKind::Down(MouseButton::Left),
			column: 10,
			row: 6,
			modifiers: crossterm::event::KeyModifiers::NONE,
		});
		let result = TestButtons { yes: false }
			.handle_event(TestState, event, test_area())
			.unwrap();
		assert_eq!(result, KeyResult::Complete(TestResult(true)));
	}

	#[test]
	fn button_screen_click_second_button_confirms() {
		use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
		let event = Event::Mouse(MouseEvent {
			kind: MouseEventKind::Down(MouseButton::Left),
			column: 65,
			row: 6,
			modifiers: crossterm::event::KeyModifiers::NONE,
		});
		let result = TestButtons { yes: true }
			.handle_event(TestState, event, test_area())
			.unwrap();
		assert_eq!(result, KeyResult::Complete(TestResult(false)));
	}

	#[test]
	fn button_screen_click_outside_is_noop() {
		use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
		let event = Event::Mouse(MouseEvent {
			kind: MouseEventKind::Down(MouseButton::Left),
			column: 10,
			row: 20,
			modifiers: crossterm::event::KeyModifiers::NONE,
		});
		let result = TestButtons { yes: true }
			.handle_event(TestState, event, test_area())
			.unwrap();
		assert_eq!(
			result,
			KeyResult::Continue((TestState, TestScreen::Test(true)))
		);
	}

	#[test]
	fn button_screen_non_left_click_mouse_is_noop() {
		use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
		// Right-click directly on the button area should be ignored (non-left-click no-op).
		let event = Event::Mouse(MouseEvent {
			kind: MouseEventKind::Down(MouseButton::Right),
			column: 10,
			row: 6,
			modifiers: crossterm::event::KeyModifiers::NONE,
		});
		let result = TestButtons { yes: true }
			.handle_event(TestState, event, test_area())
			.unwrap();
		assert_eq!(
			result,
			KeyResult::Continue((TestState, TestScreen::Test(true)))
		);
	}

	#[test]
	fn button_screen_render_shows_question_and_buttons() {
		let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
		let buttons = TestButtons { yes: true };
		let content = render_to_string(&mut terminal, |frame| {
			buttons.render(frame, frame.area());
		});
		assert!(content.contains("Test question?"));
		assert!(content.contains("Yes"));
		assert!(content.contains("No"));
	}
}
