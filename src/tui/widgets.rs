//! Shared TUI widget components for rendering and terminal lifecycle management.
//!
//! This module provides reusable rendering helpers and a generic event-loop
//! wrapper used by the init and change TUI wizards.

use std::io;
use std::rc::Rc;

use crossterm::{
    ExecutableCommand,
    event::{Event, KeyCode, KeyEventKind},
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
    if selected {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(Color::Gray)
    }
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
/// Displays `text` in the given `color` inside a bordered, borderless-title
/// block at `area`.
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
    HandleFn: FnMut(S, KeyCode) -> KeyResult<S, T>,
{
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let result = loop {
        terminal.draw(|frame| draw_fn(frame, &state))?;

        if let Event::Key(key) = crossterm::event::read()?
            && key.kind == KeyEventKind::Press
        {
            match handle_fn(state, key.code) {
                KeyResult::Continue(new_state) => state = new_state,
                KeyResult::Complete(value) => break Some(value),
                KeyResult::Cancelled => break None,
            }
        }
    };

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    Ok(result)
}
