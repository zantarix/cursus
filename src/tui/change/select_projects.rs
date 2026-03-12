use crossterm::event::KeyCode;
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, List, ListItem},
};

use crate::tui::widgets::{self, KeyResult};

use super::{HandleResult, Screen};

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
			change_type: crate::model::changeset::ChangeType::Patch,
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

/// Handles key events for the [`Screen::SelectProjects`] screen.
pub(super) fn handle_key_select_projects(
	selected: &[bool],
	cursor: usize,
	key: KeyCode,
) -> HandleResult {
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

/// Renders the [`Screen::SelectProjects`] screen.
pub(super) fn render_select_projects(
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

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};

	use super::super::test_helpers::dummy_projects;
	use super::super::{Screen, handle_key};

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
		let result = handle_key(&screen, KeyCode::Up, &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, true, true], 0))
		);
	}

	#[test]
	fn projects_up_wraps_from_top() {
		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Up, &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, true, true], 2))
		);
	}

	#[test]
	fn projects_k_moves_cursor_up() {
		let screen = projects_screen(vec![true, true], 1);
		let result = handle_key(&screen, KeyCode::Char('k'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, true], 0))
		);
	}

	#[test]
	fn projects_down_moves_cursor_down() {
		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Down, &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, true, true], 1))
		);
	}

	#[test]
	fn projects_down_wraps_from_bottom() {
		let screen = projects_screen(vec![true, true, true], 2);
		let result = handle_key(&screen, KeyCode::Down, &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, true, true], 0))
		);
	}

	#[test]
	fn projects_j_moves_cursor_down() {
		let screen = projects_screen(vec![true, true], 0);
		let result = handle_key(&screen, KeyCode::Char('j'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, true], 1))
		);
	}

	#[test]
	fn projects_space_toggles_selection() {
		let screen = projects_screen(vec![true, false, true], 1);
		let result = handle_key(&screen, KeyCode::Char(' '), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, true, true], 1))
		);

		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Char(' '), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![false, true, true], 0))
		);
	}

	#[test]
	fn projects_a_toggles_all_on() {
		let screen = projects_screen(vec![true, false, true], 0);
		let result = handle_key(&screen, KeyCode::Char('a'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, true, true], 0))
		);
	}

	#[test]
	fn projects_a_toggles_all_off_when_all_selected() {
		let screen = projects_screen(vec![true, true, true], 0);
		let result = handle_key(&screen, KeyCode::Char('a'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![false, false, false], 0))
		);
	}

	#[test]
	fn projects_enter_advances_when_at_least_one_selected() {
		use crate::model::changeset::ChangeType;
		let screen = projects_screen(vec![false, true, false], 1);
		let result = handle_key(&screen, KeyCode::Enter, &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectChangeType {
				change_type: ChangeType::Patch,
				selected_indices: vec![1],
			})
		);
	}

	#[test]
	fn projects_enter_shows_error_when_none_selected() {
		let screen = projects_screen(vec![false, false, false], 0);
		let result = handle_key(&screen, KeyCode::Enter, &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectProjects {
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
		assert_eq!(result, super::super::KeyResult::Cancelled);
	}

	#[test]
	fn projects_q_cancels() {
		let screen = projects_screen(vec![true, true], 0);
		let result = handle_key(&screen, KeyCode::Char('q'), &[]).unwrap();
		assert_eq!(result, super::super::KeyResult::Cancelled);
	}

	#[test]
	fn projects_other_keys_do_nothing() {
		let screen = projects_screen(vec![true, false], 0);
		let result = handle_key(&screen, KeyCode::Char('x'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, false], 0))
		);

		let result = handle_key(&screen, KeyCode::Left, &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(projects_screen(vec![true, false], 0))
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
			super::super::KeyResult::Continue(projects_screen(vec![false, false], 1))
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
			super::super::KeyResult::Continue(projects_screen(vec![true, false], 0))
		);
	}

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
				super::super::KeyResult::Continue(projects_screen(vec![], 0)),
				"key {key:?} should be a no-op on empty projects"
			);
		}
	}

	#[test]
	fn projects_empty_esc_cancels() {
		let screen = projects_screen(vec![], 0);
		let result = handle_key(&screen, KeyCode::Esc, &[]).unwrap();
		assert_eq!(result, super::super::KeyResult::Cancelled);
	}

	#[test]
	fn projects_empty_q_cancels() {
		let screen = projects_screen(vec![], 0);
		let result = handle_key(&screen, KeyCode::Char('q'), &[]).unwrap();
		assert_eq!(result, super::super::KeyResult::Cancelled);
	}

	#[test]
	fn ui_renders_select_projects_screen() {
		let mut terminal = create_test_terminal();
		let projects = dummy_projects(2);
		let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
		let screen = projects_screen(vec![true, false], 0);
		terminal
			.draw(|frame| super::super::ui(frame, &screen, &names))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Select Projects"));
		assert!(content.contains("project-0"));
		assert!(content.contains("project-1"));
		assert!(content.contains("[x]"));
		assert!(content.contains("[ ]"));
	}

	#[test]
	fn ui_renders_select_projects_error() {
		let mut terminal = create_test_terminal();
		let projects = dummy_projects(1);
		let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
		let screen = Screen::SelectProjects {
			selected: vec![false],
			cursor: 0,
			error: true,
		};
		terminal
			.draw(|frame| super::super::ui(frame, &screen, &names))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Select at least one project"));
	}
}
