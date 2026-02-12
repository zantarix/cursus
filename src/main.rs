use std::io;

use clap::Parser;
use crossterm::{
	ExecutableCommand,
	event::{self, Event, KeyCode, KeyEventKind},
	terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, Paragraph},
};

#[derive(Parser, Debug)]
#[command(name = "chronicle")]
#[command(about = "A CLI application with interactive TUI support")]
struct Args {
	/// Run in interactive TUI mode
	#[arg(short, long)]
	interactive: bool,

	/// Name to greet (optional, will prompt in interactive mode)
	#[arg(short, long)]
	name: Option<String>,
}

fn main() -> anyhow::Result<()> {
	let args = Args::parse();

	if args.interactive || args.name.is_none() {
		run_tui(args.name)?;
	} else {
		println!("Hello, {}!", args.name.unwrap());
	}

	Ok(())
}

struct App {
	input: String,
	submitted: bool,
}

impl App {
	fn new(initial: Option<String>) -> Self {
		Self {
			input: initial.unwrap_or_default(),
			submitted: false,
		}
	}
}

fn run_tui(initial_name: Option<String>) -> anyhow::Result<()> {
	// Setup terminal
	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

	let mut app = App::new(initial_name);

	// Main loop
	loop {
		terminal.draw(|frame| ui(frame, &app))?;

		if let Event::Key(key) = event::read()? {
			if key.kind == KeyEventKind::Press {
				match key.code {
					KeyCode::Esc => break,
					KeyCode::Enter => {
						app.submitted = true;
						break;
					}
					KeyCode::Backspace => {
						app.input.pop();
					}
					KeyCode::Char(c) => {
						app.input.push(c);
					}
					_ => {}
				}
			}
		}
	}

	// Restore terminal
	disable_raw_mode()?;
	io::stdout().execute(LeaveAlternateScreen)?;

	if app.submitted && !app.input.is_empty() {
		println!("Hello, {}!", app.input);
	}

	Ok(())
}

fn ui(frame: &mut Frame, app: &App) {
	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.margin(2)
		.constraints([
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
		.block(Block::default().borders(Borders::ALL).title("Welcome"));
	frame.render_widget(title, chunks[0]);

	let input = Paragraph::new(app.input.as_str())
		.style(Style::default().fg(Color::Yellow))
		.block(
			Block::default()
				.borders(Borders::ALL)
				.title("Enter your name"),
		);
	frame.render_widget(input, chunks[1]);

	let help = Paragraph::new("Press Enter to submit, Esc to cancel")
		.style(Style::default().fg(Color::Gray));
	frame.render_widget(help, chunks[2]);

	// Show cursor at end of input
	frame.set_cursor_position((chunks[1].x + app.input.len() as u16 + 1, chunks[1].y + 1));
}
