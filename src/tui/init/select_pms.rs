use crossterm::event::{KeyCode, KeyEvent};
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

	let mut remaining = Vec::new();
	if cargo && !state.git_workdir.join("Cargo.toml").exists() {
		remaining.push(PackageManager::Cargo);
	}
	if npm && !state.git_workdir.join("package.json").exists() {
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

/// Handles key events for the [`Screen::SelectPackageManagers`] screen.
pub(super) fn handle_select_pms(
	state: WizardState,
	cargo: bool,
	npm: bool,
	focus: PmFocus,
	key: KeyEvent,
) -> HandleResult {
	match key.code {
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

/// Renders the [`Screen::SelectPackageManagers`] screen.
pub(super) fn render_select_pms(frame: &mut Frame, cargo: bool, npm: bool, focus: PmFocus) {
	let chunks = widgets::wizard_layout(
		frame,
		&[
			Constraint::Length(3),
			Constraint::Length(6),
			Constraint::Min(1),
		],
	);
	widgets::render_question(
		frame,
		chunks[0],
		"Which package managers does this project use?",
		Color::Yellow,
	);

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
		Line::from(Span::styled(format!("  {cargo_check} Cargo"), cargo_style)),
		Line::from(Span::styled(format!("  {npm_check} NPM"), npm_style)),
	];
	let list = Paragraph::new(content).block(
		Block::default()
			.borders(Borders::ALL)
			.title("Package Managers"),
	);
	frame.render_widget(list, chunks[1]);

	widgets::render_help(
		frame,
		chunks[2],
		"↑/↓/Tab: move focus | Space: toggle | Enter: confirm (≥1 required) | Esc: cancel",
	);
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use crate::model::config::PackageManager;

	use super::super::test_helpers::*;
	use super::super::{PmFocus, Screen, handle_key};

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
	fn ui_renders_select_pms() {
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
