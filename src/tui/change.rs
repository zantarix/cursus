//! TUI for selecting projects and the type of change (major, minor, patch).

use std::io;

use clap::ValueEnum;
use crossterm::{
	ExecutableCommand,
	event::{Event, KeyCode, KeyEventKind},
	terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, List, ListItem, Paragraph},
};
use serde::{Deserialize, Serialize};

use crate::package_manager::Project;

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

/// The result of a completed change selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeResult {
	/// The projects selected by the user.
	pub projects: Vec<Project>,
	/// The type of change selected by the user.
	pub change_type: ChangeType,
}

/// Options that can be pre-filled to skip interactive steps.
#[derive(Debug, Clone, Default)]
pub struct ChangeOptions {
	/// Pre-selected change type (skips selection screen).
	pub change_type: Option<ChangeType>,
	/// Pre-selected project indices (skips project selection screen).
	pub projects: Option<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
	SelectProjects {
		selected: Vec<bool>,
		cursor: usize,
		error: bool,
	},
	SelectChangeType(ChangeType),
}

/// Result of processing a key press in the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
enum KeyResult {
	/// Continue with updated screen state.
	Continue(Screen),
	/// Selection completed with selected project indices and change type.
	Complete(Vec<usize>, ChangeType),
	/// Selection cancelled by user.
	Cancelled,
}

fn handle_key(screen: &Screen, key: KeyCode) -> KeyResult {
	match screen {
		Screen::SelectProjects {
			selected, cursor, ..
		} => handle_key_select_projects(selected, *cursor, key),
		Screen::SelectChangeType(change_type) => handle_key_change_type(*change_type, key),
	}
}

fn handle_key_select_projects(selected: &[bool], cursor: usize, key: KeyCode) -> KeyResult {
	let len = selected.len();
	match key {
		KeyCode::Up | KeyCode::Char('k') => {
			let new_cursor = if cursor == 0 { len - 1 } else { cursor - 1 };
			KeyResult::Continue(Screen::SelectProjects {
				selected: selected.to_vec(),
				cursor: new_cursor,
				error: false,
			})
		}
		KeyCode::Down | KeyCode::Char('j') => {
			let new_cursor = if cursor + 1 >= len { 0 } else { cursor + 1 };
			KeyResult::Continue(Screen::SelectProjects {
				selected: selected.to_vec(),
				cursor: new_cursor,
				error: false,
			})
		}
		KeyCode::Char(' ') => {
			let mut new_selected = selected.to_vec();
			new_selected[cursor] = !new_selected[cursor];
			KeyResult::Continue(Screen::SelectProjects {
				selected: new_selected,
				cursor,
				error: false,
			})
		}
		KeyCode::Char('a') => {
			let all_selected = selected.iter().all(|&s| s);
			let new_selected = vec![!all_selected; len];
			KeyResult::Continue(Screen::SelectProjects {
				selected: new_selected,
				cursor,
				error: false,
			})
		}
		KeyCode::Enter => {
			if selected.iter().any(|&s| s) {
				KeyResult::Continue(Screen::SelectChangeType(ChangeType::Patch))
			} else {
				KeyResult::Continue(Screen::SelectProjects {
					selected: selected.to_vec(),
					cursor,
					error: true,
				})
			}
		}
		KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
		_ => KeyResult::Continue(Screen::SelectProjects {
			selected: selected.to_vec(),
			cursor,
			error: false,
		}),
	}
}

fn handle_key_change_type(current: ChangeType, key: KeyCode) -> KeyResult {
	match key {
		KeyCode::Left | KeyCode::Char('h') => {
			KeyResult::Continue(Screen::SelectChangeType(current.prev()))
		}
		KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => {
			KeyResult::Continue(Screen::SelectChangeType(current.next()))
		}
		KeyCode::Enter => KeyResult::Complete(vec![], current),
		KeyCode::Char('m') => KeyResult::Complete(vec![], ChangeType::Major),
		KeyCode::Char('i') => KeyResult::Complete(vec![], ChangeType::Minor),
		KeyCode::Char('p') => KeyResult::Complete(vec![], ChangeType::Patch),
		KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
		_ => KeyResult::Continue(Screen::SelectChangeType(current)),
	}
}

/// Runs the interactive TUI for selecting projects and a change type.
///
/// Displays a terminal UI that allows the user to select which projects
/// to include and the type of semantic version change.
///
/// # Returns
///
/// Returns `Ok(Some(ChangeResult))` if the user completes selection,
/// or `Ok(None)` if the user cancels.
///
/// # Errors
///
/// Returns an error if terminal setup or I/O operations fail.
pub fn run(projects: &[Project], options: &ChangeOptions) -> anyhow::Result<Option<ChangeResult>> {
	let project_indices = match &options.projects {
		Some(indices) => indices.clone(),
		None if projects.len() == 1 => vec![0],
		_ => {
			// Need interactive project selection - fall through to TUI
			vec![]
		}
	};

	let have_projects = !project_indices.is_empty();

	if let Some(change_type) = options.change_type {
		// Both pre-filled
		let indices = if have_projects {
			project_indices
		} else {
			(0..projects.len()).collect()
		};
		return Ok(Some(ChangeResult {
			projects: indices.into_iter().map(|i| projects[i].clone()).collect(),
			change_type,
		}));
	}

	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

	let project_names: Vec<&str> = projects.iter().map(|p| p.name()).collect();

	let initial_screen = if have_projects {
		Screen::SelectChangeType(ChangeType::Patch)
	} else {
		Screen::SelectProjects {
			selected: vec![true; projects.len()],
			cursor: 0,
			error: false,
		}
	};

	let mut screen = initial_screen;
	let mut selected_indices = project_indices;

	let result = loop {
		terminal.draw(|frame| ui(frame, &screen, &project_names))?;

		if let Event::Key(key) = crossterm::event::read()?
			&& key.kind == KeyEventKind::Press
		{
			match handle_key(&screen, key.code) {
				KeyResult::Continue(new_screen) => {
					// When transitioning from projects to change type, capture selected indices
					if matches!(new_screen, Screen::SelectChangeType(_))
						&& let Screen::SelectProjects { ref selected, .. } = screen
					{
						selected_indices = selected
							.iter()
							.enumerate()
							.filter(|&(_, &s)| s)
							.map(|(i, _)| i)
							.collect();
					}
					screen = new_screen;
				}
				KeyResult::Complete(_, change_type) => {
					break Some(ChangeResult {
						projects: selected_indices
							.iter()
							.map(|&i| projects[i].clone())
							.collect(),
						change_type,
					});
				}
				KeyResult::Cancelled => break None,
			}
		}
	};

	disable_raw_mode()?;
	io::stdout().execute(LeaveAlternateScreen)?;

	Ok(result)
}

fn ui(frame: &mut Frame, screen: &Screen, project_names: &[&str]) {
	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.margin(2)
		.constraints([
			Constraint::Length(3),
			Constraint::Min(5),
			Constraint::Length(1),
		])
		.split(frame.area());

	match screen {
		Screen::SelectProjects {
			selected,
			cursor,
			error,
		} => {
			render_select_projects(frame, &chunks, project_names, selected, *cursor, *error);
		}
		Screen::SelectChangeType(change_type) => {
			render_select_change_type(frame, &chunks, *change_type);
		}
	}
}

fn render_select_projects(
	frame: &mut Frame,
	chunks: &[Rect],
	project_names: &[&str],
	selected: &[bool],
	cursor: usize,
	error: bool,
) {
	let question_text = if error {
		"Select at least one project to continue."
	} else {
		"Which projects does this change apply to?"
	};
	let question_color = if error { Color::Red } else { Color::Yellow };
	let question = Paragraph::new(question_text)
		.style(Style::default().fg(question_color))
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, chunks[0]);

	let items: Vec<ListItem> = project_names
		.iter()
		.zip(selected.iter())
		.enumerate()
		.map(|(i, (name, &is_selected))| {
			let checkbox = if is_selected { "[x]" } else { "[ ]" };
			let style = if i == cursor {
				Style::default()
					.fg(Color::Cyan)
					.add_modifier(Modifier::BOLD)
			} else if is_selected {
				Style::default().fg(Color::Green)
			} else {
				Style::default().fg(Color::Gray)
			};
			ListItem::new(format!(" {checkbox} {name}")).style(style)
		})
		.collect();

	let list = List::new(items).block(
		Block::default()
			.borders(Borders::ALL)
			.title("Select Projects"),
	);
	frame.render_widget(list, chunks[1]);

	let help = Paragraph::new(
		"↑/↓/j/k: navigate | Space: toggle | a: toggle all | Enter: confirm | Esc: cancel",
	)
	.style(Style::default().fg(Color::DarkGray));
	frame.render_widget(help, chunks[2]);
}

fn render_select_change_type(frame: &mut Frame, chunks: &[Rect], selected: ChangeType) {
	let question = Paragraph::new("What type of change is this?")
		.style(Style::default().fg(Color::Yellow))
		.block(Block::default().borders(Borders::ALL));
	frame.render_widget(question, chunks[0]);

	let buttons = Line::from(
		std::iter::once(Span::raw("  "))
			.chain(button_spans(
				" ",
				"M",
				"ajor ",
				selected == ChangeType::Major,
			))
			.chain(std::iter::once(Span::raw("   ")))
			.chain(button_spans(
				" M",
				"i",
				"nor ",
				selected == ChangeType::Minor,
			))
			.chain(std::iter::once(Span::raw("   ")))
			.chain(button_spans(
				" ",
				"P",
				"atch ",
				selected == ChangeType::Patch,
			))
			.chain(std::iter::once(Span::raw("  ")))
			.collect::<Vec<_>>(),
	);
	let button_para =
		Paragraph::new(buttons).block(Block::default().borders(Borders::ALL).title("Change Type"));
	frame.render_widget(button_para, chunks[1]);

	let help = Paragraph::new("←/→/Tab: switch | m/i/p: select | Enter: confirm | Esc: cancel")
		.style(Style::default().fg(Color::DarkGray));
	frame.render_widget(help, chunks[2]);
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

fn button_spans<'a>(
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

	// handle_key tests - SelectChangeType screen
	#[test]
	fn change_type_left_moves_to_previous() {
		let screen = Screen::SelectChangeType(ChangeType::Minor);
		let result = handle_key(&screen, KeyCode::Left);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectChangeType(ChangeType::Major))
		);
	}

	#[test]
	fn change_type_right_moves_to_next() {
		let screen = Screen::SelectChangeType(ChangeType::Minor);
		let result = handle_key(&screen, KeyCode::Right);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectChangeType(ChangeType::Patch))
		);
	}

	#[test]
	fn change_type_tab_moves_to_next() {
		let screen = Screen::SelectChangeType(ChangeType::Major);
		let result = handle_key(&screen, KeyCode::Tab);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectChangeType(ChangeType::Minor))
		);
	}

	#[test]
	fn change_type_h_moves_to_previous() {
		let screen = Screen::SelectChangeType(ChangeType::Patch);
		let result = handle_key(&screen, KeyCode::Char('h'));
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectChangeType(ChangeType::Minor))
		);
	}

	#[test]
	fn change_type_l_moves_to_next() {
		let screen = Screen::SelectChangeType(ChangeType::Major);
		let result = handle_key(&screen, KeyCode::Char('l'));
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectChangeType(ChangeType::Minor))
		);
	}

	#[test]
	fn change_type_enter_completes_with_selected() {
		let screen = Screen::SelectChangeType(ChangeType::Major);
		let result = handle_key(&screen, KeyCode::Enter);
		assert_eq!(result, KeyResult::Complete(vec![], ChangeType::Major));

		let screen = Screen::SelectChangeType(ChangeType::Minor);
		let result = handle_key(&screen, KeyCode::Enter);
		assert_eq!(result, KeyResult::Complete(vec![], ChangeType::Minor));

		let screen = Screen::SelectChangeType(ChangeType::Patch);
		let result = handle_key(&screen, KeyCode::Enter);
		assert_eq!(result, KeyResult::Complete(vec![], ChangeType::Patch));
	}

	#[test]
	fn change_type_m_selects_major() {
		let screen = Screen::SelectChangeType(ChangeType::Patch);
		let result = handle_key(&screen, KeyCode::Char('m'));
		assert_eq!(result, KeyResult::Complete(vec![], ChangeType::Major));
	}

	#[test]
	fn change_type_i_selects_minor() {
		let screen = Screen::SelectChangeType(ChangeType::Patch);
		let result = handle_key(&screen, KeyCode::Char('i'));
		assert_eq!(result, KeyResult::Complete(vec![], ChangeType::Minor));
	}

	#[test]
	fn change_type_p_selects_patch() {
		let screen = Screen::SelectChangeType(ChangeType::Major);
		let result = handle_key(&screen, KeyCode::Char('p'));
		assert_eq!(result, KeyResult::Complete(vec![], ChangeType::Patch));
	}

	#[test]
	fn change_type_esc_cancels() {
		let screen = Screen::SelectChangeType(ChangeType::Minor);
		let result = handle_key(&screen, KeyCode::Esc);
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn change_type_q_cancels() {
		let screen = Screen::SelectChangeType(ChangeType::Minor);
		let result = handle_key(&screen, KeyCode::Char('q'));
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn change_type_other_keys_do_nothing() {
		let screen = Screen::SelectChangeType(ChangeType::Minor);
		let result = handle_key(&screen, KeyCode::Char('x'));
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectChangeType(ChangeType::Minor))
		);

		let result = handle_key(&screen, KeyCode::Up);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectChangeType(ChangeType::Minor))
		);
	}

	// handle_key tests - SelectProjects screen
	fn projects_screen(selected: Vec<bool>, cursor: usize) -> Screen {
		Screen::SelectProjects {
			selected,
			cursor,
			error: false,
		}
	}

	#[test]
	fn projects_up_moves_cursor_up() {
		let screen = projects_screen(vec![true, true, true], 1);
		let result = handle_key(&screen, KeyCode::Up);
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 0))
		);
	}

	#[test]
	fn projects_up_wraps_from_top() {
		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Up);
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 2))
		);
	}

	#[test]
	fn projects_k_moves_cursor_up() {
		let screen = projects_screen(vec![true, true], 1);
		let result = handle_key(&screen, KeyCode::Char('k'));
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true], 0))
		);
	}

	#[test]
	fn projects_down_moves_cursor_down() {
		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Down);
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 1))
		);
	}

	#[test]
	fn projects_down_wraps_from_bottom() {
		let screen = projects_screen(vec![true, true, true], 2);
		let result = handle_key(&screen, KeyCode::Down);
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 0))
		);
	}

	#[test]
	fn projects_j_moves_cursor_down() {
		let screen = projects_screen(vec![true, true], 0);
		let result = handle_key(&screen, KeyCode::Char('j'));
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true], 1))
		);
	}

	#[test]
	fn projects_space_toggles_selection() {
		let screen = projects_screen(vec![true, false, true], 1);
		let result = handle_key(&screen, KeyCode::Char(' '));
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 1))
		);

		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Char(' '));
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![false, true, true], 0))
		);
	}

	#[test]
	fn projects_a_toggles_all_on() {
		let screen = projects_screen(vec![true, false, true], 0);
		let result = handle_key(&screen, KeyCode::Char('a'));
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 0))
		);
	}

	#[test]
	fn projects_a_toggles_all_off_when_all_selected() {
		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Char('a'));
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![false, false, false], 0))
		);
	}

	#[test]
	fn projects_enter_advances_when_at_least_one_selected() {
		let screen = projects_screen(vec![false, true, false], 1);
		let result = handle_key(&screen, KeyCode::Enter);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectChangeType(ChangeType::Patch))
		);
	}

	#[test]
	fn projects_enter_shows_error_when_none_selected() {
		let screen = projects_screen(vec![false, false, false], 0);
		let result = handle_key(&screen, KeyCode::Enter);
		assert_eq!(
			result,
			KeyResult::Continue(Screen::SelectProjects {
				selected: vec![false, false, false],
				cursor: 0,
				error: true,
			})
		);
	}

	#[test]
	fn projects_esc_cancels() {
		let screen = projects_screen(vec![true, true], 0);
		let result = handle_key(&screen, KeyCode::Esc);
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn projects_q_cancels() {
		let screen = projects_screen(vec![true, true], 0);
		let result = handle_key(&screen, KeyCode::Char('q'));
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn projects_other_keys_do_nothing() {
		let screen = projects_screen(vec![true, false], 0);
		let result = handle_key(&screen, KeyCode::Char('x'));
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, false], 0))
		);

		let result = handle_key(&screen, KeyCode::Left);
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, false], 0))
		);
	}

	#[test]
	fn projects_error_clears_on_navigation() {
		let screen = Screen::SelectProjects {
			selected: vec![false, false],
			cursor: 0,
			error: true,
		};
		let result = handle_key(&screen, KeyCode::Down);
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![false, false], 1))
		);
	}

	#[test]
	fn projects_error_clears_on_toggle() {
		let screen = Screen::SelectProjects {
			selected: vec![false, false],
			cursor: 0,
			error: true,
		};
		let result = handle_key(&screen, KeyCode::Char(' '));
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, false], 0))
		);
	}

	// UI rendering tests
	fn create_test_terminal() -> Terminal<ratatui::backend::TestBackend> {
		let backend = ratatui::backend::TestBackend::new(80, 24);
		Terminal::new(backend).unwrap()
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

	#[test]
	fn ui_renders_select_projects_screen() {
		let mut terminal = create_test_terminal();
		let names = vec!["project-a", "project-b"];
		let screen = projects_screen(vec![true, false], 0);
		terminal.draw(|frame| ui(frame, &screen, &names)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Select Projects"));
		assert!(content.contains("project-a"));
		assert!(content.contains("project-b"));
		assert!(content.contains("[x]"));
		assert!(content.contains("[ ]"));
	}

	#[test]
	fn ui_renders_select_projects_error() {
		let mut terminal = create_test_terminal();
		let names = vec!["project-a"];
		let screen = Screen::SelectProjects {
			selected: vec![false],
			cursor: 0,
			error: true,
		};
		terminal.draw(|frame| ui(frame, &screen, &names)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Select at least one project"));
	}

	#[test]
	fn ui_renders_select_change_type_screen() {
		let mut terminal = create_test_terminal();
		let names: Vec<&str> = vec![];
		let screen = Screen::SelectChangeType(ChangeType::Major);
		terminal.draw(|frame| ui(frame, &screen, &names)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Major"));
		assert!(content.contains("Minor"));
		assert!(content.contains("Patch"));
	}

	#[test]
	fn ui_renders_change_type_with_minor_selected() {
		let mut terminal = create_test_terminal();
		let names: Vec<&str> = vec![];
		let screen = Screen::SelectChangeType(ChangeType::Minor);
		terminal.draw(|frame| ui(frame, &screen, &names)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("What type of change"));
	}

	#[test]
	fn ui_renders_change_type_with_patch_selected() {
		let mut terminal = create_test_terminal();
		let names: Vec<&str> = vec![];
		let screen = Screen::SelectChangeType(ChangeType::Patch);
		terminal.draw(|frame| ui(frame, &screen, &names)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Change Type"));
	}

	// Workflow test
	#[test]
	fn workflow_select_projects_then_change_type() {
		// Start with 3 projects, all selected
		let screen = projects_screen(vec![true, true, true], 0);

		// Deselect first project
		let result = handle_key(&screen, KeyCode::Char(' '));
		let screen = match result {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		};
		assert_eq!(screen, projects_screen(vec![false, true, true], 0));

		// Confirm project selection
		let result = handle_key(&screen, KeyCode::Enter);
		let screen = match result {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		};
		assert_eq!(screen, Screen::SelectChangeType(ChangeType::Patch));

		// Select minor
		let result = handle_key(&screen, KeyCode::Char('i'));
		assert_eq!(result, KeyResult::Complete(vec![], ChangeType::Minor));
	}
}
