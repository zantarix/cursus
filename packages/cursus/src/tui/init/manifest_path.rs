use crossterm::event::{Event, KeyCode};
use ratatui::prelude::*;
use ratatui_textarea::TextArea;

use crate::model::config::PackageManager;
use crate::tui::widgets::{self, KeyResult};

use super::{HandleResult, Screen, WizardState, advance_from_manifest_queue};

/// Handles events for the [`Screen::ManifestPath`] screen.
pub(super) fn handle_manifest_path(
	mut state: WizardState,
	pm: PackageManager,
	mut textarea: TextArea<'static>,
	event: Event,
) -> HandleResult {
	match event {
		Event::Key(key) => match key.code {
			KeyCode::Enter => {
				let text = textarea.lines().first().cloned().unwrap_or_default();
				let trimmed = text.trim().to_string();
				let path = if trimmed.is_empty() {
					None
				} else {
					Some(trimmed)
				};
				match pm {
					PackageManager::Cargo => state.cargo_path = path,
					PackageManager::Npm => state.npm_path = path,
				}
				let (new_state, next_screen) = advance_from_manifest_queue(state);
				Ok(KeyResult::Continue((new_state, next_screen)))
			}
			KeyCode::Esc => Ok(KeyResult::Cancelled),
			_ => {
				textarea.input(key);
				Ok(KeyResult::Continue((
					state,
					Screen::ManifestPath { pm, textarea },
				)))
			}
		},
		_ => Ok(KeyResult::Continue((
			state,
			Screen::ManifestPath { pm, textarea },
		))),
	}
}

/// Renders the [`Screen::ManifestPath`] screen.
pub(super) fn render_manifest_path(
	frame: &mut Frame,
	area: Rect,
	pm: PackageManager,
	textarea: &TextArea<'static>,
) {
	let pm_name = match pm {
		PackageManager::Cargo => "Cargo.toml",
		PackageManager::Npm => "package.json",
	};
	let question = crate::t!("manifest-path-question", "manifest" => pm_name);
	let chunks = widgets::wizard_layout(
		area,
		&[
			Constraint::Length(widgets::paragraph_height(&question, area.width, 2)),
			Constraint::Length(3),
			Constraint::Min(1),
		],
	);
	widgets::render_question(frame, chunks[0], &question, Color::Yellow);
	frame.render_widget(textarea, chunks[1]);
	widgets::render_help(frame, chunks[2], &crate::t!("manifest-path-help"));
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;
	use ratatui_textarea::TextArea;

	use crate::model::config::PackageManager;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};

	#[test]
	fn manifest_path_enter_stores_path_and_advances() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let mut textarea = TextArea::default();
		textarea.insert_str("backend/");
		let screen = Screen::ManifestPath {
			pm: PackageManager::Cargo,
			textarea,
		};
		let (new_state, next) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert_eq!(new_state.cargo_path, Some("backend/".to_string()));
		assert!(matches!(next, Screen::EnableGit(_)));
	}

	#[test]
	fn manifest_path_enter_with_empty_text_stores_none() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ManifestPath {
			pm: PackageManager::Npm,
			textarea: TextArea::default(),
		};
		let (new_state, _) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert_eq!(new_state.npm_path, None);
	}

	#[test]
	fn manifest_path_esc_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ManifestPath {
			pm: PackageManager::Cargo,
			textarea: TextArea::default(),
		};
		assert_cancelled(handle_key(state, screen, key(KeyCode::Esc)));
	}

	#[test]
	fn manifest_path_q_key_types_character_not_cancel() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::ManifestPath {
			pm: PackageManager::Cargo,
			textarea: TextArea::default(),
		};
		let (_, next) = unwrap_continue(handle_key(state, screen, key(KeyCode::Char('q'))));
		assert!(matches!(next, Screen::ManifestPath { .. }));
	}

	#[test]
	fn manifest_path_advances_to_second_pm_when_both_missing() {
		let dir = temp_dir();
		let mut state = make_state(&dir);
		state.remaining_manifest_pms = vec![PackageManager::Npm];
		let screen = Screen::ManifestPath {
			pm: PackageManager::Cargo,
			textarea: TextArea::default(),
		};
		let (_, next) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert!(matches!(
			next,
			Screen::ManifestPath {
				pm: PackageManager::Npm,
				..
			}
		));
	}

	#[test]
	fn ui_renders_manifest_path() {
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
					&Screen::ManifestPath {
						pm: PackageManager::Cargo,
						textarea: TextArea::default(),
					},
				)
			})
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Cargo.toml"));
	}
}
