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
mod tests;
