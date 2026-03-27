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
pub(super) fn advance_from_select_pms(
	mut state: WizardState,
	cargo: bool,
	npm: bool,
) -> (WizardState, Screen) {
	state.cargo_enabled = cargo;
	state.npm_enabled = npm;

	let fs = state.env.fs();
	let mut remaining = Vec::new();
	if cargo && !fs.exists(&state.env.git().path().child("Cargo.toml")) {
		remaining.push(PackageManager::Cargo);
	}
	if npm && !fs.exists(&state.env.git().path().child("package.json")) {
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
pub(super) fn handle_select_pms(
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
pub(super) fn render_select_pms(
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

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use crate::model::config::PackageManager;

	use super::super::test_helpers::*;
	use super::super::{PmFocus, Screen, handle_key};
	use super::*;

	#[test]
	fn select_pms_tab_moves_focus() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: true,
			npm: false,
			focus: PmFocus::Cargo,
		};
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Tab)));
		assert!(matches!(
			s,
			Screen::SelectPackageManagers {
				focus: PmFocus::Npm,
				..
			}
		));
	}

	#[test]
	fn select_pms_space_toggles_focused_item() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: true,
			npm: false,
			focus: PmFocus::Cargo,
		};
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Char(' '))));
		assert!(matches!(
			s,
			Screen::SelectPackageManagers { cargo: false, .. }
		));
	}

	#[test]
	fn select_pms_space_toggles_npm_when_focused() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: false,
			npm: false,
			focus: PmFocus::Npm,
		};
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Char(' '))));
		assert!(matches!(s, Screen::SelectPackageManagers { npm: true, .. }));
	}

	#[test]
	fn select_pms_enter_with_none_selected_does_not_advance() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: false,
			npm: false,
			focus: PmFocus::Cargo,
		};
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert!(matches!(s, Screen::SelectPackageManagers { .. }));
	}

	#[test]
	fn select_pms_enter_with_cargo_advances_to_enable_git_when_manifest_exists() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: true,
			npm: false,
			focus: PmFocus::Cargo,
		};
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert!(matches!(s, Screen::EnableGit(_)));
	}

	#[test]
	fn select_pms_enter_with_cargo_shows_manifest_path_when_missing() {
		let dir = temp_dir(); // No Cargo.toml
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: true,
			npm: false,
			focus: PmFocus::Cargo,
		};
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert!(matches!(
			s,
			Screen::ManifestPath {
				pm: PackageManager::Cargo,
				..
			}
		));
	}

	#[test]
	fn select_pms_esc_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: true,
			npm: false,
			focus: PmFocus::Cargo,
		};
		assert_cancelled(handle_key(state, screen, key(KeyCode::Esc)));
	}

	#[test]
	fn select_pms_click_cargo_row_toggles_cargo() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		// wizard_layout margin=2: inner starts at y=area.y+2=5
		// question block: y=5, height=3
		// checkbox block: y=8, fills remaining → inner y_start=9
		// Cargo row (relative 0): absolute row = 9
		let (_, s) = unwrap_continue(handle_select_pms(
			state,
			false,
			false,
			PmFocus::Npm,
			mouse_click(10, area.y + 6),
			area,
		));
		assert!(matches!(
			s,
			Screen::SelectPackageManagers {
				cargo: true,
				focus: PmFocus::Cargo,
				..
			}
		));
	}

	#[test]
	fn select_pms_click_npm_row_toggles_npm() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		// NPM row (relative 1): absolute row = 10 = area.y + 7
		let (_, s) = unwrap_continue(handle_select_pms(
			state,
			false,
			false,
			PmFocus::Cargo,
			mouse_click(10, area.y + 7),
			area,
		));
		assert!(matches!(
			s,
			Screen::SelectPackageManagers {
				npm: true,
				focus: PmFocus::Npm,
				..
			}
		));
	}

	#[test]
	fn select_pms_click_outside_checkboxes_does_nothing() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (_, s) = unwrap_continue(handle_select_pms(
			state,
			false,
			true,
			PmFocus::Cargo,
			mouse_click(10, area.y + 20),
			area,
		));
		assert!(matches!(
			s,
			Screen::SelectPackageManagers {
				cargo: false,
				npm: true,
				focus: PmFocus::Cargo,
				..
			}
		));
	}

	#[test]
	fn select_pms_click_empty_row_in_checkbox_block_does_nothing() {
		// The checkbox block fills remaining space with borders → many inner rows.
		// Only rows 0 (Cargo) and 1 (NPM) have content; the rest are blank.
		// Clicking a blank row must not toggle anything.
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		// Blank row 2 inside the checkbox inner area (area.y+2+3+1+2 = area.y+8)
		let (_, s) = unwrap_continue(handle_select_pms(
			state,
			false,
			true,
			PmFocus::Cargo,
			mouse_click(10, area.y + 8),
			area,
		));
		assert!(matches!(
			s,
			Screen::SelectPackageManagers {
				cargo: false,
				npm: true,
				focus: PmFocus::Cargo,
				..
			}
		));
	}

	/// Catches the `&&`→`||` mutation at line 31:
	/// `if npm && !package_json.exists()` — when package.json already exists,
	/// npm must NOT be queued for manifest prompting.
	/// With `||`, `npm || !exists()` = `true || false = true` → queued (wrong).
	#[test]
	fn select_pms_npm_with_existing_package_json_skips_manifest_prompt() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("package.json"), "{}").unwrap();
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: false,
			npm: true,
			focus: PmFocus::Npm,
		};
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		// Should advance to EnableGit, not ManifestPath
		assert!(
			matches!(s, Screen::EnableGit(_)),
			"package.json exists → no manifest prompt expected"
		);
	}

	#[test]
	fn ui_renders_select_pms() {
		crate::locale::set_locale("en");
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| {
				super::super::ui(
					frame,
					&state,
					&Screen::SelectPackageManagers {
						cargo: true,
						npm: false,
						focus: PmFocus::Cargo,
					},
				)
			})
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Package Managers"));
		assert!(content.contains("Cargo"));
		assert!(content.contains("NPM"));
	}
}
