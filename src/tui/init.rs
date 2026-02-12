use std::io;

use crossterm::{
	ExecutableCommand,
	event::{self, Event, KeyCode, KeyEventKind},
	terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, Paragraph},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SetupChoice {
	Yes,
	No,
}

pub fn prompt_setup() -> anyhow::Result<SetupChoice> {
	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

	let mut selected = SetupChoice::Yes;

	loop {
		terminal.draw(|frame| ui(frame, selected))?;

		if let Event::Key(key) = event::read()?
			&& key.kind == KeyEventKind::Press
		{
			match key.code {
				KeyCode::Left
				| KeyCode::Right
				| KeyCode::Tab
				| KeyCode::Char('h')
				| KeyCode::Char('l') => {
					selected = match selected {
						SetupChoice::Yes => SetupChoice::No,
						SetupChoice::No => SetupChoice::Yes,
					};
				}
				KeyCode::Enter => break,
				KeyCode::Esc | KeyCode::Char('q') => {
					selected = SetupChoice::No;
					break;
				}
				_ => {}
			}
		}
	}

	disable_raw_mode()?;
	io::stdout().execute(LeaveAlternateScreen)?;

	Ok(selected)
}

fn ui(frame: &mut Frame, selected: SetupChoice) {
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
		.block(Block::default().borders(Borders::ALL).title("Setup"));
	frame.render_widget(title, chunks[0]);

	let question = Paragraph::new("No configuration found. Set up Chronicle for this repository?")
		.style(Style::default().fg(Color::Yellow))
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, chunks[1]);

	let yes_style = if selected == SetupChoice::Yes {
		Style::default()
			.fg(Color::Green)
			.add_modifier(Modifier::BOLD | Modifier::REVERSED)
	} else {
		Style::default().fg(Color::Gray)
	};
	let no_style = if selected == SetupChoice::No {
		Style::default()
			.fg(Color::Red)
			.add_modifier(Modifier::BOLD | Modifier::REVERSED)
	} else {
		Style::default().fg(Color::Gray)
	};

	let buttons = Line::from(vec![
		Span::raw("  "),
		Span::styled(" Yes ", yes_style),
		Span::raw("   "),
		Span::styled(" No ", no_style),
		Span::raw("  "),
	]);
	let button_para =
		Paragraph::new(buttons).block(Block::default().borders(Borders::ALL).title("Choose"));
	frame.render_widget(button_para, chunks[2]);

	let help = Paragraph::new("Use ←/→ or Tab to switch, Enter to confirm, Esc to cancel")
		.style(Style::default().fg(Color::DarkGray));
	frame.render_widget(help, chunks[3]);
}
