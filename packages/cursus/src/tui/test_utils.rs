//! Shared test utilities for TUI unit tests.

use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Frame, Terminal, backend::TestBackend};

/// Creates a test terminal with an 80×24 display area.
pub(super) fn create_test_terminal() -> Terminal<TestBackend> {
	let backend = TestBackend::new(80, 24);
	Terminal::new(backend).unwrap()
}

/// Converts a terminal buffer to a plain string for assertion testing.
///
/// Each row is joined by a newline, with a trailing newline at the end.
pub(super) fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
	(0..buffer.area.height)
		.map(|y| {
			(0..buffer.area.width)
				.map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
				.collect::<String>()
		})
		.collect::<Vec<_>>()
		.join("\n")
		+ "\n"
}

/// Constructs a left mouse button down event at `(col, row)`.
pub(super) fn mouse_click(col: u16, row: u16) -> Event {
	Event::Mouse(MouseEvent {
		kind: MouseEventKind::Down(MouseButton::Left),
		column: col,
		row,
		modifiers: KeyModifiers::NONE,
	})
}

/// Draws a single frame using `draw_fn` and returns the rendered buffer as a string.
///
/// Combines `terminal.draw()`, buffer cloning, and [`buffer_to_string`] into
/// one call, reducing boilerplate in rendering tests.
pub(super) fn render_to_string<F>(terminal: &mut Terminal<TestBackend>, draw_fn: F) -> String
where
	F: FnOnce(&mut Frame),
{
	terminal.draw(draw_fn).unwrap();
	buffer_to_string(terminal.backend().buffer())
}
