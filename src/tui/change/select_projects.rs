use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, Paragraph},
};

use crate::tui::widgets::{self, KeyResult};

use super::{HandleResult, Screen};

const HELP: &str = "↑/↓/j/k: navigate | Space: toggle | a: all | c: changed | u: unchanged | Enter: confirm | Esc: cancel";
const QUESTION: &str = "Which projects does this change apply to?";
const QUESTION_ERROR: &str = "Select at least one project to continue.";
const QUESTION_HEIGHT: u16 = 3;

fn new_screen(selected: &[bool], cursor: usize, error: bool, changed_count: usize) -> Screen {
	Screen::SelectProjects {
		selected: selected.to_vec(),
		cursor,
		error,
		changed_count,
	}
}

fn move_project_cursor(
	selected: &[bool],
	cursor: usize,
	changed_count: usize,
	up: bool,
) -> HandleResult {
	let len = selected.len();
	let new_cursor = if up {
		if cursor == 0 { len - 1 } else { cursor - 1 }
	} else if cursor + 1 >= len {
		0
	} else {
		cursor + 1
	};
	KeyResult::Continue(new_screen(selected, new_cursor, false, changed_count))
}

fn advance_to_change_type(selected: &[bool], cursor: usize, changed_count: usize) -> HandleResult {
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
		KeyResult::Continue(new_screen(selected, cursor, true, changed_count))
	}
}

/// Toggles all entries in `selected[start..end]`: if all are on, turns them
/// off; otherwise turns them all on. Returns the updated vec.
fn toggle_group(selected: &[bool], start: usize, end: usize) -> Vec<bool> {
	let all_on = selected[start..end].iter().all(|&s| s);
	let mut new_selected = selected.to_vec();
	for s in new_selected[start..end].iter_mut() {
		*s = !all_on;
	}
	new_selected
}

fn handle_key_inner(
	selected: &[bool],
	cursor: usize,
	changed_count: usize,
	key: KeyCode,
) -> HandleResult {
	let len = selected.len();
	if len == 0 {
		return match key {
			KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
			_ => KeyResult::Continue(new_screen(&[], 0, false, changed_count)),
		};
	}
	match key {
		KeyCode::Up | KeyCode::Char('k') => {
			move_project_cursor(selected, cursor, changed_count, true)
		}
		KeyCode::Down | KeyCode::Char('j') => {
			move_project_cursor(selected, cursor, changed_count, false)
		}
		KeyCode::Char(' ') => {
			let mut new_selected = selected.to_vec();
			new_selected[cursor] = !new_selected[cursor];
			KeyResult::Continue(new_screen(&new_selected, cursor, false, changed_count))
		}
		KeyCode::Char('a') => {
			let all_on = selected.iter().all(|&s| s);
			let new_selected = vec![!all_on; len];
			KeyResult::Continue(new_screen(&new_selected, cursor, false, changed_count))
		}
		KeyCode::Char('c') => {
			let end = changed_count.min(len);
			if end == 0 {
				KeyResult::Continue(new_screen(selected, cursor, false, changed_count))
			} else {
				let new_sel = toggle_group(selected, 0, end);
				KeyResult::Continue(new_screen(&new_sel, cursor, false, changed_count))
			}
		}
		KeyCode::Char('u') => {
			let start = changed_count.min(len);
			if start >= len {
				KeyResult::Continue(new_screen(selected, cursor, false, changed_count))
			} else {
				let new_sel = toggle_group(selected, start, len);
				KeyResult::Continue(new_screen(&new_sel, cursor, false, changed_count))
			}
		}
		KeyCode::Enter => advance_to_change_type(selected, cursor, changed_count),
		KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
		_ => KeyResult::Continue(new_screen(selected, cursor, false, changed_count)),
	}
}

/// Maps an inner-block row to a project index, accounting for group headers.
///
/// Layout within the bordered block's inner area:
/// - Row 0: "Changed" group header (not a project)
/// - Rows 1..=changed_count: changed projects at indices `0..changed_count`
/// - Row changed_count + 1: "Unchanged" group header (not a project)
/// - Rows changed_count+2..: unchanged projects at indices `changed_count..total`
fn row_to_project_index(inner_row: u16, changed_count: usize, total: usize) -> Option<usize> {
	let row = inner_row as usize;
	if row == 0 {
		return None; // "Changed" header
	}
	if row <= changed_count {
		return Some(row - 1);
	}
	if row == changed_count + 1 {
		return None; // "Unchanged" header
	}
	let project_idx = changed_count + (row - changed_count - 2);
	if project_idx < total {
		Some(project_idx)
	} else {
		None
	}
}

fn handle_mouse_select_projects(
	selected: &[bool],
	cursor: usize,
	changed_count: usize,
	col: u16,
	row: u16,
	content_area: Rect,
) -> HandleResult {
	let help_h = widgets::paragraph_height(HELP, content_area.width, 0);
	let chunks = widgets::wizard_layout(
		content_area,
		&[
			Constraint::Length(QUESTION_HEIGHT),
			Constraint::Min(5),
			Constraint::Length(help_h),
		],
	);
	let block_area = chunks[1];
	let inner_y_start = block_area.y + 1;
	let inner_y_end = block_area.y + block_area.height.saturating_sub(1);
	let inner_x_start = block_area.x + 1;
	let inner_x_end = block_area.x + block_area.width.saturating_sub(1);
	let no_change = || KeyResult::Continue(new_screen(selected, cursor, false, changed_count));
	if row < inner_y_start || row >= inner_y_end || col < inner_x_start || col >= inner_x_end {
		return no_change();
	}
	let inner_row = row - inner_y_start;
	let total = selected.len();
	match row_to_project_index(inner_row, changed_count, total) {
		Some(project_idx) => {
			let mut new_selected = selected.to_vec();
			new_selected[project_idx] = !new_selected[project_idx];
			KeyResult::Continue(new_screen(&new_selected, project_idx, false, changed_count))
		}
		None => no_change(),
	}
}

/// Handles events for the [`Screen::SelectProjects`] screen.
pub(super) fn handle_event_select_projects(
	selected: &[bool],
	cursor: usize,
	changed_count: usize,
	event: Event,
	content_area: Rect,
) -> HandleResult {
	match event {
		Event::Key(KeyEvent { code, .. }) => {
			handle_key_inner(selected, cursor, changed_count, code)
		}
		Event::Mouse(me) if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) => {
			handle_mouse_select_projects(
				selected,
				cursor,
				changed_count,
				me.column,
				me.row,
				content_area,
			)
		}
		_ => KeyResult::Continue(new_screen(selected, cursor, false, changed_count)),
	}
}

fn project_line(name: &str, is_selected: bool, is_cursor: bool) -> Line<'static> {
	let checkbox = if is_selected { "[x]" } else { "[ ]" };
	let style = if is_cursor {
		Style::default()
			.fg(Color::Cyan)
			.add_modifier(Modifier::BOLD)
	} else if is_selected {
		Style::default().fg(Color::Green)
	} else {
		Style::default().fg(Color::Gray)
	};
	Line::from(Span::styled(format!("   {checkbox} {name}"), style))
}

fn group_header(label: &'static str, count: usize) -> Line<'static> {
	let header_style = Style::default()
		.fg(Color::Yellow)
		.add_modifier(Modifier::BOLD);
	if count == 0 {
		Line::from(vec![
			Span::styled(format!("  {label} "), header_style),
			Span::styled("(none)", Style::default().add_modifier(Modifier::DIM)),
		])
	} else {
		Line::from(Span::styled(format!("  {label} ({count})"), header_style))
	}
}

fn build_project_lines(
	project_names: &[&str],
	selected: &[bool],
	cursor: usize,
	changed_count: usize,
) -> Vec<Line<'static>> {
	let total = project_names.len();
	let effective_changed = changed_count.min(total);
	let unchanged_count = total - effective_changed;
	let mut lines: Vec<Line<'static>> = Vec::new();
	lines.push(group_header("Changed", effective_changed));
	for i in 0..effective_changed {
		lines.push(project_line(project_names[i], selected[i], i == cursor));
	}
	lines.push(group_header("Unchanged", unchanged_count));
	for i in effective_changed..total {
		lines.push(project_line(project_names[i], selected[i], i == cursor));
	}
	lines
}

/// Renders the [`Screen::SelectProjects`] screen.
pub(super) fn render_select_projects(
	frame: &mut Frame,
	area: Rect,
	project_names: &[&str],
	selected: &[bool],
	cursor: usize,
	error: bool,
	changed_count: usize,
) {
	let question_text = if error { QUESTION_ERROR } else { QUESTION };
	let question_color = if error { Color::Red } else { Color::Yellow };
	let help_h = widgets::paragraph_height(HELP, area.width, 0);
	let chunks = widgets::wizard_layout(
		area,
		&[
			Constraint::Length(QUESTION_HEIGHT),
			Constraint::Min(5),
			Constraint::Length(help_h),
		],
	);
	widgets::render_question(frame, chunks[0], question_text, question_color);
	let lines = build_project_lines(project_names, selected, cursor, changed_count);
	let para = Paragraph::new(lines).block(
		Block::default()
			.borders(Borders::ALL)
			.title("Select Projects"),
	);
	frame.render_widget(para, chunks[1]);
	widgets::render_help(frame, chunks[2], HELP);
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;
	use ratatui::prelude::Rect;

	use crate::tui::test_utils::{buffer_to_string, create_test_terminal, mouse_click};

	use super::super::test_helpers::dummy_projects;
	use super::super::{Screen, handle_key};

	fn projects_screen(selected: Vec<bool>, cursor: usize) -> Screen {
		let changed_count = selected.len(); // treat all as "changed" for backward compat
		Screen::SelectProjects {
			selected,
			cursor,
			error: false,
			changed_count,
		}
	}

	/// Simulates a mouse click on the projects screen using an 80×24 terminal area.
	fn click(
		col: u16,
		row: u16,
		selected: &[bool],
		cursor: usize,
		changed_count: usize,
	) -> super::HandleResult {
		let area = Rect::new(0, 0, 80, 24);
		super::handle_event_select_projects(
			selected,
			cursor,
			changed_count,
			mouse_click(col, row),
			area,
		)
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
	fn projects_c_toggles_changed_group_on() {
		// changed_count=2 means first 2 are "Changed"
		let screen = Screen::SelectProjects {
			selected: vec![false, false, true],
			cursor: 0,
			error: false,
			changed_count: 2,
		};
		let result = handle_key(&screen, KeyCode::Char('c'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![true, true, true],
				cursor: 0,
				error: false,
				changed_count: 2,
			})
		);
	}

	#[test]
	fn projects_c_toggles_changed_group_off_when_all_on() {
		let screen = Screen::SelectProjects {
			selected: vec![true, true, false],
			cursor: 0,
			error: false,
			changed_count: 2,
		};
		let result = handle_key(&screen, KeyCode::Char('c'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![false, false, false],
				cursor: 0,
				error: false,
				changed_count: 2,
			})
		);
	}

	#[test]
	fn projects_c_with_zero_changed_count_is_noop() {
		let screen = Screen::SelectProjects {
			selected: vec![false, false],
			cursor: 0,
			error: false,
			changed_count: 0,
		};
		let result = handle_key(&screen, KeyCode::Char('c'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![false, false],
				cursor: 0,
				error: false,
				changed_count: 0,
			})
		);
	}

	#[test]
	fn projects_u_toggles_unchanged_group_on() {
		let screen = Screen::SelectProjects {
			selected: vec![true, false, false],
			cursor: 0,
			error: false,
			changed_count: 1,
		};
		let result = handle_key(&screen, KeyCode::Char('u'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![true, true, true],
				cursor: 0,
				error: false,
				changed_count: 1,
			})
		);
	}

	#[test]
	fn projects_u_toggles_unchanged_group_off_when_all_on() {
		let screen = Screen::SelectProjects {
			selected: vec![false, true, true],
			cursor: 0,
			error: false,
			changed_count: 1,
		};
		let result = handle_key(&screen, KeyCode::Char('u'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![false, false, false],
				cursor: 0,
				error: false,
				changed_count: 1,
			})
		);
	}

	#[test]
	fn projects_u_with_all_changed_is_noop() {
		let screen = Screen::SelectProjects {
			selected: vec![true, true],
			cursor: 0,
			error: false,
			changed_count: 2,
		};
		let result = handle_key(&screen, KeyCode::Char('u'), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![true, true],
				cursor: 0,
				error: false,
				changed_count: 2,
			})
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
				changed_count: 3,
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
			changed_count: 2,
		};
		let result = handle_key(&screen, KeyCode::Down, &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![false, false],
				cursor: 1,
				error: false,
				changed_count: 2,
			})
		);
	}

	#[test]
	fn projects_error_clears_on_toggle() {
		let screen = Screen::SelectProjects {
			selected: vec![false, false],
			cursor: 0,
			error: true,
			changed_count: 2,
		};
		let result = handle_key(&screen, KeyCode::Char(' '), &[]).unwrap();
		assert_eq!(
			result,
			super::super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![true, false],
				cursor: 0,
				error: false,
				changed_count: 2,
			})
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

	// --- Mouse tests ---
	//
	// Terminal: 80×24. wizard_layout margin=2 → inner starts at (2,2).
	// Layout: [question h=3, block min=5, help h=dynamic]
	// chunks[0] (question): y=2, height=3
	// chunks[1] (block): y=5, height = 24-2-2-3-help_h
	//   HELP text at width 76 wraps to 2 lines; help_h = 2
	//   block height = 24-4-3-2 = 15
	// Block inner: y_start = block.y+1 = 6, y_end = block.y+15-1 = 19
	// inner_x_start = 2+1 = 3, inner_x_end = 2+76-1 = 77
	//
	// For changed_count=1:
	//   inner_row 0 (abs row 6): "Changed" header
	//   inner_row 1 (abs row 7): project[0] (changed)
	//   inner_row 2 (abs row 8): "Unchanged" header
	//   inner_row 3 (abs row 9): project[1] (unchanged)

	#[test]
	fn mouse_click_on_changed_project_toggles_it() {
		let selected = vec![true, false];
		// inner_row=1 → project[0], absolute row 7
		let result = click(10, 7, &selected, 0, 1);
		assert_eq!(
			result,
			super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![false, false],
				cursor: 0,
				error: false,
				changed_count: 1,
			})
		);
	}

	#[test]
	fn mouse_click_on_unchanged_project_toggles_it() {
		let selected = vec![true, false];
		// inner_row=3 → project[1] (unchanged), absolute row 9
		let result = click(10, 9, &selected, 0, 1);
		assert_eq!(
			result,
			super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![true, true],
				cursor: 1,
				error: false,
				changed_count: 1,
			})
		);
	}

	#[test]
	fn mouse_click_on_changed_header_is_noop() {
		let selected = vec![true, false];
		// inner_row=0 → "Changed" header, absolute row 6
		let result = click(10, 6, &selected, 0, 1);
		assert_eq!(
			result,
			super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![true, false],
				cursor: 0,
				error: false,
				changed_count: 1,
			})
		);
	}

	#[test]
	fn mouse_click_on_unchanged_header_is_noop() {
		let selected = vec![true, false];
		// inner_row=2 → "Unchanged" header, absolute row 8
		let result = click(10, 8, &selected, 0, 1);
		assert_eq!(
			result,
			super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![true, false],
				cursor: 0,
				error: false,
				changed_count: 1,
			})
		);
	}

	#[test]
	fn mouse_click_outside_block_is_noop() {
		let selected = vec![true, false];
		// Click far below the block (row 23, inside margin)
		let result = click(10, 23, &selected, 0, 1);
		assert_eq!(
			result,
			super::KeyResult::Continue(Screen::SelectProjects {
				selected: vec![true, false],
				cursor: 0,
				error: false,
				changed_count: 1,
			})
		);
	}

	// --- row_to_project_index tests ---

	#[test]
	fn row_to_project_index_changed_header_is_none() {
		assert_eq!(super::row_to_project_index(0, 1, 2), None);
	}

	#[test]
	fn row_to_project_index_unchanged_header_is_none() {
		// With changed_count=1, unchanged header is at inner_row = changed_count+1 = 2
		assert_eq!(super::row_to_project_index(2, 1, 2), None);
	}

	#[test]
	fn row_to_project_index_changed_projects() {
		// changed_count=2, total=4: rows 1 and 2 are project[0] and project[1]
		assert_eq!(super::row_to_project_index(1, 2, 4), Some(0));
		assert_eq!(super::row_to_project_index(2, 2, 4), Some(1));
	}

	#[test]
	fn row_to_project_index_unchanged_projects() {
		// changed_count=1, total=3: unchanged header at row 2, projects at rows 3 and 4
		assert_eq!(super::row_to_project_index(3, 1, 3), Some(1));
		assert_eq!(super::row_to_project_index(4, 1, 3), Some(2));
	}

	#[test]
	fn row_to_project_index_beyond_total_is_none() {
		// changed_count=1, total=2: rows 0-3 valid (headers+projects), row 4 is beyond
		assert_eq!(super::row_to_project_index(4, 1, 2), None);
	}

	#[test]
	fn row_to_project_index_zero_changed_count() {
		// All unchanged: Changed header at 0, Unchanged header at 1, projects at 2+
		assert_eq!(super::row_to_project_index(0, 0, 2), None); // Changed header
		assert_eq!(super::row_to_project_index(1, 0, 2), None); // Unchanged header
		assert_eq!(super::row_to_project_index(2, 0, 2), Some(0));
		assert_eq!(super::row_to_project_index(3, 0, 2), Some(1));
		assert_eq!(super::row_to_project_index(4, 0, 2), None); // beyond total
	}

	#[test]
	fn row_to_project_index_all_changed() {
		// changed_count=total=2: rows 1-2 are projects, row 3 is Unchanged header, row 4+ beyond
		assert_eq!(super::row_to_project_index(0, 2, 2), None); // Changed header
		assert_eq!(super::row_to_project_index(1, 2, 2), Some(0));
		assert_eq!(super::row_to_project_index(2, 2, 2), Some(1));
		assert_eq!(super::row_to_project_index(3, 2, 2), None); // Unchanged header
		assert_eq!(super::row_to_project_index(4, 2, 2), None); // beyond total
	}

	// --- Render tests ---

	#[test]
	fn ui_renders_select_projects_screen() {
		let mut terminal = create_test_terminal();
		let projects = dummy_projects(2);
		let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
		let screen = Screen::SelectProjects {
			selected: vec![true, false],
			cursor: 0,
			error: false,
			changed_count: 2,
		};
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
	fn ui_renders_group_headers() {
		let mut terminal = create_test_terminal();
		let projects = dummy_projects(2);
		let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
		// project-0 changed, project-1 unchanged
		let screen = Screen::SelectProjects {
			selected: vec![true, false],
			cursor: 0,
			error: false,
			changed_count: 1,
		};
		terminal
			.draw(|frame| super::super::ui(frame, &screen, &names))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Changed"));
		assert!(content.contains("Unchanged"));
	}

	#[test]
	fn ui_renders_group_headers_with_none_when_empty() {
		let mut terminal = create_test_terminal();
		let projects = dummy_projects(1);
		let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
		// All projects unchanged (changed_count=0)
		let screen = Screen::SelectProjects {
			selected: vec![false],
			cursor: 0,
			error: false,
			changed_count: 0,
		};
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
		let screen = Screen::SelectProjects {
			selected: vec![false],
			cursor: 0,
			error: true,
			changed_count: 1,
		};
		terminal
			.draw(|frame| super::super::ui(frame, &screen, &names))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Select at least one project"));
	}
}
