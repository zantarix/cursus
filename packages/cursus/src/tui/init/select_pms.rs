use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::model::config::PackageManager;
use crate::tui::widgets::{self, KeyResult};

use super::{HandleResult, PmFocus, Screen, WizardState, advance_from_manifest_queue};

/// Commits the selected package managers to state and advances to the next screen.
///
/// For each selected PM whose manifest file is absent at the git root, a
/// [`Screen::ManifestPath`] prompt is queued. If all manifests are present the
/// wizard skips straight to [`Screen::EnableGit`].
pub(crate) fn advance_from_select_pms(
	mut state: WizardState,
	cargo: bool,
	npm: bool,
) -> (WizardState, Screen) {
	state.cargo_enabled = cargo;
	state.npm_enabled = npm;

	let mut remaining = Vec::new();
	if cargo
		&& !state
			.env
			.git()
			.path()
			.child("Cargo.toml")
			.as_path()
			.exists()
	{
		remaining.push(PackageManager::Cargo);
	}
	if npm
		&& !state
			.env
			.git()
			.path()
			.child("package.json")
			.as_path()
			.exists()
	{
		remaining.push(PackageManager::Npm);
	}
	state.remaining_manifest_pms = remaining;

	advance_from_manifest_queue(state)
}

fn toggle_pm_selection(cargo: bool, npm: bool, focus: PmFocus) -> (bool, bool) {
	match focus {
		PmFocus::Cargo => (!cargo, npm),
		PmFocus::Npm => (cargo, !npm),
	}
}

fn handle_key_select_pms(
	state: WizardState,
	cargo: bool,
	npm: bool,
	focus: PmFocus,
	code: KeyCode,
) -> HandleResult {
	match code {
		KeyCode::Left
		| KeyCode::Right
		| KeyCode::Tab
		| KeyCode::Char('h')
		| KeyCode::Char('l')
		| KeyCode::Up
		| KeyCode::Down
		| KeyCode::Char('j')
		| KeyCode::Char('k') => Ok(KeyResult::Continue((
			state,
			Screen::SelectPackageManagers {
				cargo,
				npm,
				focus: focus.toggle(),
			},
		))),
		KeyCode::Char(' ') => {
			let (new_cargo, new_npm) = toggle_pm_selection(cargo, npm, focus);
			Ok(KeyResult::Continue((
				state,
				Screen::SelectPackageManagers {
					cargo: new_cargo,
					npm: new_npm,
					focus,
				},
			)))
		}
		KeyCode::Enter => {
			if !cargo && !npm {
				Ok(KeyResult::Continue((
					state,
					Screen::SelectPackageManagers { cargo, npm, focus },
				)))
			} else {
				let (new_state, next_screen) = advance_from_select_pms(state, cargo, npm);
				Ok(KeyResult::Continue((new_state, next_screen)))
			}
		}
		KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
		_ => Ok(KeyResult::Continue((
			state,
			Screen::SelectPackageManagers { cargo, npm, focus },
		))),
	}
}

fn handle_mouse_select_pms(
	state: WizardState,
	cargo: bool,
	npm: bool,
	focus: PmFocus,
	col: u16,
	row: u16,
	content_area: Rect,
) -> HandleResult {
	let question = crate::t!("select-pms-question");
	let help = crate::t!("select-pms-help");
	let q_height = widgets::paragraph_height(&question, content_area.width, 2);
	let chunks = widgets::wizard_layout(
		content_area,
		&[
			Constraint::Length(q_height),
			Constraint::Min(1),
			Constraint::Length(widgets::paragraph_height(&help, content_area.width, 0)),
		],
	);
	let checkbox_area = chunks[1];
	let inner_y_start = checkbox_area.y + 1;
	let inner_y_end = checkbox_area.y + checkbox_area.height.saturating_sub(1);
	let inner_x_start = checkbox_area.x + 1;
	let inner_x_end = checkbox_area.x + checkbox_area.width.saturating_sub(1);
	if row < inner_y_start || row >= inner_y_end || col < inner_x_start || col >= inner_x_end {
		return Ok(KeyResult::Continue((
			state,
			Screen::SelectPackageManagers { cargo, npm, focus },
		)));
	}
	let clicked_focus = match row - inner_y_start {
		0 => PmFocus::Cargo,
		1 => PmFocus::Npm,
		_ => {
			return Ok(KeyResult::Continue((
				state,
				Screen::SelectPackageManagers { cargo, npm, focus },
			)));
		}
	};
	let (new_cargo, new_npm) = toggle_pm_selection(cargo, npm, clicked_focus);
	Ok(KeyResult::Continue((
		state,
		Screen::SelectPackageManagers {
			cargo: new_cargo,
			npm: new_npm,
			focus: clicked_focus,
		},
	)))
}

/// Handles events for the [`Screen::SelectPackageManagers`] screen.
pub(crate) fn handle_select_pms(
	state: WizardState,
	cargo: bool,
	npm: bool,
	focus: PmFocus,
	event: Event,
	content_area: Rect,
) -> HandleResult {
	match event {
		Event::Key(KeyEvent { code, .. }) => handle_key_select_pms(state, cargo, npm, focus, code),
		Event::Mouse(me) if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) => {
			handle_mouse_select_pms(state, cargo, npm, focus, me.column, me.row, content_area)
		}
		_ => Ok(KeyResult::Continue((
			state,
			Screen::SelectPackageManagers { cargo, npm, focus },
		))),
	}
}

/// Renders the [`Screen::SelectPackageManagers`] screen.
pub(crate) fn render_select_pms(
	frame: &mut Frame,
	area: Rect,
	cargo: bool,
	npm: bool,
	focus: PmFocus,
) {
	let question = crate::t!("select-pms-question");
	let help = crate::t!("select-pms-help");
	let help_h = widgets::paragraph_height(&help, area.width, 0);
	let chunks = widgets::wizard_layout(
		area,
		&[
			Constraint::Length(widgets::paragraph_height(&question, area.width, 2)),
			Constraint::Min(1),
			Constraint::Length(help_h),
		],
	);
	widgets::render_question(frame, chunks[0], &question, Color::Yellow);

	let cargo_style = if focus == PmFocus::Cargo {
		Style::default()
			.fg(Color::Cyan)
			.add_modifier(Modifier::BOLD)
	} else {
		Style::default().fg(Color::Gray)
	};
	let npm_style = if focus == PmFocus::Npm {
		Style::default()
			.fg(Color::Cyan)
			.add_modifier(Modifier::BOLD)
	} else {
		Style::default().fg(Color::Gray)
	};
	let cargo_check = if cargo { "[x]" } else { "[ ]" };
	let npm_check = if npm { "[x]" } else { "[ ]" };
	let content = vec![
		Line::from(Span::styled(
			format!("  {cargo_check} {}", crate::t!("cargo-label")),
			cargo_style,
		)),
		Line::from(Span::styled(
			format!("  {npm_check} {}", crate::t!("npm-label")),
			npm_style,
		)),
	];
	let list = Paragraph::new(content).block(
		Block::default()
			.borders(Borders::ALL)
			.title(crate::t!("select-pms-title")),
	);
	frame.render_widget(list, chunks[1]);

	widgets::render_help(frame, chunks[2], &help);
}
