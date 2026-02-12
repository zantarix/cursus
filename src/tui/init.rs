use std::io;
use std::path::Path;

use crossterm::{
	ExecutableCommand,
	event::{self, Event, KeyCode, KeyEventKind},
	terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, Paragraph},
};

use crate::config::{Config, PackageManager};

enum Screen {
	Confirm(bool),
	SelectPackageManager(PackageManager),
}

fn detect_package_manager(git_root: &Path) -> PackageManager {
	// Prefer NPM as tie breaker, so check for it first
	if git_root.join("package.json").exists() {
		PackageManager::Npm
	} else if git_root.join("Cargo.toml").exists() {
		PackageManager::Cargo
	} else {
		PackageManager::Npm
	}
}

/// Runs the interactive TUI setup wizard for Chronicle configuration.
///
/// Displays a terminal UI that guides the user through selecting a package
/// manager for their project. Auto-detects the likely package manager based
/// on the presence of `package.json` or `Cargo.toml`.
///
/// # Returns
///
/// Returns `Ok(Some(Config))` if the user completes setup, or `Ok(None)` if
/// the user cancels.
///
/// # Errors
///
/// Returns an error if terminal setup or I/O operations fail.
pub fn setup(git_root: &Path) -> anyhow::Result<Option<Config>> {
	let detected = detect_package_manager(git_root);

	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

	let mut screen = Screen::Confirm(true);

	let result = loop {
		terminal.draw(|frame| ui(frame, &screen))?;

		if let Event::Key(key) = event::read()?
			&& key.kind == KeyEventKind::Press
		{
			match &mut screen {
				Screen::Confirm(yes) => match key.code {
					KeyCode::Left
					| KeyCode::Right
					| KeyCode::Tab
					| KeyCode::Char('h')
					| KeyCode::Char('l') => {
						*yes = !*yes;
					}
					KeyCode::Enter => {
						if *yes {
							screen = Screen::SelectPackageManager(detected);
						} else {
							break None;
						}
					}
					KeyCode::Esc | KeyCode::Char('q') => break None,
					_ => {}
				},
				Screen::SelectPackageManager(selected) => match key.code {
					KeyCode::Left
					| KeyCode::Right
					| KeyCode::Tab
					| KeyCode::Char('h')
					| KeyCode::Char('l') => {
						*selected = match selected {
							PackageManager::Cargo => PackageManager::Npm,
							PackageManager::Npm => PackageManager::Cargo,
						};
					}
					KeyCode::Enter => {
						break Some(Config {
							package_manager: *selected,
						});
					}
					KeyCode::Esc | KeyCode::Char('q') => break None,
					_ => {}
				},
			}
		}
	};

	disable_raw_mode()?;
	io::stdout().execute(LeaveAlternateScreen)?;

	Ok(result)
}

fn ui(frame: &mut Frame, screen: &Screen) {
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

	match screen {
		Screen::Confirm(yes) => render_confirm(frame, &chunks, *yes),
		Screen::SelectPackageManager(selected) => render_package_manager(frame, &chunks, *selected),
	}

	let help = Paragraph::new("Use ←/→ or Tab to switch, Enter to confirm, Esc to cancel")
		.style(Style::default().fg(Color::DarkGray));
	frame.render_widget(help, chunks[3]);
}

fn render_confirm(frame: &mut Frame, chunks: &[Rect], yes: bool) {
	let question = Paragraph::new("No configuration found. Set up Chronicle for this repository?")
		.style(Style::default().fg(Color::Yellow))
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, chunks[1]);

	let yes_style = if yes {
		Style::default()
			.fg(Color::Green)
			.add_modifier(Modifier::BOLD | Modifier::REVERSED)
	} else {
		Style::default().fg(Color::Gray)
	};
	let no_style = if !yes {
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
}

fn render_package_manager(frame: &mut Frame, chunks: &[Rect], selected: PackageManager) {
	let question = Paragraph::new("Which package manager does this project use?")
		.style(Style::default().fg(Color::Yellow))
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, chunks[1]);

	let cargo_style = if selected == PackageManager::Cargo {
		Style::default()
			.fg(Color::Green)
			.add_modifier(Modifier::BOLD | Modifier::REVERSED)
	} else {
		Style::default().fg(Color::Gray)
	};
	let npm_style = if selected == PackageManager::Npm {
		Style::default()
			.fg(Color::Green)
			.add_modifier(Modifier::BOLD | Modifier::REVERSED)
	} else {
		Style::default().fg(Color::Gray)
	};

	let buttons = Line::from(vec![
		Span::raw("  "),
		Span::styled(" Cargo ", cargo_style),
		Span::raw("   "),
		Span::styled(" NPM ", npm_style),
		Span::raw("  "),
	]);
	let button_para = Paragraph::new(buttons).block(
		Block::default()
			.borders(Borders::ALL)
			.title("Package Manager"),
	);
	frame.render_widget(button_para, chunks[2]);
}
