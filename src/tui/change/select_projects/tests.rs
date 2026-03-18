use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::prelude::Rect;

use crate::model::changeset::ChangeType;
use crate::tui::test_utils::{buffer_to_string, create_test_terminal, mouse_click};

use super::super::test_helpers::dummy_projects;
use super::super::{Screen, handle_key};

fn projects_screen(selected: Vec<bool>, levels: Vec<ChangeType>, cursor: usize) -> Screen {
	let changed_count = selected.len(); // treat all as "changed" for test simplicity
	Screen::SelectProjects(super::super::SelectProjectsState {
		selected,
		levels,
		cursor,
		error: false,
		changed_count,
	})
}

fn default_levels(n: usize) -> Vec<ChangeType> {
	vec![ChangeType::Patch; n]
}

/// Unwrap a `Continue(SelectProjects(...))` result.
fn unwrap_select_projects(
	result: anyhow::Result<super::super::HandleResult>,
) -> (Vec<bool>, Vec<ChangeType>, usize, bool, usize) {
	match result.unwrap() {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState {
				selected,
				levels,
				cursor,
				error,
				changed_count,
			},
		)) => (selected, levels, cursor, error, changed_count),
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

/// Simulates a mouse click on the projects screen using an 80×24 terminal area.
fn click(
	col: u16,
	row: u16,
	selected: &[bool],
	levels: &[ChangeType],
	cursor: usize,
	changed_count: usize,
) -> super::HandleResult {
	let area = Rect::new(0, 0, 80, 24);
	super::handle_event_select_projects(
		super::super::SelectProjectsState {
			selected: selected.to_vec(),
			levels: levels.to_vec(),
			cursor,
			error: false,
			changed_count,
		},
		mouse_click(col, row),
		area,
		&[],
	)
}

#[test]
fn projects_up_moves_cursor_up() {
	let screen = projects_screen(vec![true, true, true], default_levels(3), 1);
	let (_, _, cursor, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Up, &[]));
	assert_eq!(cursor, 0);
}

#[test]
fn projects_up_wraps_from_top() {
	let screen = projects_screen(vec![true, true, true], default_levels(3), 0);
	let (_, _, cursor, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Up, &[]));
	assert_eq!(cursor, 2);
}

#[test]
fn projects_k_moves_cursor_up() {
	let screen = projects_screen(vec![true, true], default_levels(2), 1);
	let (_, _, cursor, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('k'), &[]));
	assert_eq!(cursor, 0);
}

#[test]
fn projects_down_moves_cursor_down() {
	let screen = projects_screen(vec![true, true, true], default_levels(3), 0);
	let (_, _, cursor, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Down, &[]));
	assert_eq!(cursor, 1);
}

#[test]
fn projects_down_wraps_from_bottom() {
	let screen = projects_screen(vec![true, true, true], default_levels(3), 2);
	let (_, _, cursor, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Down, &[]));
	assert_eq!(cursor, 0);
}

#[test]
fn projects_j_moves_cursor_down() {
	let screen = projects_screen(vec![true, true], default_levels(2), 0);
	let (_, _, cursor, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('j'), &[]));
	assert_eq!(cursor, 1);
}

#[test]
fn projects_space_toggles_selection() {
	let screen = projects_screen(vec![true, false, true], default_levels(3), 1);
	let (selected, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char(' '), &[]));
	assert_eq!(selected, vec![true, true, true]);

	let screen = projects_screen(vec![true, true, true], default_levels(3), 0);
	let (selected, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char(' '), &[]));
	assert_eq!(selected, vec![false, true, true]);
}

#[test]
fn projects_right_cycles_level_when_selected() {
	let screen = projects_screen(
		vec![true, false],
		vec![ChangeType::Patch, ChangeType::Patch],
		0,
	);
	let (_, levels, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Right, &[]));
	assert_eq!(levels[0], ChangeType::Major);
	assert_eq!(levels[1], ChangeType::Patch);
}

#[test]
fn projects_left_cycles_level_when_selected() {
	let screen = projects_screen(
		vec![true, false],
		vec![ChangeType::Patch, ChangeType::Patch],
		0,
	);
	let (_, levels, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Left, &[]));
	assert_eq!(levels[0], ChangeType::Minor);
}

#[test]
fn projects_right_noop_when_not_selected() {
	let screen = projects_screen(
		vec![false, true],
		vec![ChangeType::Patch, ChangeType::Patch],
		0,
	);
	let (_, levels, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Right, &[]));
	// Cursor is at 0 which is not selected → no level change
	assert_eq!(levels[0], ChangeType::Patch);
}

#[test]
fn projects_dot_bulk_cycles_all_selected_forward() {
	let screen = projects_screen(
		vec![true, false, true],
		vec![ChangeType::Patch, ChangeType::Patch, ChangeType::Patch],
		0,
	);
	let (_, levels, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('.'), &[]));
	// First selected is index 0 at Patch → next is Major; both selected get Major
	assert_eq!(levels[0], ChangeType::Major);
	assert_eq!(levels[1], ChangeType::Patch); // not selected, unchanged
	assert_eq!(levels[2], ChangeType::Major);
}

#[test]
fn projects_comma_bulk_cycles_all_selected_backward() {
	let screen = projects_screen(
		vec![true, false, true],
		vec![ChangeType::Patch, ChangeType::Patch, ChangeType::Patch],
		0,
	);
	let (_, levels, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char(','), &[]));
	// Patch → prev = Minor
	assert_eq!(levels[0], ChangeType::Minor);
	assert_eq!(levels[1], ChangeType::Patch); // not selected, unchanged
	assert_eq!(levels[2], ChangeType::Minor);
}

#[test]
fn projects_a_toggles_all_on() {
	let screen = projects_screen(vec![true, false, true], default_levels(3), 0);
	let (selected, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('a'), &[]));
	assert_eq!(selected, vec![true, true, true]);
}

#[test]
fn projects_a_toggles_all_off_when_all_selected() {
	let screen = projects_screen(vec![true, true, true], default_levels(3), 0);
	let (selected, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('a'), &[]));
	assert_eq!(selected, vec![false, false, false]);
}

#[test]
fn projects_c_toggles_changed_group_on() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![false, false, true],
		levels: default_levels(3),
		cursor: 0,
		error: false,
		changed_count: 2,
	});
	let (selected, _, _, _, changed_count) =
		unwrap_select_projects(handle_key(screen, KeyCode::Char('c'), &[]));
	assert_eq!(selected[0], true);
	assert_eq!(selected[1], true);
	assert_eq!(selected[2], true);
	assert_eq!(changed_count, 2);
}

#[test]
fn projects_c_toggles_changed_group_off_when_all_on() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![true, true, false],
		levels: default_levels(3),
		cursor: 0,
		error: false,
		changed_count: 2,
	});
	let (selected, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('c'), &[]));
	assert_eq!(selected, vec![false, false, false]);
}

#[test]
fn projects_c_with_zero_changed_count_is_noop() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![false, false],
		levels: default_levels(2),
		cursor: 0,
		error: false,
		changed_count: 0,
	});
	let (selected, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('c'), &[]));
	assert_eq!(selected, vec![false, false]);
}

#[test]
fn projects_u_toggles_unchanged_group_on() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![true, false, false],
		levels: default_levels(3),
		cursor: 0,
		error: false,
		changed_count: 1,
	});
	let (selected, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('u'), &[]));
	assert_eq!(selected, vec![true, true, true]);
}

#[test]
fn projects_u_toggles_unchanged_group_off_when_all_on() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![false, true, true],
		levels: default_levels(3),
		cursor: 0,
		error: false,
		changed_count: 1,
	});
	let (selected, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('u'), &[]));
	assert_eq!(selected, vec![false, false, false]);
}

#[test]
fn projects_u_with_all_changed_is_noop() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![true, true],
		levels: default_levels(2),
		cursor: 0,
		error: false,
		changed_count: 2,
	});
	let (selected, ..) = unwrap_select_projects(handle_key(screen, KeyCode::Char('u'), &[]));
	assert_eq!(selected, vec![true, true]);
}

#[test]
fn projects_enter_advances_to_enter_message_when_at_least_one_selected() {
	let projects = dummy_projects(3);
	let screen = projects_screen(vec![false, true, false], default_levels(3), 1);
	let result = handle_key(screen, KeyCode::Enter, &projects).unwrap();
	match result {
		super::super::KeyResult::Continue(Screen::EnterMessage { projects: proj, .. }) => {
			assert_eq!(proj.len(), 1);
			assert_eq!(proj[0].0.name(), "project-1");
			assert_eq!(proj[0].1, ChangeType::Patch);
		}
		_ => panic!("Expected Continue(EnterMessage)"),
	}
}

#[test]
fn projects_enter_preserves_levels_in_result() {
	let projects = dummy_projects(2);
	let screen = projects_screen(
		vec![true, true],
		vec![ChangeType::Major, ChangeType::Minor],
		0,
	);
	let result = handle_key(screen, KeyCode::Enter, &projects).unwrap();
	match result {
		super::super::KeyResult::Continue(Screen::EnterMessage { projects: proj, .. }) => {
			assert_eq!(proj[0].1, ChangeType::Major);
			assert_eq!(proj[1].1, ChangeType::Minor);
		}
		_ => panic!("Expected Continue(EnterMessage)"),
	}
}

#[test]
fn projects_enter_shows_error_when_none_selected() {
	let screen = projects_screen(vec![false, false, false], default_levels(3), 0);
	let (_, _, _, error, _) = unwrap_select_projects(handle_key(screen, KeyCode::Enter, &[]));
	assert!(error);
}

#[test]
fn projects_esc_cancels() {
	let screen = projects_screen(vec![true, true], default_levels(2), 0);
	let result = handle_key(screen, KeyCode::Esc, &[]).unwrap();
	assert!(matches!(result, super::super::KeyResult::Cancelled));
}

#[test]
fn projects_q_cancels() {
	let screen = projects_screen(vec![true, true], default_levels(2), 0);
	let result = handle_key(screen, KeyCode::Char('q'), &[]).unwrap();
	assert!(matches!(result, super::super::KeyResult::Cancelled));
}

#[test]
fn projects_other_keys_do_nothing() {
	let screen = projects_screen(vec![true, false], default_levels(2), 0);
	let (selected, _, cursor, ..) =
		unwrap_select_projects(handle_key(screen, KeyCode::Char('x'), &[]));
	assert_eq!(selected, vec![true, false]);
	assert_eq!(cursor, 0);
}

#[test]
fn projects_error_clears_on_navigation() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![false, false],
		levels: default_levels(2),
		cursor: 0,
		error: true,
		changed_count: 2,
	});
	let (_, _, cursor, error, _) = unwrap_select_projects(handle_key(screen, KeyCode::Down, &[]));
	assert_eq!(cursor, 1);
	assert!(!error);
}

#[test]
fn projects_error_clears_on_toggle() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![false, false],
		levels: default_levels(2),
		cursor: 0,
		error: true,
		changed_count: 2,
	});
	let (selected, _, _, error, _) =
		unwrap_select_projects(handle_key(screen, KeyCode::Char(' '), &[]));
	assert_eq!(selected, vec![true, false]);
	assert!(!error);
}

#[test]
fn projects_empty_navigation_keys_are_no_ops() {
	let _screen = projects_screen(vec![], vec![], 0);
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
		let result = handle_key(
			Screen::SelectProjects(super::super::SelectProjectsState {
				selected: vec![],
				levels: vec![],
				cursor: 0,
				error: false,
				changed_count: 0,
			}),
			key,
			&[],
		)
		.unwrap();
		match result {
			super::super::KeyResult::Continue(Screen::SelectProjects(
				super::super::SelectProjectsState { selected, .. },
			)) => {
				assert!(
					selected.is_empty(),
					"key {key:?} should be a no-op on empty projects"
				);
			}
			_ => panic!("key {key:?} should be a no-op on empty projects"),
		}
	}
}

#[test]
fn projects_empty_esc_cancels() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![],
		levels: vec![],
		cursor: 0,
		error: false,
		changed_count: 0,
	});
	let result = handle_key(screen, KeyCode::Esc, &[]).unwrap();
	assert!(matches!(result, super::super::KeyResult::Cancelled));
}

#[test]
fn projects_empty_q_cancels() {
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![],
		levels: vec![],
		cursor: 0,
		error: false,
		changed_count: 0,
	});
	let result = handle_key(screen, KeyCode::Char('q'), &[]).unwrap();
	assert!(matches!(result, super::super::KeyResult::Cancelled));
}

// --- Mouse tests ---

#[test]
fn mouse_click_on_changed_project_toggles_it() {
	let selected = vec![true, false];
	let levels = default_levels(2);
	// inner_row=1 → project[0], absolute row 7
	let result = click(10, 7, &selected, &levels, 0, 1);
	match result {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState { selected, .. },
		)) => {
			assert_eq!(selected[0], false);
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

#[test]
fn mouse_click_on_unchanged_project_toggles_it() {
	let selected = vec![true, false];
	let levels = default_levels(2);
	// inner_row=3 → project[1] (unchanged), absolute row 9
	let result = click(10, 9, &selected, &levels, 0, 1);
	match result {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState { selected, .. },
		)) => {
			assert_eq!(selected[1], true);
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

#[test]
fn mouse_click_on_changed_header_is_noop() {
	let selected = vec![true, false];
	let levels = default_levels(2);
	// inner_row=0 → "Changed" header, absolute row 6
	let result = click(10, 6, &selected, &levels, 0, 1);
	match result {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState { selected: sel, .. },
		)) => {
			assert_eq!(sel, vec![true, false]);
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

#[test]
fn mouse_click_on_unchanged_header_is_noop() {
	let selected = vec![true, false];
	let levels = default_levels(2);
	// inner_row=2 → "Unchanged" header, absolute row 8
	let result = click(10, 8, &selected, &levels, 0, 1);
	match result {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState { selected: sel, .. },
		)) => {
			assert_eq!(sel, vec![true, false]);
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

#[test]
fn mouse_click_outside_block_is_noop() {
	let selected = vec![true, false];
	let levels = default_levels(2);
	let result = click(10, 23, &selected, &levels, 0, 1);
	match result {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState { selected: sel, .. },
		)) => {
			assert_eq!(sel, vec![true, false]);
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

/// Clicks a level indicator on a selected project.
///
/// Layout (80×24 terminal, 2-cell margin):
///   inner_x_start = 3, prefix = 7, name_col_width = 0 → level_start = 12
///   Major: cols 12–18, Minor: cols 20–26, Patch: cols 28–34
fn click_level(col: u16, row: u16) -> super::HandleResult {
	let selected = vec![true];
	let levels = vec![ChangeType::Patch];
	click(col, row, &selected, &levels, 0, 1)
}

#[test]
fn mouse_click_on_major_indicator_sets_major() {
	// name_col_width=0, level_start=12, Major at offset 0–7 → col 12
	match click_level(12, 7) {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState { levels, .. },
		)) => {
			assert_eq!(levels[0], ChangeType::Major);
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

#[test]
fn mouse_click_on_minor_indicator_sets_minor() {
	// Minor at offset 8–15 → col 20
	match click_level(20, 7) {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState { levels, .. },
		)) => {
			assert_eq!(levels[0], ChangeType::Minor);
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

#[test]
fn mouse_click_on_patch_indicator_sets_patch() {
	// Patch at offset 16–23 → col 28
	match click_level(28, 7) {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState { levels, .. },
		)) => {
			assert_eq!(levels[0], ChangeType::Patch);
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

#[test]
fn mouse_click_on_level_of_unselected_project_toggles_instead() {
	// col 12 is in the level indicator area, but project is not selected
	let selected = vec![false];
	let levels = vec![ChangeType::Patch];
	match click(12, 7, &selected, &levels, 0, 1) {
		super::super::KeyResult::Continue(Screen::SelectProjects(
			super::super::SelectProjectsState {
				selected: sel,
				levels: lvl,
				..
			},
		)) => {
			assert_eq!(sel[0], true); // toggled on
			assert_eq!(lvl[0], ChangeType::Patch); // level unchanged
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

/// Catches deletion of the `MouseButton::Left` guard at line 376: a right-click
/// must not trigger any selection change.
#[test]
fn right_click_is_ignored() {
	let area = Rect::new(0, 0, 80, 24);
	let right_click = Event::Mouse(MouseEvent {
		kind: MouseEventKind::Down(MouseButton::Right),
		column: 10,
		row: 7,
		modifiers: KeyModifiers::NONE,
	});
	let state = super::super::SelectProjectsState {
		selected: vec![false, false],
		levels: vec![ChangeType::Patch; 2],
		cursor: 0,
		error: false,
		changed_count: 2,
	};
	let result = super::handle_event_select_projects(state, right_click, area, &[]);
	match result {
		super::super::KeyResult::Continue(Screen::SelectProjects(s)) => {
			assert!(!s.selected[0], "right-click must not toggle selection");
			assert!(!s.selected[1], "right-click must not toggle selection");
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

// --- row_to_project_index tests ---

#[test]
fn row_to_project_index_changed_header_is_none() {
	assert_eq!(super::row_to_project_index(0, 1, 2), None);
}

#[test]
fn row_to_project_index_unchanged_header_is_none() {
	assert_eq!(super::row_to_project_index(2, 1, 2), None);
}

#[test]
fn row_to_project_index_changed_projects() {
	assert_eq!(super::row_to_project_index(1, 2, 4), Some(0));
	assert_eq!(super::row_to_project_index(2, 2, 4), Some(1));
}

#[test]
fn row_to_project_index_unchanged_projects() {
	assert_eq!(super::row_to_project_index(3, 1, 3), Some(1));
	assert_eq!(super::row_to_project_index(4, 1, 3), Some(2));
}

#[test]
fn row_to_project_index_beyond_total_is_none() {
	assert_eq!(super::row_to_project_index(4, 1, 2), None);
}

#[test]
fn row_to_project_index_zero_changed_count() {
	assert_eq!(super::row_to_project_index(0, 0, 2), None);
	assert_eq!(super::row_to_project_index(1, 0, 2), None);
	assert_eq!(super::row_to_project_index(2, 0, 2), Some(0));
	assert_eq!(super::row_to_project_index(3, 0, 2), Some(1));
	assert_eq!(super::row_to_project_index(4, 0, 2), None);
}

#[test]
fn row_to_project_index_all_changed() {
	assert_eq!(super::row_to_project_index(0, 2, 2), None);
	assert_eq!(super::row_to_project_index(1, 2, 2), Some(0));
	assert_eq!(super::row_to_project_index(2, 2, 2), Some(1));
	assert_eq!(super::row_to_project_index(3, 2, 2), None);
	assert_eq!(super::row_to_project_index(4, 2, 2), None);
}

// --- Render tests ---

#[test]
fn ui_renders_select_projects_screen() {
	crate::locale::set_locale("en");
	let mut terminal = create_test_terminal();
	let projects = dummy_projects(2);
	let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![true, false],
		levels: default_levels(2),
		cursor: 0,
		error: false,
		changed_count: 2,
	});
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
fn ui_renders_selected_project_with_level() {
	crate::locale::set_locale("en");
	let mut terminal = create_test_terminal();
	let projects = dummy_projects(1);
	let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![true],
		levels: vec![ChangeType::Minor],
		cursor: 0,
		error: false,
		changed_count: 1,
	});
	terminal
		.draw(|frame| super::super::ui(frame, &screen, &names))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("minor"));
}

#[test]
fn ui_renders_group_headers() {
	crate::locale::set_locale("en");
	let mut terminal = create_test_terminal();
	let projects = dummy_projects(2);
	let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![true, false],
		levels: default_levels(2),
		cursor: 0,
		error: false,
		changed_count: 1,
	});
	terminal
		.draw(|frame| super::super::ui(frame, &screen, &names))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Changed"));
	assert!(content.contains("Unchanged"));
}

#[test]
fn ui_renders_group_headers_with_none_when_empty() {
	crate::locale::set_locale("en");
	let mut terminal = create_test_terminal();
	let projects = dummy_projects(1);
	let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![false],
		levels: default_levels(1),
		cursor: 0,
		error: false,
		changed_count: 0,
	});
	terminal
		.draw(|frame| super::super::ui(frame, &screen, &names))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Changed"));
	assert!(content.contains("(none)"));
}

#[test]
fn ui_renders_select_projects_error() {
	let mut terminal = create_test_terminal();
	let projects = dummy_projects(1);
	let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let screen = Screen::SelectProjects(super::super::SelectProjectsState {
		selected: vec![false],
		levels: default_levels(1),
		cursor: 0,
		error: true,
		changed_count: 1,
	});
	terminal
		.draw(|frame| super::super::ui(frame, &screen, &names))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Select at least one project"));
}
