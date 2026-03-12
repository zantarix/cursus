//! TUI for selecting projects and the type of change (major, minor, patch).

use crossterm::event::KeyCode;
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, List, ListItem, Paragraph},
};

use anyhow::Context as _;

use super::widgets::{self, KeyResult};
use crate::model::changeset::ChangeType;
use crate::package_manager::Project;

/// Shorthand for the handle_key return type used by the internal state machine.
type HandleResult = KeyResult<Screen, ChangeResult>;

impl ChangeType {
	/// Returns the next change type when cycling through options in the TUI.
	fn next(self) -> Self {
		match self {
			Self::Major => Self::Minor,
			Self::Minor => Self::Patch,
			Self::Patch => Self::Major,
		}
	}

	/// Returns the previous change type when cycling through options in the TUI.
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
	SelectChangeType {
		change_type: ChangeType,
		selected_indices: Vec<usize>,
	},
}

fn handle_key(screen: &Screen, key: KeyCode, projects: &[Project]) -> anyhow::Result<HandleResult> {
	match screen {
		Screen::SelectProjects {
			selected, cursor, ..
		} => Ok(handle_key_select_projects(selected, *cursor, key)),
		Screen::SelectChangeType {
			change_type,
			selected_indices,
		} => handle_key_change_type(*change_type, selected_indices, key, projects),
	}
}

fn move_project_cursor(selected: &[bool], cursor: usize, up: bool) -> HandleResult {
	let len = selected.len();
	let new_cursor = if up {
		if cursor == 0 { len - 1 } else { cursor - 1 }
	} else if cursor + 1 >= len {
		0
	} else {
		cursor + 1
	};
	KeyResult::Continue(Screen::SelectProjects {
		selected: selected.to_vec(),
		cursor: new_cursor,
		error: false,
	})
}

fn advance_to_change_type(selected: &[bool], cursor: usize) -> HandleResult {
	if selected.iter().any(|&s| s) {
		let selected_indices = selected
			.iter()
			.enumerate()
			.filter(|&(_, &s)| s)
			.map(|(i, _)| i)
			.collect();
		KeyResult::Continue(Screen::SelectChangeType {
			change_type: ChangeType::Patch,
			selected_indices,
		})
	} else {
		KeyResult::Continue(Screen::SelectProjects {
			selected: selected.to_vec(),
			cursor,
			error: true,
		})
	}
}

fn handle_key_select_projects(selected: &[bool], cursor: usize, key: KeyCode) -> HandleResult {
	let len = selected.len();
	if len == 0 {
		return match key {
			KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
			_ => KeyResult::Continue(Screen::SelectProjects {
				selected: vec![],
				cursor: 0,
				error: false,
			}),
		};
	}
	match key {
		KeyCode::Up | KeyCode::Char('k') => move_project_cursor(selected, cursor, true),
		KeyCode::Down | KeyCode::Char('j') => move_project_cursor(selected, cursor, false),
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
		KeyCode::Enter => advance_to_change_type(selected, cursor),
		KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
		_ => KeyResult::Continue(Screen::SelectProjects {
			selected: selected.to_vec(),
			cursor,
			error: false,
		}),
	}
}

fn handle_key_change_type(
	current: ChangeType,
	selected_indices: &[usize],
	key: KeyCode,
	projects: &[Project],
) -> anyhow::Result<HandleResult> {
	let complete = |ct: ChangeType| -> anyhow::Result<HandleResult> {
		let resolved = selected_indices
			.iter()
			.map(|&i| {
				projects.get(i).cloned().with_context(|| {
					format!(
						"selected index {i} is out of range ({} projects)",
						projects.len()
					)
				})
			})
			.collect::<anyhow::Result<Vec<_>>>()?;
		Ok(KeyResult::Complete(ChangeResult {
			projects: resolved,
			change_type: ct,
		}))
	};
	match key {
		KeyCode::Left | KeyCode::Char('h') => Ok(KeyResult::Continue(Screen::SelectChangeType {
			change_type: current.prev(),
			selected_indices: selected_indices.to_vec(),
		})),
		KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => {
			Ok(KeyResult::Continue(Screen::SelectChangeType {
				change_type: current.next(),
				selected_indices: selected_indices.to_vec(),
			}))
		}
		KeyCode::Enter => complete(current),
		KeyCode::Char('m') => complete(ChangeType::Major),
		KeyCode::Char('i') => complete(ChangeType::Minor),
		KeyCode::Char('p') => complete(ChangeType::Patch),
		KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
		_ => Ok(KeyResult::Continue(Screen::SelectChangeType {
			change_type: current,
			selected_indices: selected_indices.to_vec(),
		})),
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

	let project_names: Vec<&str> = projects.iter().map(|p| p.name()).collect();

	let initial_screen = if have_projects {
		Screen::SelectChangeType {
			change_type: ChangeType::Patch,
			selected_indices: project_indices,
		}
	} else {
		Screen::SelectProjects {
			selected: vec![true; projects.len()],
			cursor: 0,
			error: false,
		}
	};

	let result = widgets::run_tui(
		initial_screen,
		|frame, screen| ui(frame, screen, &project_names),
		|screen, key| handle_key(&screen, key.code, projects),
	)?;

	Ok(result)
}

fn ui(frame: &mut Frame, screen: &Screen, project_names: &[&str]) {
	let chunks = widgets::wizard_layout(
		frame,
		&[
			Constraint::Length(3),
			Constraint::Min(5),
			Constraint::Length(1),
		],
	);

	match screen {
		Screen::SelectProjects {
			selected,
			cursor,
			error,
		} => {
			render_select_projects(frame, &chunks, project_names, selected, *cursor, *error);
		}
		Screen::SelectChangeType { change_type, .. } => {
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
	widgets::render_question(frame, chunks[0], question_text, question_color);

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

	widgets::render_help(
		frame,
		chunks[2],
		"↑/↓/j/k: navigate | Space: toggle | a: toggle all | Enter: confirm | Esc: cancel",
	);
}

fn render_select_change_type(frame: &mut Frame, chunks: &[Rect], selected: ChangeType) {
	widgets::render_question(
		frame,
		chunks[0],
		"What type of change is this?",
		Color::Yellow,
	);

	let buttons = Line::from(
		std::iter::once(Span::raw("  "))
			.chain(widgets::button_spans(
				" ",
				"M",
				"ajor ",
				selected == ChangeType::Major,
			))
			.chain(std::iter::once(Span::raw("   ")))
			.chain(widgets::button_spans(
				" M",
				"i",
				"nor ",
				selected == ChangeType::Minor,
			))
			.chain(std::iter::once(Span::raw("   ")))
			.chain(widgets::button_spans(
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

	widgets::render_help(
		frame,
		chunks[2],
		"←/→/Tab: switch | m/i/p: select | Enter: confirm | Esc: cancel",
	);
}

#[cfg(test)]
mod tests {
	use super::super::test_utils::{buffer_to_string, create_test_terminal};
	use super::*;

	fn dummy_projects(n: usize) -> Vec<Project> {
		(0..n)
			.map(|i| {
				Project::new_test(
					&format!("project-{i}"),
					&format!("/nonexistent/projects/project-{i}"),
				)
			})
			.collect()
	}

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

	// Helpers
	fn projects_screen(selected: Vec<bool>, cursor: usize) -> Screen {
		Screen::SelectProjects {
			selected,
			cursor,
			error: false,
		}
	}

	fn change_type_screen(change_type: ChangeType, selected_indices: Vec<usize>) -> Screen {
		Screen::SelectChangeType {
			change_type,
			selected_indices,
		}
	}

	// handle_key tests - SelectChangeType screen
	// Navigation tests use empty selected_indices since projects are never indexed during navigation.
	#[test]
	fn change_type_left_moves_to_previous() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Left, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Major, vec![]))
		);
	}

	#[test]
	fn change_type_right_moves_to_next() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Right, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Patch, vec![]))
		);
	}

	#[test]
	fn change_type_tab_moves_to_next() {
		let screen = change_type_screen(ChangeType::Major, vec![]);
		let result = handle_key(&screen, KeyCode::Tab, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);
	}

	#[test]
	fn change_type_h_moves_to_previous() {
		let screen = change_type_screen(ChangeType::Patch, vec![]);
		let result = handle_key(&screen, KeyCode::Char('h'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);
	}

	#[test]
	fn change_type_l_moves_to_next() {
		let screen = change_type_screen(ChangeType::Major, vec![]);
		let result = handle_key(&screen, KeyCode::Char('l'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);
	}

	#[test]
	fn change_type_enter_completes_with_selected() {
		let projects = dummy_projects(2);

		let screen = change_type_screen(ChangeType::Major, vec![0]);
		let result = handle_key(&screen, KeyCode::Enter, &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: vec![projects[0].clone()],
				change_type: ChangeType::Major,
			})
		);

		let screen = change_type_screen(ChangeType::Minor, vec![1]);
		let result = handle_key(&screen, KeyCode::Enter, &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: vec![projects[1].clone()],
				change_type: ChangeType::Minor,
			})
		);

		let screen = change_type_screen(ChangeType::Patch, vec![0, 1]);
		let result = handle_key(&screen, KeyCode::Enter, &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: projects.clone(),
				change_type: ChangeType::Patch,
			})
		);
	}

	#[test]
	fn change_type_m_selects_major() {
		let projects = dummy_projects(1);
		let screen = change_type_screen(ChangeType::Patch, vec![0]);
		let result = handle_key(&screen, KeyCode::Char('m'), &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: projects.clone(),
				change_type: ChangeType::Major,
			})
		);
	}

	#[test]
	fn change_type_i_selects_minor() {
		let projects = dummy_projects(1);
		let screen = change_type_screen(ChangeType::Patch, vec![0]);
		let result = handle_key(&screen, KeyCode::Char('i'), &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: projects.clone(),
				change_type: ChangeType::Minor,
			})
		);
	}

	#[test]
	fn change_type_p_selects_patch() {
		let projects = dummy_projects(1);
		let screen = change_type_screen(ChangeType::Major, vec![0]);
		let result = handle_key(&screen, KeyCode::Char('p'), &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: projects.clone(),
				change_type: ChangeType::Patch,
			})
		);
	}

	#[test]
	fn change_type_esc_cancels() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Esc, &[]).unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn change_type_q_cancels() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Char('q'), &[]).unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn change_type_other_keys_do_nothing() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Char('x'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);

		let result = handle_key(&screen, KeyCode::Up, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);
	}

	#[test]
	fn change_type_out_of_bounds_index_returns_error() {
		// selected_indices pointing beyond the projects slice must return Err, not panic
		let projects = dummy_projects(1);
		let screen = change_type_screen(ChangeType::Patch, vec![99]);
		let err = handle_key(&screen, KeyCode::Enter, &projects).unwrap_err();
		let msg = err.to_string();
		assert!(
			msg.contains("99"),
			"error should mention the bad index: {msg}"
		);
		assert!(
			msg.contains("1 projects"),
			"error should mention the slice length: {msg}"
		);
	}

	// Coverage gap: prefilled projects start the TUI at SelectChangeType directly.
	// This test verifies that an initial screen constructed with pre-filled indices
	// (the path taken by run() when have_projects == true) resolves correctly on Enter.
	#[test]
	fn prefilled_projects_initial_screen_completes_correctly() {
		let projects = dummy_projects(3);
		// Simulates run() initial_screen when have_projects == true with indices [0, 2]
		let screen = change_type_screen(ChangeType::Patch, vec![0, 2]);
		let result = handle_key(&screen, KeyCode::Enter, &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: vec![projects[0].clone(), projects[2].clone()],
				change_type: ChangeType::Patch,
			})
		);
	}

	// handle_key tests - SelectProjects screen
	#[test]
	fn projects_up_moves_cursor_up() {
		let screen = projects_screen(vec![true, true, true], 1);
		let result = handle_key(&screen, KeyCode::Up, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 0))
		);
	}

	#[test]
	fn projects_up_wraps_from_top() {
		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Up, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 2))
		);
	}

	#[test]
	fn projects_k_moves_cursor_up() {
		let screen = projects_screen(vec![true, true], 1);
		let result = handle_key(&screen, KeyCode::Char('k'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true], 0))
		);
	}

	#[test]
	fn projects_down_moves_cursor_down() {
		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Down, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 1))
		);
	}

	#[test]
	fn projects_down_wraps_from_bottom() {
		let screen = projects_screen(vec![true, true, true], 2);
		let result = handle_key(&screen, KeyCode::Down, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 0))
		);
	}

	#[test]
	fn projects_j_moves_cursor_down() {
		let screen = projects_screen(vec![true, true], 0);
		let result = handle_key(&screen, KeyCode::Char('j'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true], 1))
		);
	}

	#[test]
	fn projects_space_toggles_selection() {
		let screen = projects_screen(vec![true, false, true], 1);
		let result = handle_key(&screen, KeyCode::Char(' '), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 1))
		);

		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Char(' '), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![false, true, true], 0))
		);
	}

	#[test]
	fn projects_a_toggles_all_on() {
		let screen = projects_screen(vec![true, false, true], 0);
		let result = handle_key(&screen, KeyCode::Char('a'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, true, true], 0))
		);
	}

	#[test]
	fn projects_a_toggles_all_off_when_all_selected() {
		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Char('a'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![false, false, false], 0))
		);
	}

	#[test]
	fn projects_enter_advances_when_at_least_one_selected() {
		let screen = projects_screen(vec![false, true, false], 1);
		let result = handle_key(&screen, KeyCode::Enter, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Patch, vec![1]))
		);
	}

	#[test]
	fn projects_enter_shows_error_when_none_selected() {
		let screen = projects_screen(vec![false, false, false], 0);
		let result = handle_key(&screen, KeyCode::Enter, &[]).unwrap();
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
		let result = handle_key(&screen, KeyCode::Esc, &[]).unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn projects_q_cancels() {
		let screen = projects_screen(vec![true, true], 0);
		let result = handle_key(&screen, KeyCode::Char('q'), &[]).unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn projects_other_keys_do_nothing() {
		let screen = projects_screen(vec![true, false], 0);
		let result = handle_key(&screen, KeyCode::Char('x'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, false], 0))
		);

		let result = handle_key(&screen, KeyCode::Left, &[]).unwrap();
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
		let result = handle_key(&screen, KeyCode::Down, &[]).unwrap();
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
		let result = handle_key(&screen, KeyCode::Char(' '), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(projects_screen(vec![true, false], 0))
		);
	}

	// UI rendering tests
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
		let screen = change_type_screen(ChangeType::Major, vec![]);
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
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		terminal.draw(|frame| ui(frame, &screen, &names)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("What type of change"));
	}

	#[test]
	fn ui_renders_change_type_with_patch_selected() {
		let mut terminal = create_test_terminal();
		let names: Vec<&str> = vec![];
		let screen = change_type_screen(ChangeType::Patch, vec![]);
		terminal.draw(|frame| ui(frame, &screen, &names)).unwrap();
		let buffer = terminal.backend().buffer().clone();
		let content = buffer_to_string(&buffer);
		assert!(content.contains("Change Type"));
	}

	// Empty projects guard tests
	#[test]
	fn projects_empty_navigation_keys_are_no_ops() {
		let screen = projects_screen(vec![], 0);
		for key in [
			KeyCode::Up,
			KeyCode::Down,
			KeyCode::Char('k'),
			KeyCode::Char('j'),
			KeyCode::Char(' '),
			KeyCode::Char('a'),
			KeyCode::Enter,
			KeyCode::Char('x'),
		] {
			let result = handle_key(&screen, key, &[]).unwrap();
			assert_eq!(
				result,
				KeyResult::Continue(projects_screen(vec![], 0)),
				"key {key:?} should be a no-op on empty projects"
			);
		}
	}

	#[test]
	fn projects_empty_esc_cancels() {
		let screen = projects_screen(vec![], 0);
		let result = handle_key(&screen, KeyCode::Esc, &[]).unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn projects_empty_q_cancels() {
		let screen = projects_screen(vec![], 0);
		let result = handle_key(&screen, KeyCode::Char('q'), &[]).unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	// Workflow test
	#[test]
	fn workflow_select_projects_then_change_type() {
		let projects = dummy_projects(3);

		// Start with 3 projects, all selected
		let screen = projects_screen(vec![true, true, true], 0);

		// Deselect first project
		let screen = match handle_key(&screen, KeyCode::Char(' '), &projects).unwrap() {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		};
		assert_eq!(screen, projects_screen(vec![false, true, true], 0));

		// Confirm project selection
		let screen = match handle_key(&screen, KeyCode::Enter, &projects).unwrap() {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		};
		assert_eq!(screen, change_type_screen(ChangeType::Patch, vec![1, 2]));

		// Select minor
		let result = handle_key(&screen, KeyCode::Char('i'), &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: vec![projects[1].clone(), projects[2].clone()],
				change_type: ChangeType::Minor,
			})
		);
	}
}
