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
pub struct ButtonDef {
	/// The text label displayed inside the button.
	pub label: String,
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
		.line_count(inner);
	let lines = u16::try_from(lines).unwrap_or(u16::MAX);
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
	let Ok(n) = u16::try_from(tabs.len()) else {
		return;
	};
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

/// Renders a row of buttons as equal-width blocks with one blank line of
/// padding above and below each label.
///
/// Each button occupies an equal share of `area`. The selected button's style
/// fills the entire button area. Unselected buttons have a dark grey background.
pub fn render_buttons(frame: &mut Frame, area: Rect, buttons: &[ButtonDef]) {
	if buttons.is_empty() {
		return;
	}
	let n = buttons.len();
	let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Fill(1)).collect();
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
		let content = Text::from(vec![
			Line::from(""),
			Line::from(btn.label.as_str()),
			Line::from(""),
		]);
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

/// RAII guard that reverses terminal setup steps in the correct order on drop.
///
/// Each field is set to `true` immediately after the corresponding setup step
/// succeeds, so a `?`-propagated error at any point still cleans up whatever
/// was already done.
struct TerminalGuard {
	raw_mode: bool,
	alt_screen: bool,
	mouse_capture: bool,
	kbd_enhancement: bool,
}

// `TerminalGuard` setup methods and `Drop` flush directly to the real `stdout`
// via crossterm — there is no in-process seam available, so they are excluded
// from coverage in line with the convention used in `cursus-bin/src/main.rs`
// for true IO boundaries.
impl TerminalGuard {
	#[cfg_attr(coverage_nightly, coverage(off))]
	fn new() -> Self {
		Self {
			raw_mode: false,
			alt_screen: false,
			mouse_capture: false,
			kbd_enhancement: false,
		}
	}

	#[cfg_attr(coverage_nightly, coverage(off))]
	fn enable_raw_mode(&mut self) -> io::Result<()> {
		enable_raw_mode()?;
		self.raw_mode = true;
		Ok(())
	}

	#[cfg_attr(coverage_nightly, coverage(off))]
	fn enter_alternate_screen(&mut self) -> io::Result<()> {
		io::stdout().execute(EnterAlternateScreen)?;
		self.alt_screen = true;
		Ok(())
	}

	#[cfg_attr(coverage_nightly, coverage(off))]
	fn enable_mouse_capture(&mut self) -> io::Result<()> {
		io::stdout().execute(EnableMouseCapture)?;
		self.mouse_capture = true;
		Ok(())
	}

	#[cfg_attr(coverage_nightly, coverage(off))]
	fn push_keyboard_enhancement(&mut self) -> io::Result<()> {
		io::stdout().execute(PushKeyboardEnhancementFlags(
			KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
		))?;
		self.kbd_enhancement = true;
		Ok(())
	}
}

impl Drop for TerminalGuard {
	#[cfg_attr(coverage_nightly, coverage(off))]
	fn drop(&mut self) {
		if self.kbd_enhancement {
			io::stdout().execute(PopKeyboardEnhancementFlags).ok();
		}
		if self.mouse_capture {
			io::stdout().execute(DisableMouseCapture).ok();
		}
		if self.alt_screen {
			io::stdout().execute(LeaveAlternateScreen).ok();
		}
		if self.raw_mode {
			disable_raw_mode().ok();
		}
	}
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
// Drives the real terminal: owns `stdout`, blocks on `crossterm::event::read()`,
// and cannot be exercised without a live tty. Excluded from coverage in line with
// the `cursus-bin/src/main.rs` convention for true IO entrypoints.
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn run_tui<S, T, DrawFn, HandleFn>(
	mut state: S,
	mut draw_fn: DrawFn,
	mut handle_fn: HandleFn,
) -> anyhow::Result<Option<T>>
where
	DrawFn: FnMut(&mut Frame, &S),
	HandleFn: FnMut(S, Event, Rect) -> anyhow::Result<KeyResult<S, T>>,
{
	let mut guard = TerminalGuard::new();
	guard.enable_raw_mode()?;
	guard.enter_alternate_screen()?;
	guard.enable_mouse_capture()?;
	// Enable DISAMBIGUATE_ESCAPE_CODES on terminals that support the kitty
	// keyboard protocol so that Shift+Enter is distinguishable from Enter.
	// Falls back gracefully on terminals that don't support it; those users
	// can use Alt+Enter instead.
	if supports_keyboard_enhancement().unwrap_or(false) {
		guard.push_keyboard_enhancement()?;
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

	// `guard` drops here, restoring the terminal in all cases.
	result
}

/// Returns the index of the button clicked at `(col, row)`, or `None` for a miss.
///
/// Uses the standard wizard layout (2-cell margin, question block, then
/// a 3-row button row) to locate the button area and splits it into
/// `n_buttons` equal-width cells with 1-cell spacing, matching
/// [`render_buttons`]. `question` must be the same text passed to
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
	let constraints: Vec<Constraint> = (0..n_buttons).map(|_| Constraint::Fill(1)).collect();
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
