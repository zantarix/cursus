//! TUI for selecting the type of change (major, minor, patch).

use std::io;

use clap::ValueEnum;
use crossterm::{
	ExecutableCommand,
	event::{Event, KeyCode, KeyEventKind},
	terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, Paragraph},
};
use serde::{Deserialize, Serialize};

/// The type of semantic version change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
	/// A breaking change that increments the major version.
	Major,
	/// A backwards-compatible feature that increments the minor version.
	Minor,
	/// A backwards-compatible bug fix that increments the patch version.
	Patch,
}

impl ChangeType {
	/// Returns the next change type when cycling through options.
	fn next(self) -> Self {
		match self {
			Self::Major => Self::Minor,
			Self::Minor => Self::Patch,
			Self::Patch => Self::Major,
		}
	}

	/// Returns the previous change type when cycling through options.
	fn prev(self) -> Self {
		match self {
			Self::Major => Self::Patch,
			Self::Minor => Self::Major,
			Self::Patch => Self::Minor,
		}
	}
}

/// Options that can be pre-filled to skip interactive steps.
#[derive(Debug, Clone, Default)]
pub struct ChangeOptions {
	/// Pre-selected change type (skips selection screen).
	pub change_type: Option<ChangeType>,
}

/// Result of processing a key press in the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyResult {
	/// Continue with updated selection.
	Continue(ChangeType),
	/// Selection completed.
	Complete(ChangeType),
	/// Selection cancelled by user.
	Cancelled,
}

fn handle_key(selected: ChangeType, key: KeyCode) -> KeyResult {
	match key {
		KeyCode::Left | KeyCode::Char('h') => KeyResult::Continue(selected.prev()),
		KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => KeyResult::Continue(selected.next()),
		KeyCode::Enter => KeyResult::Complete(selected),
		KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
		_ => KeyResult::Continue(selected),
	}
}

/// Runs the interactive TUI for selecting a change type.
///
/// Displays a terminal UI that allows the user to select between
/// major, minor, or patch version changes.
///
/// # Returns
///
/// Returns `Ok(Some(ChangeType))` if the user completes selection,
/// or `Ok(None)` if the user cancels.
///
/// # Errors
///
/// Returns an error if terminal setup or I/O operations fail.
pub fn run(options: &ChangeOptions) -> anyhow::Result<Option<ChangeType>> {
	// If change type is pre-filled, return immediately
	if let Some(change_type) = options.change_type {
		return Ok(Some(change_type));
	}

	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

	let mut selected = ChangeType::Patch;

	let result = loop {
		terminal.draw(|frame| ui(frame, selected))?;

		if let Event::Key(key) = crossterm::event::read()?
			&& key.kind == KeyEventKind::Press
		{
			match handle_key(selected, key.code) {
				KeyResult::Continue(new_selected) => selected = new_selected,
				KeyResult::Complete(change_type) => break Some(change_type),
				KeyResult::Cancelled => break None,
			}
		}
	};

	disable_raw_mode()?;
	io::stdout().execute(LeaveAlternateScreen)?;

	Ok(result)
}

fn ui(frame: &mut Frame, selected: ChangeType) {
	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.margin(2)
		.constraints([
			Constraint::Length(3),
			Constraint::Length(3),
			Constraint::Length(3),
			Constraint::Min(1),
		])
		.split(frame.area());

	let title = Paragraph::new("Chronicle")
		.style(
			Style::default()
				.fg(Color::Cyan)
				.add_modifier(Modifier::BOLD),
		)
		.block(Block::default().borders(Borders::ALL).title("Change"));
	frame.render_widget(title, chunks[0]);

	let question = Paragraph::new("What type of change is this?")
		.style(Style::default().fg(Color::Yellow))
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, chunks[1]);

	let major_style = button_style(selected == ChangeType::Major);
	let minor_style = button_style(selected == ChangeType::Minor);
	let patch_style = button_style(selected == ChangeType::Patch);

	let buttons = Line::from(vec![
		Span::raw("  "),
		Span::styled(" Major ", major_style),
		Span::raw("   "),
		Span::styled(" Minor ", minor_style),
		Span::raw("   "),
		Span::styled(" Patch ", patch_style),
		Span::raw("  "),
	]);
	let button_para =
		Paragraph::new(buttons).block(Block::default().borders(Borders::ALL).title("Change Type"));
	frame.render_widget(button_para, chunks[2]);

	let help = Paragraph::new("Use ←/→ or Tab to switch, Enter to confirm, Esc to cancel")
		.style(Style::default().fg(Color::DarkGray));
	frame.render_widget(help, chunks[3]);
}

fn button_style(selected: bool) -> Style {
	if selected {
		Style::default()
			.fg(Color::Green)
			.add_modifier(Modifier::BOLD | Modifier::REVERSED)
	} else {
		Style::default().fg(Color::Gray)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// ChangeType navigation tests
	#[test]
	fn change_type_next_cycles_forward() {
		assert_eq!(ChangeType::Major.next(), ChangeType::Minor);
		assert_eq!(ChangeType::Minor.next(), ChangeType::Patch);
		assert_eq!(ChangeType::Patch.next(), ChangeType::Major);
	}

	#[test]
	fn change_type_prev_cycles_backward() {
		assert_eq!(ChangeType::Major.prev(), ChangeType::Patch);
		assert_eq!(ChangeType::Minor.prev(), ChangeType::Major);
		assert_eq!(ChangeType::Patch.prev(), ChangeType::Minor);
	}

	// handle_key tests
	#[test]
	fn left_moves_to_previous() {
		let result = handle_key(ChangeType::Minor, KeyCode::Left);
		assert_eq!(result, KeyResult::Continue(ChangeType::Major));
	}

	#[test]
	fn right_moves_to_next() {
		let result = handle_key(ChangeType::Minor, KeyCode::Right);
		assert_eq!(result, KeyResult::Continue(ChangeType::Patch));
	}

	#[test]
	fn tab_moves_to_next() {
		let result = handle_key(ChangeType::Major, KeyCode::Tab);
		assert_eq!(result, KeyResult::Continue(ChangeType::Minor));
	}

	#[test]
	fn h_moves_to_previous() {
		let result = handle_key(ChangeType::Patch, KeyCode::Char('h'));
		assert_eq!(result, KeyResult::Continue(ChangeType::Minor));
	}

	#[test]
	fn l_moves_to_next() {
		let result = handle_key(ChangeType::Major, KeyCode::Char('l'));
		assert_eq!(result, KeyResult::Continue(ChangeType::Minor));
	}

	#[test]
	fn enter_completes_with_selected() {
		let result = handle_key(ChangeType::Major, KeyCode::Enter);
		assert_eq!(result, KeyResult::Complete(ChangeType::Major));

		let result = handle_key(ChangeType::Minor, KeyCode::Enter);
		assert_eq!(result, KeyResult::Complete(ChangeType::Minor));

		let result = handle_key(ChangeType::Patch, KeyCode::Enter);
		assert_eq!(result, KeyResult::Complete(ChangeType::Patch));
	}

	#[test]
	fn esc_cancels() {
		let result = handle_key(ChangeType::Minor, KeyCode::Esc);
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn q_cancels() {
		let result = handle_key(ChangeType::Minor, KeyCode::Char('q'));
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn other_keys_do_nothing() {
		let result = handle_key(ChangeType::Minor, KeyCode::Char('x'));
		assert_eq!(result, KeyResult::Continue(ChangeType::Minor));

		let result = handle_key(ChangeType::Minor, KeyCode::Up);
		assert_eq!(result, KeyResult::Continue(ChangeType::Minor));
	}

	// UI rendering tests
	#[test]
	fn ui_renders_with_major_selected() {
		let backend = ratatui::backend::TestBackend::new(80, 20);
		let mut terminal = Terminal::new(backend).unwrap();
		terminal.draw(|frame| ui(frame, ChangeType::Major)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Major"));
		assert!(content.contains("Minor"));
		assert!(content.contains("Patch"));
	}

	#[test]
	fn ui_renders_with_minor_selected() {
		let backend = ratatui::backend::TestBackend::new(80, 20);
		let mut terminal = Terminal::new(backend).unwrap();
		terminal.draw(|frame| ui(frame, ChangeType::Minor)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("What type of change"));
	}

	#[test]
	fn ui_renders_with_patch_selected() {
		let backend = ratatui::backend::TestBackend::new(80, 20);
		let mut terminal = Terminal::new(backend).unwrap();
		terminal.draw(|frame| ui(frame, ChangeType::Patch)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Chronicle"));
	}

	fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
		let mut s = String::new();
		for y in 0..buffer.area.height {
			for x in 0..buffer.area.width {
				s.push(buffer[(x, y)].symbol().chars().next().unwrap_or(' '));
			}
			s.push('\n');
		}
		s
	}
}
