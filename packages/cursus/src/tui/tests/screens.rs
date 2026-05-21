use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::backend::TestBackend;
use ratatui::prelude::*;

use crate::tui::screens::*;
use crate::tui::test_utils::render_to_string;
use crate::tui::widgets::{ButtonDef, KeyResult};

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

	fn question(&self) -> String {
		"Test question?".to_string()
	}

	fn buttons(&self) -> Vec<ButtonDef> {
		vec![
			ButtonDef {
				label: "Yes".to_string(),
				selected: self.yes,
				color: None,
			},
			ButtonDef {
				label: "No".to_string(),
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
	crate::locale::set_locale("en");
	let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
	let buttons = TestButtons { yes: true };
	let content = render_to_string(&mut terminal, |frame| {
		buttons.render(frame, frame.area());
	});
	assert!(content.contains("Test question?"));
	assert!(content.contains("Yes"));
	assert!(content.contains("No"));
}
