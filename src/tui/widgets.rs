//! Shared TUI widget components for rendering and terminal lifecycle management.
//!
//! This module provides reusable rendering helpers and a generic event-loop
//! wrapper used by the init and change TUI wizards.

use std::io;
use std::rc::Rc;

use crossterm::{
	ExecutableCommand,
	event::{Event, KeyEvent, KeyEventKind},
	terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, Paragraph},
};

/// Result of processing a key press in a TUI wizard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyResult<S, T> {
	/// Continue with updated wizard state.
	Continue(S),
	/// Wizard completed with a value.
	Complete(T),
	/// Wizard cancelled by the user.
	Cancelled,
}

/// Definition of a single button in a button row widget.
pub struct ButtonDef<'a> {
	/// The text label displayed inside the button.
	pub label: &'a str,
	/// Whether this button is currently selected/highlighted.
	pub selected: bool,
	/// Optional foreground color override for the selected state.
	/// Defaults to `Color::Green` when `None`.
	pub color: Option<Color>,
}

/// Returns the style for a button based on selection state.
///
/// A selected button is rendered green, bold, and reversed.
/// An unselected button is rendered in gray.
pub fn button_style(selected: bool) -> Style {
	button_style_colored(selected, Color::Green)
}

/// Returns the style for a button with a custom foreground color when selected.
///
/// A selected button is rendered in `color`, bold, and reversed.
/// An unselected button is rendered in gray.
pub fn button_style_colored(selected: bool, color: Color) -> Style {
	if selected {
		Style::default()
			.fg(color)
			.add_modifier(Modifier::BOLD | Modifier::REVERSED)
	} else {
		Style::default().fg(Color::Gray)
	}
}

/// Renders a question prompt inside a bordered block.
///
/// Displays `text` in the given `color` inside a bordered block (no title)
/// at `area`.
pub fn render_question(frame: &mut Frame, area: Rect, text: &str, color: Color) {
	let question = Paragraph::new(text)
		.style(Style::default().fg(color))
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, area);
}

/// Renders dimmed help text at `area`.
///
/// Displays `text` in `Color::DarkGray` without a border.
pub fn render_help(frame: &mut Frame, area: Rect, text: &str) {
	let help = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
	frame.render_widget(help, area);
}

/// Renders a horizontal row of styled buttons inside a bordered block.
///
/// Buttons are separated by three spaces and styled according to their
/// `selected` state and optional `color` override. The block is given the
/// provided `title`.
pub fn render_button_row(frame: &mut Frame, area: Rect, title: &str, buttons: &[ButtonDef<'_>]) {
	let mut spans = vec![Span::raw("  ")];
	for (i, btn) in buttons.iter().enumerate() {
		if i > 0 {
			spans.push(Span::raw("   "));
		}
		let style = match btn.color {
			Some(color) => button_style_colored(btn.selected, color),
			None => button_style(btn.selected),
		};
		spans.push(Span::styled(format!(" {} ", btn.label), style));
	}
	spans.push(Span::raw("  "));
	let line = Line::from(spans);
	let para = Paragraph::new(line).block(Block::default().borders(Borders::ALL).title(title));
	frame.render_widget(para, area);
}

/// Returns an iterator of styled spans for a button with an underlined shortcut key.
///
/// The `shortcut` character is underlined to signal keyboard navigation.
/// All spans share the button style (selected or unselected).
pub fn button_spans<'a>(
	prefix: &'a str,
	shortcut: &'a str,
	suffix: &'a str,
	selected: bool,
) -> impl Iterator<Item = Span<'a>> {
	let base = button_style(selected);
	let underlined = base.add_modifier(Modifier::UNDERLINED);
	[
		Span::styled(prefix, base),
		Span::styled(shortcut, underlined),
		Span::styled(suffix, base),
	]
	.into_iter()
}

/// Creates the standard vertical wizard layout with a 2-cell margin.
///
/// Returns layout areas corresponding to `constraints`, split over
/// `frame.area()`.
pub fn wizard_layout(frame: &Frame, constraints: &[Constraint]) -> Rc<[Rect]> {
	Layout::default()
		.direction(Direction::Vertical)
		.margin(2)
		.constraints(constraints.iter().copied())
		.split(frame.area())
}

/// Runs the interactive TUI event loop with the given state and callbacks.
///
/// Handles terminal setup (`enable_raw_mode`, `EnterAlternateScreen`), the
/// key-event loop, and cleanup. The `draw_fn` renders each frame from the
/// current state, and `handle_fn` transitions the state given a key press,
/// returning a [`KeyResult`] to continue, complete, or cancel.
///
/// Terminal cleanup (`disable_raw_mode`, `LeaveAlternateScreen`) is always
/// performed, even when the loop exits due to an I/O error. On error, cleanup
/// failures are suppressed so the original error is preserved.
///
/// # Returns
///
/// `Ok(Some(T))` when the wizard completes, or `Ok(None)` if cancelled.
///
/// # Errors
///
/// Returns an error if terminal setup or I/O operations fail.
pub fn run_tui<S, T, DrawFn, HandleFn>(
	mut state: S,
	mut draw_fn: DrawFn,
	mut handle_fn: HandleFn,
) -> anyhow::Result<Option<T>>
where
	DrawFn: FnMut(&mut Frame, &S),
	HandleFn: FnMut(S, KeyEvent) -> anyhow::Result<KeyResult<S, T>>,
{
	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

	let result: anyhow::Result<Option<T>> = loop {
		if let Err(e) = terminal.draw(|frame| draw_fn(frame, &state)) {
			break Err(e.into());
		}
		match crossterm::event::read() {
			Err(e) => break Err(e.into()),
			Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => match handle_fn(state, key) {
				Err(e) => break Err(e),
				Ok(KeyResult::Continue(new_state)) => state = new_state,
				Ok(KeyResult::Complete(value)) => break Ok(Some(value)),
				Ok(KeyResult::Cancelled) => break Ok(None),
			},
			Ok(_) => {}
		}
	};

	// Always restore the terminal, even if the loop errored.
	// Cleanup errors are suppressed to preserve the primary error.
	disable_raw_mode().ok();
	io::stdout().execute(LeaveAlternateScreen).ok();

	result
}

#[cfg(test)]
mod tests {
	use ratatui::backend::TestBackend;

	use super::super::test_utils::render_to_string;
	use super::*;

	fn make_terminal() -> Terminal<TestBackend> {
		Terminal::new(TestBackend::new(80, 5)).unwrap()
	}

	// button_style tests
	#[test]
	fn button_style_selected_is_green_bold_reversed() {
		assert_eq!(
			button_style(true),
			Style::default()
				.fg(Color::Green)
				.add_modifier(Modifier::BOLD | Modifier::REVERSED)
		);
	}

	#[test]
	fn button_style_unselected_is_gray() {
		assert_eq!(button_style(false), Style::default().fg(Color::Gray));
	}

	// button_style_colored tests
	#[test]
	fn button_style_colored_selected_uses_given_color() {
		assert_eq!(
			button_style_colored(true, Color::Red),
			Style::default()
				.fg(Color::Red)
				.add_modifier(Modifier::BOLD | Modifier::REVERSED)
		);
	}

	#[test]
	fn button_style_colored_unselected_is_gray_regardless_of_color() {
		assert_eq!(
			button_style_colored(false, Color::Red),
			Style::default().fg(Color::Gray)
		);
	}

	// button_spans tests
	#[test]
	fn button_spans_returns_three_spans() {
		let spans: Vec<_> = button_spans(" ", "M", "ajor ", true).collect();
		assert_eq!(spans.len(), 3);
		assert_eq!(spans[0].content, " ");
		assert_eq!(spans[1].content, "M");
		assert_eq!(spans[2].content, "ajor ");
	}

	#[test]
	fn button_spans_shortcut_has_underline_modifier() {
		let spans: Vec<_> = button_spans(" ", "M", "ajor ", true).collect();
		assert!(spans[1].style.add_modifier.contains(Modifier::UNDERLINED));
	}

	#[test]
	fn button_spans_unselected_is_gray() {
		let spans: Vec<_> = button_spans(" ", "M", "ajor ", false).collect();
		for span in &spans {
			assert_eq!(span.style.fg, Some(Color::Gray));
		}
	}

	// render_question tests
	#[test]
	fn render_question_shows_text() {
		let mut terminal = make_terminal();
		let content = render_to_string(&mut terminal, |frame| {
			render_question(frame, frame.area(), "Is this correct?", Color::Yellow);
		});
		assert!(content.contains("Is this correct?"));
	}

	#[test]
	fn render_question_renders_border() {
		let mut terminal = make_terminal();
		let content = render_to_string(&mut terminal, |frame| {
			render_question(frame, frame.area(), "Q", Color::Red);
		});
		// Bordered block renders corner characters
		assert!(content.contains('─') || content.contains('│') || content.contains('┌'));
	}

	// render_help tests
	#[test]
	fn render_help_shows_text() {
		let mut terminal = make_terminal();
		let content = render_to_string(&mut terminal, |frame| {
			render_help(frame, frame.area(), "Press Esc to cancel");
		});
		assert!(content.contains("Press Esc to cancel"));
	}

	// render_button_row tests
	#[test]
	fn render_button_row_shows_title_and_labels() {
		let mut terminal = make_terminal();
		let content = render_to_string(&mut terminal, |frame| {
			render_button_row(
				frame,
				frame.area(),
				"Choose",
				&[
					ButtonDef {
						label: "Yes",
						selected: true,
						color: None,
					},
					ButtonDef {
						label: "No",
						selected: false,
						color: Some(Color::Red),
					},
				],
			);
		});
		assert!(content.contains("Choose"));
		assert!(content.contains("Yes"));
		assert!(content.contains("No"));
	}

	#[test]
	fn render_button_row_single_button() {
		let mut terminal = make_terminal();
		let content = render_to_string(&mut terminal, |frame| {
			render_button_row(
				frame,
				frame.area(),
				"Title",
				&[ButtonDef {
					label: "OK",
					selected: true,
					color: None,
				}],
			);
		});
		assert!(content.contains("OK"));
	}

	// wizard_layout tests
	#[test]
	fn wizard_layout_returns_correct_chunk_count() {
		let backend = TestBackend::new(80, 24);
		let mut terminal = Terminal::new(backend).unwrap();
		terminal
			.draw(|frame| {
				let chunks = wizard_layout(
					frame,
					&[
						Constraint::Length(3),
						Constraint::Length(3),
						Constraint::Min(1),
					],
				);
				assert_eq!(chunks.len(), 3);
			})
			.unwrap();
	}

	#[test]
	fn wizard_layout_applies_margin() {
		let backend = TestBackend::new(80, 24);
		let mut terminal = Terminal::new(backend).unwrap();
		terminal
			.draw(|frame| {
				let full_area = frame.area();
				let chunks = wizard_layout(frame, &[Constraint::Min(0)]);
				// The single chunk should be inset by the 2-cell margin on each side
				assert!(chunks[0].x >= full_area.x + 2);
				assert!(chunks[0].y >= full_area.y + 2);
			})
			.unwrap();
	}
}
