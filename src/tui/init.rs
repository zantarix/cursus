use std::io;
use std::path::Path;

use crossterm::{
	ExecutableCommand,
	event::{Event, KeyCode, KeyEventKind},
	terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, Paragraph},
};

use crate::config::{Config, PackageManager};

/// Options that can be pre-filled to skip interactive steps.
#[derive(Debug, Clone, Default)]
pub struct InitOptions {
	/// Pre-selected package manager (skips package manager selection screen).
	pub package_manager: Option<PackageManager>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
	Confirm(bool),
	SelectPackageManager(PackageManager),
}

/// Result of processing a key press in the setup wizard.
#[derive(Debug, Clone, PartialEq, Eq)]
enum KeyResult {
	/// Continue with updated screen state.
	Continue(Screen),
	/// Setup completed with a configuration.
	Complete(Config),
	/// Setup cancelled by user.
	Cancelled,
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

fn handle_key(
	screen: Screen,
	key: KeyCode,
	detected: PackageManager,
	options: &InitOptions,
) -> KeyResult {
	match screen {
		Screen::Confirm(yes) => match key {
			KeyCode::Left
			| KeyCode::Right
			| KeyCode::Tab
			| KeyCode::Char('h')
			| KeyCode::Char('l') => KeyResult::Continue(Screen::Confirm(!yes)),
			KeyCode::Enter => {
				if yes {
					// If package manager is pre-filled, skip to completion
					if let Some(pm) = options.package_manager {
						KeyResult::Complete(Config::with_package_manager(pm))
					} else {
						KeyResult::Continue(Screen::SelectPackageManager(detected))
					}
				} else {
					KeyResult::Cancelled
				}
			}
			KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
			_ => KeyResult::Continue(screen),
		},
		Screen::SelectPackageManager(selected) => match key {
			KeyCode::Left
			| KeyCode::Right
			| KeyCode::Tab
			| KeyCode::Char('h')
			| KeyCode::Char('l') => {
				let new_selected = match selected {
					PackageManager::Cargo => PackageManager::Npm,
					PackageManager::Npm => PackageManager::Cargo,
				};
				KeyResult::Continue(Screen::SelectPackageManager(new_selected))
			}
			KeyCode::Enter => KeyResult::Complete(Config::with_package_manager(selected)),
			KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
			_ => KeyResult::Continue(screen),
		},
	}
}

/// Runs the interactive TUI setup wizard for Chronicle configuration.
///
/// Displays a terminal UI that guides the user through selecting a package
/// manager for their project. Auto-detects the likely package manager based
/// on the presence of `package.json` or `Cargo.toml`.
///
/// Pre-filled options in `InitOptions` will skip their corresponding steps.
///
/// # Returns
///
/// Returns `Ok(Some(Config))` if the user completes setup, or `Ok(None)` if
/// the user cancels.
///
/// # Errors
///
/// Returns an error if terminal setup or I/O operations fail.
pub fn run(git_root: &Path, options: &InitOptions) -> anyhow::Result<Option<Config>> {
	let detected = detect_package_manager(git_root);

	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

	let mut screen = Screen::Confirm(true);

	let result = loop {
		terminal.draw(|frame| ui(frame, &screen))?;

		if let Event::Key(key) = crossterm::event::read()?
			&& key.kind == KeyEventKind::Press
		{
			match handle_key(screen, key.code, detected, options) {
				KeyResult::Continue(new_screen) => screen = new_screen,
				KeyResult::Complete(config) => break Some(config),
				KeyResult::Cancelled => break None,
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
			Constraint::Min(1),
		])
		.split(frame.area());

	match screen {
		Screen::Confirm(yes) => render_confirm(frame, &chunks, *yes),
		Screen::SelectPackageManager(selected) => render_package_manager(frame, &chunks, *selected),
	}

	let help = Paragraph::new("Use ←/→ or Tab to switch, Enter to confirm, Esc to cancel")
		.style(Style::default().fg(Color::DarkGray));
	frame.render_widget(help, chunks[2]);
}

fn render_confirm(frame: &mut Frame, chunks: &[Rect], yes: bool) {
	let question = Paragraph::new("No configuration found. Set up Chronicle for this repository?")
		.style(Style::default().fg(Color::Yellow))
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, chunks[0]);

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
	frame.render_widget(button_para, chunks[1]);
}

fn render_package_manager(frame: &mut Frame, chunks: &[Rect], selected: PackageManager) {
	let question = Paragraph::new("Which package manager does this project use?")
		.style(Style::default().fg(Color::Yellow))
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, chunks[0]);

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
	frame.render_widget(button_para, chunks[1]);
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	/// Helper to call handle_key with no pre-filled options (default behavior).
	fn handle_key_default(screen: Screen, key: KeyCode, detected: PackageManager) -> KeyResult {
		handle_key(screen, key, detected, &InitOptions::default())
	}

	fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	// detect_package_manager tests
	#[test]
	fn detect_package_manager_defaults_to_npm() {
		let dir = temp_dir();
		let result = detect_package_manager(dir.path());
		assert_eq!(result, PackageManager::Npm);
	}

	#[test]
	fn detect_package_manager_detects_npm() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("package.json"), "{}").unwrap();
		let result = detect_package_manager(dir.path());
		assert_eq!(result, PackageManager::Npm);
	}

	#[test]
	fn detect_package_manager_detects_cargo() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		let result = detect_package_manager(dir.path());
		assert_eq!(result, PackageManager::Cargo);
	}

	#[test]
	fn detect_package_manager_prefers_npm_when_both_exist() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("package.json"), "{}").unwrap();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		let result = detect_package_manager(dir.path());
		assert_eq!(result, PackageManager::Npm);
	}

	// handle_key tests - Confirm screen
	#[test]
	fn confirm_left_toggles_selection() {
		let result = handle_key_default(Screen::Confirm(true), KeyCode::Left, PackageManager::Npm);
		assert_eq!(result, KeyResult::Continue(Screen::Confirm(false)));

		let result = handle_key_default(Screen::Confirm(false), KeyCode::Left, PackageManager::Npm);
		assert_eq!(result, KeyResult::Continue(Screen::Confirm(true)));
	}

	#[test]
	fn confirm_right_toggles_selection() {
		let result = handle_key_default(Screen::Confirm(true), KeyCode::Right, PackageManager::Npm);
		assert_eq!(result, KeyResult::Continue(Screen::Confirm(false)));
	}

	#[test]
	fn confirm_tab_toggles_selection() {
		let result = handle_key_default(Screen::Confirm(true), KeyCode::Tab, PackageManager::Npm);
		assert_eq!(result, KeyResult::Continue(Screen::Confirm(false)));
	}

	#[test]
	fn confirm_h_toggles_selection() {
		let result = handle_key_default(
			Screen::Confirm(true),
			KeyCode::Char('h'),
			PackageManager::Npm,
		);
		assert_eq!(result, KeyResult::Continue(Screen::Confirm(false)));
	}

	#[test]
	fn confirm_l_toggles_selection() {
		let result = handle_key_default(
			Screen::Confirm(true),
			KeyCode::Char('l'),
			PackageManager::Npm,
		);
		assert_eq!(result, KeyResult::Continue(Screen::Confirm(false)));
	}

	#[test]
	fn confirm_enter_yes_advances_to_package_manager() {
		let result =
			handle_key_default(Screen::Confirm(true), KeyCode::Enter, PackageManager::Cargo);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectPackageManager(PackageManager::Cargo))
		);
	}

	#[test]
	fn confirm_enter_yes_with_prefilled_pm_completes_immediately() {
		let options = InitOptions {
			package_manager: Some(PackageManager::Npm),
		};
		let result = handle_key(
			Screen::Confirm(true),
			KeyCode::Enter,
			PackageManager::Cargo, // detected doesn't matter when pre-filled
			&options,
		);
		assert_eq!(
			result,
			KeyResult::Complete(Config::with_package_manager(PackageManager::Npm))
		);
	}

	#[test]
	fn confirm_enter_yes_with_prefilled_cargo_completes_immediately() {
		let options = InitOptions {
			package_manager: Some(PackageManager::Cargo),
		};
		let result = handle_key(
			Screen::Confirm(true),
			KeyCode::Enter,
			PackageManager::Npm,
			&options,
		);
		assert_eq!(
			result,
			KeyResult::Complete(Config::with_package_manager(PackageManager::Cargo))
		);
	}

	#[test]
	fn confirm_enter_no_cancels() {
		let result =
			handle_key_default(Screen::Confirm(false), KeyCode::Enter, PackageManager::Npm);
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn confirm_esc_cancels() {
		let result = handle_key_default(Screen::Confirm(true), KeyCode::Esc, PackageManager::Npm);
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn confirm_q_cancels() {
		let result = handle_key_default(
			Screen::Confirm(true),
			KeyCode::Char('q'),
			PackageManager::Npm,
		);
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn confirm_other_keys_do_nothing() {
		let result = handle_key_default(
			Screen::Confirm(true),
			KeyCode::Char('x'),
			PackageManager::Npm,
		);
		assert_eq!(result, KeyResult::Continue(Screen::Confirm(true)));

		let result = handle_key_default(Screen::Confirm(true), KeyCode::Up, PackageManager::Npm);
		assert_eq!(result, KeyResult::Continue(Screen::Confirm(true)));
	}

	// handle_key tests - SelectPackageManager screen
	#[test]
	fn select_pm_left_toggles_selection() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Npm),
			KeyCode::Left,
			PackageManager::Npm,
		);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectPackageManager(PackageManager::Cargo))
		);

		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Cargo),
			KeyCode::Left,
			PackageManager::Npm,
		);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectPackageManager(PackageManager::Npm))
		);
	}

	#[test]
	fn select_pm_right_toggles_selection() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Npm),
			KeyCode::Right,
			PackageManager::Npm,
		);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectPackageManager(PackageManager::Cargo))
		);
	}

	#[test]
	fn select_pm_tab_toggles_selection() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Cargo),
			KeyCode::Tab,
			PackageManager::Npm,
		);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectPackageManager(PackageManager::Npm))
		);
	}

	#[test]
	fn select_pm_h_toggles_selection() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Npm),
			KeyCode::Char('h'),
			PackageManager::Npm,
		);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectPackageManager(PackageManager::Cargo))
		);
	}

	#[test]
	fn select_pm_l_toggles_selection() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Cargo),
			KeyCode::Char('l'),
			PackageManager::Npm,
		);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectPackageManager(PackageManager::Npm))
		);
	}

	#[test]
	fn select_pm_enter_completes_with_npm() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Npm),
			KeyCode::Enter,
			PackageManager::Cargo,
		);
		assert_eq!(
			result,
			KeyResult::Complete(Config::with_package_manager(PackageManager::Npm))
		);
	}

	#[test]
	fn select_pm_enter_completes_with_cargo() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Cargo),
			KeyCode::Enter,
			PackageManager::Npm,
		);
		assert_eq!(
			result,
			KeyResult::Complete(Config::with_package_manager(PackageManager::Cargo))
		);
	}

	#[test]
	fn select_pm_esc_cancels() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Npm),
			KeyCode::Esc,
			PackageManager::Npm,
		);
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn select_pm_q_cancels() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Npm),
			KeyCode::Char('q'),
			PackageManager::Npm,
		);
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn select_pm_other_keys_do_nothing() {
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Npm),
			KeyCode::Char('x'),
			PackageManager::Npm,
		);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectPackageManager(PackageManager::Npm))
		);
	}

	// Full workflow tests
	#[test]
	fn workflow_confirm_yes_select_npm() {
		// Start at confirm, select yes
		let result = handle_key_default(Screen::Confirm(true), KeyCode::Enter, PackageManager::Npm);
		let Screen::SelectPackageManager(pm) = (match result {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		}) else {
			panic!("Expected SelectPackageManager")
		};
		assert_eq!(pm, PackageManager::Npm);

		// Select npm and confirm
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Npm),
			KeyCode::Enter,
			PackageManager::Npm,
		);
		assert_eq!(
			result,
			KeyResult::Complete(Config::with_package_manager(PackageManager::Npm))
		);
	}

	#[test]
	fn workflow_confirm_yes_toggle_select_cargo() {
		// Start at confirm with detected cargo
		let result =
			handle_key_default(Screen::Confirm(true), KeyCode::Enter, PackageManager::Cargo);
		let Screen::SelectPackageManager(pm) = (match result {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		}) else {
			panic!("Expected SelectPackageManager")
		};
		assert_eq!(pm, PackageManager::Cargo);

		// Toggle to npm then back to cargo
		let result = handle_key_default(
			Screen::SelectPackageManager(PackageManager::Cargo),
			KeyCode::Tab,
			PackageManager::Cargo,
		);
		let screen = match result {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		};

		let result = handle_key_default(screen, KeyCode::Tab, PackageManager::Cargo);
		let screen = match result {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		};

		// Confirm cargo
		let result = handle_key_default(screen, KeyCode::Enter, PackageManager::Cargo);
		assert_eq!(
			result,
			KeyResult::Complete(Config::with_package_manager(PackageManager::Cargo))
		);
	}

	// UI rendering tests using TestBackend
	fn create_test_terminal() -> Terminal<ratatui::backend::TestBackend> {
		let backend = ratatui::backend::TestBackend::new(80, 24);
		Terminal::new(backend).unwrap()
	}

	#[test]
	fn ui_renders_confirm_screen_yes_selected() {
		let mut terminal = create_test_terminal();
		terminal
			.draw(|frame| ui(frame, &Screen::Confirm(true)))
			.unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Choose"));
		assert!(content.contains("Yes"));
		assert!(content.contains("No"));
	}

	#[test]
	fn ui_renders_confirm_screen_no_selected() {
		let mut terminal = create_test_terminal();
		terminal
			.draw(|frame| ui(frame, &Screen::Confirm(false)))
			.unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Choose"));
		assert!(content.contains("Yes"));
		assert!(content.contains("No"));
	}

	#[test]
	fn ui_renders_package_manager_screen_npm_selected() {
		let mut terminal = create_test_terminal();
		terminal
			.draw(|frame| ui(frame, &Screen::SelectPackageManager(PackageManager::Npm)))
			.unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Package Manager"));
		assert!(content.contains("Cargo"));
		assert!(content.contains("NPM"));
	}

	#[test]
	fn ui_renders_package_manager_screen_cargo_selected() {
		let mut terminal = create_test_terminal();
		terminal
			.draw(|frame| ui(frame, &Screen::SelectPackageManager(PackageManager::Cargo)))
			.unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Package Manager"));
		assert!(content.contains("Cargo"));
		assert!(content.contains("NPM"));
	}

	fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
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
}
