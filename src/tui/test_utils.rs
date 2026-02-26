//! Shared test utilities for TUI unit tests.

use ratatui::{Terminal, backend::TestBackend};

/// Creates a test terminal with an 80×24 display area.
pub fn create_test_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(80, 24);
    Terminal::new(backend).unwrap()
}

/// Converts a terminal buffer to a plain string for assertion testing.
///
/// Each row is joined by a newline, with a trailing newline at the end.
pub fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
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
