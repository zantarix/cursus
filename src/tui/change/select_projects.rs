use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, Paragraph},
};

use crate::model::changeset::ChangeType;
use crate::package_manager::Project;
use crate::tui::widgets::{self, KeyResult, button_style};

use super::{BackState, HandleResult, Screen, SelectProjectsState, enter_message};

const HELP: &str = "↑/↓/j/k: navigate | Space: toggle | ←/→: change level | ,/.: set all levels | a: all | c: changed | u: unchanged | Enter: confirm | Esc: cancel";
const QUESTION: &str = "Which projects does this change apply to?";
const QUESTION_ERROR: &str = "Select at least one project to continue.";
const QUESTION_HEIGHT: u16 = 3;

fn new_screen(
	selected: &[bool],
	levels: &[ChangeType],
	cursor: usize,
	error: bool,
	changed_count: usize,
) -> Screen {
	Screen::SelectProjects(SelectProjectsState {
		selected: selected.to_vec(),
		levels: levels.to_vec(),
		cursor,
		error,
		changed_count,
	})
}

fn move_project_cursor(
	selected: &[bool],
	levels: &[ChangeType],
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
	KeyResult::Continue(new_screen(
		selected,
		levels,
		new_cursor,
		false,
		changed_count,
	))
}

fn advance_to_enter_message(
	selected: &[bool],
	levels: &[ChangeType],
	cursor: usize,
	changed_count: usize,
	projects: &[Project],
) -> HandleResult {
	if selected.iter().any(|&s| s) {
		let selected_projects: Vec<(Project, ChangeType)> = selected
			.iter()
			.enumerate()
			.filter(|(_, s)| **s)
			.map(|(i, _)| (projects[i].clone(), levels[i]))
			.collect();
		let back = BackState::MultiPackage(SelectProjectsState {
			selected: selected.to_vec(),
			levels: levels.to_vec(),
			cursor,
			error: false,
			changed_count,
		});
		let textarea = enter_message::initial_textarea();
		KeyResult::Continue(Screen::EnterMessage {
			textarea,
			projects: selected_projects,
			back,
		})
	} else {
		KeyResult::Continue(new_screen(selected, levels, cursor, true, changed_count))
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

/// Cycles the focused project's level by one step (forward or backward).
/// Has no effect when the focused project is not selected.
fn cycle_single_level(
	selected: &[bool],
	levels: &[ChangeType],
	cursor: usize,
	changed_count: usize,
	forward: bool,
) -> HandleResult {
	if selected[cursor] {
		let mut new_levels = levels.to_vec();
		new_levels[cursor] = if forward {
			new_levels[cursor].next()
		} else {
			new_levels[cursor].prev()
		};
		KeyResult::Continue(new_screen(
			selected,
			&new_levels,
			cursor,
			false,
			changed_count,
		))
	} else {
		KeyResult::Continue(new_screen(selected, levels, cursor, false, changed_count))
	}
}

/// Cycles all selected projects' levels together (forward or backward).
/// Uses the first selected project as the source level. No-op when nothing is selected.
fn cycle_all_levels(
	selected: &[bool],
	levels: &[ChangeType],
	cursor: usize,
	changed_count: usize,
	forward: bool,
) -> HandleResult {
	if let Some(first) = selected.iter().position(|&s| s) {
		let new_level = if forward {
			levels[first].next()
		} else {
			levels[first].prev()
		};
		let mut new_levels = levels.to_vec();
		selected
			.iter()
			.enumerate()
			.filter(|(_, s)| **s)
			.for_each(|(i, _)| new_levels[i] = new_level);
		KeyResult::Continue(new_screen(
			selected,
			&new_levels,
			cursor,
			false,
			changed_count,
		))
	} else {
		KeyResult::Continue(new_screen(selected, levels, cursor, false, changed_count))
	}
}

/// Handles Space / 'a' / 'c' / 'u' selection-toggle keys.
fn handle_selection_key(
	selected: &[bool],
	levels: &[ChangeType],
	cursor: usize,
	changed_count: usize,
	key: KeyCode,
	len: usize,
) -> HandleResult {
	let nc =
		|sel: &[bool]| KeyResult::Continue(new_screen(sel, levels, cursor, false, changed_count));
	match key {
		KeyCode::Char(' ') => {
			let mut s = selected.to_vec();
			s[cursor] = !s[cursor];
			nc(&s)
		}
		KeyCode::Char('a') => nc(&vec![!selected.iter().all(|&s| s); len]),
		KeyCode::Char('c') => {
			let end = changed_count.min(len);
			if end == 0 {
				nc(selected)
			} else {
				nc(&toggle_group(selected, 0, end))
			}
		}
		KeyCode::Char('u') => {
			let start = changed_count.min(len);
			if start >= len {
				nc(selected)
			} else {
				nc(&toggle_group(selected, start, len))
			}
		}
		_ => nc(selected),
	}
}

fn handle_key_inner(
	selected: &[bool],
	levels: &[ChangeType],
	cursor: usize,
	changed_count: usize,
	key: KeyCode,
	projects: &[Project],
) -> HandleResult {
	let len = selected.len();
	if len == 0 {
		return match key {
			KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
			_ => KeyResult::Continue(new_screen(&[], &[], 0, false, changed_count)),
		};
	}
	match key {
		KeyCode::Up | KeyCode::Char('k') => {
			move_project_cursor(selected, levels, cursor, changed_count, true)
		}
		KeyCode::Down | KeyCode::Char('j') => {
			move_project_cursor(selected, levels, cursor, changed_count, false)
		}
		KeyCode::Left | KeyCode::Char('h') => {
			cycle_single_level(selected, levels, cursor, changed_count, false)
		}
		KeyCode::Right | KeyCode::Char('l') => {
			cycle_single_level(selected, levels, cursor, changed_count, true)
		}
		KeyCode::Char(',') => cycle_all_levels(selected, levels, cursor, changed_count, false),
		KeyCode::Char('.') => cycle_all_levels(selected, levels, cursor, changed_count, true),
		KeyCode::Char(' ' | 'a' | 'c' | 'u') => {
			handle_selection_key(selected, levels, cursor, changed_count, key, len)
		}
		KeyCode::Enter => {
			advance_to_enter_message(selected, levels, cursor, changed_count, projects)
		}
		KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancelled,
		_ => KeyResult::Continue(new_screen(selected, levels, cursor, false, changed_count)),
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

/// Tries to map a click column to a level indicator, returning the chosen
/// level if the click lands within the three-label strip.
fn click_level_at(col: u16, inner_x_start: u16, name_col_width: usize) -> Option<ChangeType> {
	// "   [x] " = 7 chars + padded name + "  " separator = 9 + name_col_width
	let level_start = inner_x_start + 9 + name_col_width as u16;
	if col < level_start {
		return None;
	}
	// Each indicator: " level " (7 chars) + " " (separator) = 8 chars
	match (col - level_start) / 8 {
		0 => Some(ChangeType::Major),
		1 => Some(ChangeType::Minor),
		2 => Some(ChangeType::Patch),
		_ => None,
	}
}

/// Returns `(inner_y_start, inner_y_end, inner_x_start, inner_x_end)` for the
/// project-list block inside the wizard layout.
fn project_block_inner_bounds(content_area: Rect) -> (u16, u16, u16, u16) {
	let help_h = widgets::paragraph_height(HELP, content_area.width, 0);
	let chunks = widgets::wizard_layout(
		content_area,
		&[
			Constraint::Length(QUESTION_HEIGHT),
			Constraint::Min(5),
			Constraint::Length(help_h),
		],
	);
	let b = chunks[1];
	(
		b.y + 1,
		b.y + b.height.saturating_sub(1),
		b.x + 1,
		b.x + b.width.saturating_sub(1),
	)
}

fn handle_mouse_select_projects(
	state: &SelectProjectsState,
	col: u16,
	row: u16,
	content_area: Rect,
	name_col_width: usize,
) -> HandleResult {
	let (inner_y_start, inner_y_end, inner_x_start, inner_x_end) =
		project_block_inner_bounds(content_area);
	let no_change = || {
		KeyResult::Continue(new_screen(
			&state.selected,
			&state.levels,
			state.cursor,
			false,
			state.changed_count,
		))
	};
	if row < inner_y_start || row >= inner_y_end || col < inner_x_start || col >= inner_x_end {
		return no_change();
	}
	let inner_row = row - inner_y_start;
	let total = state.selected.len();
	match row_to_project_index(inner_row, state.changed_count, total) {
		Some(project_idx) => {
			if state.selected[project_idx]
				&& let Some(lvl) = click_level_at(col, inner_x_start, name_col_width)
			{
				let mut new_levels = state.levels.to_vec();
				new_levels[project_idx] = lvl;
				return KeyResult::Continue(new_screen(
					&state.selected,
					&new_levels,
					project_idx,
					false,
					state.changed_count,
				));
			}
			let mut new_selected = state.selected.to_vec();
			new_selected[project_idx] = !new_selected[project_idx];
			KeyResult::Continue(new_screen(
				&new_selected,
				&state.levels,
				project_idx,
				false,
				state.changed_count,
			))
		}
		None => no_change(),
	}
}

/// Handles events for the [`Screen::SelectProjects`] screen.
pub(super) fn handle_event_select_projects(
	state: SelectProjectsState,
	event: Event,
	content_area: Rect,
	projects: &[Project],
) -> HandleResult {
	let SelectProjectsState {
		selected,
		levels,
		cursor,
		changed_count,
		..
	} = state;
	// error is cleared on any action (the field is intentionally ignored here)
	match event {
		Event::Key(KeyEvent { code, .. }) => {
			handle_key_inner(&selected, &levels, cursor, changed_count, code, projects)
		}
		Event::Mouse(me) if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) => {
			let name_col_width = projects.iter().map(|p| p.name().len()).max().unwrap_or(0);
			handle_mouse_select_projects(
				&SelectProjectsState {
					selected,
					levels,
					cursor,
					error: false,
					changed_count,
				},
				me.column,
				me.row,
				content_area,
				name_col_width,
			)
		}
		_ => KeyResult::Continue(new_screen(&selected, &levels, cursor, false, changed_count)),
	}
}

fn level_indicator(current: ChangeType) -> Vec<Span<'static>> {
	[ChangeType::Major, ChangeType::Minor, ChangeType::Patch]
		.into_iter()
		.flat_map(|l| {
			[
				Span::styled(format!(" {l} "), button_style(l == current)),
				Span::raw(" "),
			]
		})
		.collect()
}

fn project_line(
	name: &str,
	name_col_width: usize,
	level: ChangeType,
	is_selected: bool,
	is_cursor: bool,
) -> Line<'static> {
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
	let padded = format!("{:<width$}", name, width = name_col_width);
	if is_selected {
		let mut spans = vec![Span::styled(format!("   {checkbox} {padded}  "), style)];
		spans.extend(level_indicator(level));
		Line::from(spans)
	} else {
		Line::from(Span::styled(format!("   {checkbox} {padded}"), style))
	}
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
	levels: &[ChangeType],
	cursor: usize,
	changed_count: usize,
) -> Vec<Line<'static>> {
	let total = project_names.len();
	let effective_changed = changed_count.min(total);
	let unchanged_count = total - effective_changed;
	let name_col_width = project_names.iter().map(|n| n.len()).max().unwrap_or(0);
	let mut lines: Vec<Line<'static>> = Vec::new();
	lines.push(group_header("Changed", effective_changed));
	for i in 0..effective_changed {
		lines.push(project_line(
			project_names[i],
			name_col_width,
			levels[i],
			selected[i],
			i == cursor,
		));
	}
	lines.push(group_header("Unchanged", unchanged_count));
	for i in effective_changed..total {
		lines.push(project_line(
			project_names[i],
			name_col_width,
			levels[i],
			selected[i],
			i == cursor,
		));
	}
	lines
}

/// Renders the [`Screen::SelectProjects`] screen.
pub(super) fn render_select_projects(
	frame: &mut Frame,
	area: Rect,
	project_names: &[&str],
	state: &SelectProjectsState,
) {
	let question_text = if state.error {
		QUESTION_ERROR
	} else {
		QUESTION
	};
	let question_color = if state.error {
		Color::Red
	} else {
		Color::Yellow
	};
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
	let lines = build_project_lines(
		project_names,
		&state.selected,
		&state.levels,
		state.cursor,
		state.changed_count,
	);
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
		let (_, _, cursor, ..) =
			unwrap_select_projects(handle_key(screen, KeyCode::Char('k'), &[]));
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
		let (_, _, cursor, ..) =
			unwrap_select_projects(handle_key(screen, KeyCode::Char('j'), &[]));
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
		let (_, _, cursor, error, _) =
			unwrap_select_projects(handle_key(screen, KeyCode::Down, &[]));
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
}
