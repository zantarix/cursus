//! Shared TUI widget components for rendering and terminal lifecycle management.
//!
//! This module provides reusable rendering helpers and a generic event-loop
//! wrapper used by the init and change TUI wizards.

use std::io;
use std::rc::Rc;

use crossterm::{
	ExecutableCommand,
	event::{
		DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, KeyboardEnhancementFlags,
		MouseButton, MouseEventKind, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
	},
	terminal::{
		EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
		supports_keyboard_enhancement,
	},
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

/// Computes the height a paragraph needs to display `text` without clipping.
///
/// Accounts for the 2-cell wizard margin on each side. `border` is applied to
/// **both axes**: it is subtracted from the usable text width
/// (`area_width - 4 - border`) and added to the returned height. This matches
/// `Borders::ALL`, which consumes exactly 1 cell per side on both axes (so
/// `border = 2`). For borderless text pass `border = 0`. Do not pass other
/// values — a partial border (e.g. top-only) would give an incorrect width.
/// Returns at least `1 + border`.
pub fn paragraph_height(text: &str, area_width: u16, border: u16) -> u16 {
	let inner = area_width.saturating_sub(4 + border);
	let lines = Paragraph::new(text)
		.wrap(Wrap { trim: false })
		.line_count(inner) as u16;
	(lines + border).max(1 + border)
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
	HandleFn: FnMut(S, Event, Rect) -> anyhow::Result<KeyResult<S, T>>,
{
	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	io::stdout().execute(EnableMouseCapture)?;
	// Enable DISAMBIGUATE_ESCAPE_CODES on terminals that support the kitty
	// keyboard protocol so that Shift+Enter is distinguishable from Enter.
	// Falls back gracefully on terminals that don't support it; those users
	// can use Alt+Enter instead.
	let kbd_enhancement = supports_keyboard_enhancement().unwrap_or(false);
	if kbd_enhancement {
		io::stdout().execute(PushKeyboardEnhancementFlags(
			KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
		))?;
	}
	let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

	let result: anyhow::Result<Option<T>> = loop {
		let frame_area = match terminal.draw(|frame| draw_fn(frame, &state)) {
			Err(e) => break Err(e.into()),
			Ok(completed) => completed.area,
		};
		let event = match crossterm::event::read() {
			Err(e) => break Err(e.into()),
			Ok(e) => e,
		};
		let forward = match &event {
			Event::Key(key) if key.kind == KeyEventKind::Press => true,
			Event::Mouse(me) => matches!(me.kind, MouseEventKind::Down(MouseButton::Left)),
			_ => false,
		};
		if forward {
			match handle_fn(state, event, frame_area) {
				Err(e) => break Err(e),
				Ok(KeyResult::Continue(new_state)) => state = new_state,
				Ok(KeyResult::Complete(value)) => break Ok(Some(value)),
				Ok(KeyResult::Cancelled) => break Ok(None),
			}
		}
	};

	// Always restore the terminal, even if the loop errored.
	// Cleanup errors are suppressed to preserve the primary error.
	if kbd_enhancement {
		io::stdout().execute(PopKeyboardEnhancementFlags).ok();
	}
	disable_raw_mode().ok();
	io::stdout().execute(DisableMouseCapture).ok();
	io::stdout().execute(LeaveAlternateScreen).ok();

	result
}

/// Returns the index of the button clicked at `(col, row)`, or `None` for a miss.
///
/// Uses the standard wizard layout (2-cell margin, question block, then
/// a 3-row button row) to locate the button area and splits it into
/// `n_buttons` equal-width cells with 1-cell spacing, matching
/// [`render_yes_no_buttons`]. `question` must be the same text passed to
/// `render_question` on the same screen so that the height computation
/// matches.
pub fn button_click_index(
	content_area: Rect,
	question: &str,
	n_buttons: u16,
	col: u16,
	row: u16,
) -> Option<usize> {
	if n_buttons == 0 {
		return None;
	}
	let q_height = paragraph_height(question, content_area.width, 2);
	let chunks = wizard_layout(
		content_area,
		&[
			Constraint::Length(q_height),
			Constraint::Length(3),
			Constraint::Length(1),
			Constraint::Min(1),
		],
	);
	let buttons_area = chunks[1];
	if row < buttons_area.y || row >= buttons_area.y + buttons_area.height {
		return None;
	}
	if col < buttons_area.x || col >= buttons_area.x + buttons_area.width {
		return None;
	}
	let constraints: Vec<Constraint> = (0..n_buttons)
		.map(|_| Constraint::Percentage(100 / n_buttons))
		.collect();
	let cells = Layout::horizontal(constraints)
		.spacing(1)
		.split(buttons_area);
	for (i, cell) in cells.iter().enumerate() {
		if col >= cell.x && col < cell.x + cell.width {
			return Some(i);
		}
	}
	None
}

#[cfg(test)]
mod tests {
	use ratatui::backend::TestBackend;

	use super::super::test_utils::render_to_string;
	use super::*;

	fn make_terminal() -> Terminal<TestBackend> {
		Terminal::new(TestBackend::new(80, 5)).unwrap()
	}

	// paragraph_height tests
	#[test]
	fn paragraph_height_short_text_with_border_returns_minimum() {
		// "Hello" fits on one line at any reasonable width → border+1+border = 3
		assert_eq!(paragraph_height("Hello", 80, 2), 3);
	}

	#[test]
	fn paragraph_height_wrapping_text_with_border_grows() {
		// At width 80, inner = 74 chars. 87-char string wraps to 2 lines → 4
		let long = "Git strategy? Push: commit to current branch. Branch: create release branch (for PRs).";
		assert_eq!(paragraph_height(long, 80, 2), 4);
	}

	#[test]
	fn paragraph_height_zero_width_with_border_returns_minimum() {
		assert_eq!(paragraph_height("anything", 0, 2), 3);
	}

	#[test]
	fn paragraph_height_no_border_short_text_returns_one() {
		assert_eq!(paragraph_height("help", 80, 0), 1);
	}

	#[test]
	fn paragraph_height_no_border_zero_width_returns_one() {
		assert_eq!(paragraph_height("help", 0, 0), 1);
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

	// button_click_index tests
	#[test]
	fn button_click_index_hits_first_button() {
		let area = Rect::new(0, 0, 80, 24);
		// question height=3, buttons area: y=5..8, x=2..78
		// First button occupies roughly x=2..39
		let idx = button_click_index(area, "test?", 2, 10, 6);
		assert_eq!(idx, Some(0));
	}

	#[test]
	fn button_click_index_hits_second_button() {
		let area = Rect::new(0, 0, 80, 24);
		let idx = button_click_index(area, "test?", 2, 65, 6);
		assert_eq!(idx, Some(1));
	}

	#[test]
	fn button_click_index_misses_above_buttons() {
		let area = Rect::new(0, 0, 80, 24);
		// Row 2 is inside the question block, not the button area
		let idx = button_click_index(area, "test?", 2, 10, 2);
		assert_eq!(idx, None);
	}

	#[test]
	fn button_click_index_misses_below_buttons() {
		let area = Rect::new(0, 0, 80, 24);
		let idx = button_click_index(area, "test?", 2, 10, 15);
		assert_eq!(idx, None);
	}

	#[test]
	fn button_click_index_zero_buttons_returns_none() {
		let area = Rect::new(0, 0, 80, 24);
		assert_eq!(button_click_index(area, "test?", 0, 10, 6), None);
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
