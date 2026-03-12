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
	widgets::{Block, Borders, Paragraph, Wrap},
};

/// Display state of a single tab in a progress tab bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStatus {
	/// This step has been completed.
	Completed,
	/// This is the current active step.
	Current,
	/// This step has not been reached yet.
	Future,
}

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

/// Computes the height a question block needs to display `text` without clipping.
///
/// Accounts for the 2-cell wizard margin and 1-cell border on each side
/// (`area_width - 6` usable text columns). Returns at least 3 (border + one
/// line + border).
pub fn question_height(text: &str, area_width: u16) -> u16 {
	let inner = area_width.saturating_sub(6);
	let lines = Paragraph::new(text)
		.wrap(Wrap { trim: false })
		.line_count(inner) as u16;
	(lines + 2).max(3) // top and bottom border; minimum 3 for degenerate widths
}

/// Renders a question prompt inside a bordered block.
///
/// Displays `text` in the given `color` inside a bordered block (no title)
/// at `area`.
pub fn render_question(frame: &mut Frame, area: Rect, text: &str, color: Color) {
	let question = Paragraph::new(text)
		.style(Style::default().fg(color))
		.wrap(Wrap { trim: false })
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, area);
}

/// Renders dimmed help text at `area`.
///
/// Displays `text` in `Color::DarkGray` without a border.
pub fn render_help(frame: &mut Frame, area: Rect, text: &str) {
	let help = Paragraph::new(text)
		.style(Style::default().fg(Color::DarkGray))
		.wrap(Wrap { trim: false });
	frame.render_widget(help, area);
}

/// Renders a horizontal progress tab bar spanning the full width of `area`.
///
/// Tabs are split equally. Each tab label is centred and styled according to
/// its [`TabStatus`]: green for completed, bold blue for current, dark grey
/// for future.
pub fn render_tabs(frame: &mut Frame, area: Rect, tabs: &[(&str, TabStatus)]) {
	if tabs.is_empty() {
		return;
	}
	let n = tabs.len() as u16;
	let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Fill(1)).collect();
	let cells = Layout::horizontal(constraints).split(area);
	for ((label, status), &cell) in tabs.iter().zip(cells.iter()) {
		let style = match status {
			TabStatus::Completed => Style::default().fg(Color::White).bg(Color::Green),
			TabStatus::Current => Style::default()
				.fg(Color::White)
				.bg(Color::Blue)
				.add_modifier(Modifier::BOLD),
			TabStatus::Future => Style::default().fg(Color::White).bg(Color::DarkGray),
		};
		frame.render_widget(
			Paragraph::new(Text::from(vec![
				Line::from(""),
				Line::from(*label),
				Line::from(""),
			]))
			.alignment(Alignment::Center)
			.style(style),
			cell,
		);
	}
}

/// Renders either/or buttons as equal-width blocks with one blank line of
/// padding above and below each label.
///
/// Each button occupies an equal share of `area`. The selected button's style
/// fills the entire button area. Unselected buttons have a dark grey background.
pub fn render_yes_no_buttons(frame: &mut Frame, area: Rect, buttons: &[ButtonDef<'_>]) {
	if buttons.is_empty() {
		return;
	}
	let n = buttons.len() as u16;
	let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Percentage(100 / n)).collect();
	let cells = Layout::horizontal(constraints).spacing(1).split(area);
	for (btn, &cell) in buttons.iter().zip(cells.iter()) {
		let style = if btn.selected {
			match btn.color {
				Some(color) => button_style_colored(true, color),
				None => button_style(true),
			}
		} else {
			Style::default().fg(Color::Gray).bg(Color::DarkGray)
		};
		let content = Text::from(vec![Line::from(""), Line::from(btn.label), Line::from("")]);
		let para = Paragraph::new(content)
			.alignment(Alignment::Center)
			.style(style);
		frame.render_widget(para, cell);
	}
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
/// Returns layout areas corresponding to `constraints`, split over `area`.
///
/// Applies a 2-cell margin on all sides.
pub fn wizard_layout(area: Rect, constraints: &[Constraint]) -> Rc<[Rect]> {
	Layout::default()
		.direction(Direction::Vertical)
		.margin(2)
		.constraints(constraints.iter().copied())
		.split(area)
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

	// question_height tests
	#[test]
	fn question_height_short_text_returns_minimum() {
		// "Hello" fits on one line at any reasonable width → border+1+border = 3
		assert_eq!(question_height("Hello", 80), 3);
	}

	#[test]
	fn question_height_wrapping_text_grows() {
		// At width 80, inner = 74 chars. 87-char string wraps to 2 lines → 4
		let long = "Git strategy? Push: commit to current branch. Branch: create release branch (for PRs).";
		assert_eq!(question_height(long, 80), 4);
	}

	#[test]
	fn question_height_zero_width_returns_minimum() {
		assert_eq!(question_height("anything", 0), 3);
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

	// render_yes_no_buttons tests
	#[test]
	fn render_yes_no_buttons_shows_labels() {
		let backend = TestBackend::new(80, 5);
		let mut terminal = Terminal::new(backend).unwrap();
		let content = render_to_string(&mut terminal, |frame| {
			render_yes_no_buttons(
				frame,
				frame.area(),
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
		assert!(content.contains("Yes"));
		assert!(content.contains("No"));
	}

	#[test]
	fn render_yes_no_buttons_empty_does_not_panic() {
		let mut terminal = make_terminal();
		terminal
			.draw(|frame| render_yes_no_buttons(frame, frame.area(), &[]))
			.unwrap();
	}

	// render_tabs tests
	#[test]
	fn render_tabs_shows_all_labels() {
		let mut terminal = make_terminal();
		let content = render_to_string(&mut terminal, |frame| {
			render_tabs(
				frame,
				frame.area(),
				&[
					("Managers", TabStatus::Current),
					("Git", TabStatus::Future),
					("GitHub", TabStatus::Future),
				],
			);
		});
		assert!(content.contains("Managers"));
		assert!(content.contains("Git"));
		assert!(content.contains("GitHub"));
	}

	#[test]
	fn render_tabs_empty_does_not_panic() {
		let mut terminal = make_terminal();
		terminal
			.draw(|frame| render_tabs(frame, frame.area(), &[]))
			.unwrap();
	}

	// wizard_layout tests
	#[test]
	fn wizard_layout_returns_correct_chunk_count() {
		let area = Rect::new(0, 0, 80, 24);
		let chunks = wizard_layout(
			area,
			&[
				Constraint::Length(3),
				Constraint::Length(3),
				Constraint::Min(1),
			],
		);
		assert_eq!(chunks.len(), 3);
	}

	#[test]
	fn wizard_layout_applies_margin() {
		let area = Rect::new(0, 0, 80, 24);
		let chunks = wizard_layout(area, &[Constraint::Min(0)]);
		// The single chunk should be inset by the 2-cell margin on each side
		assert!(chunks[0].x >= area.x + 2);
		assert!(chunks[0].y >= area.y + 2);
	}
}
