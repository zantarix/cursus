use crossterm::event::{Event, KeyCode};
use ratatui::prelude::*;
use ratatui_textarea::TextArea;

use crate::model::config::PackageManager;
use crate::tui::widgets::{self, KeyResult};

use super::{HandleResult, Screen, WizardState, advance_from_manifest_queue};

/// Handles events for the [`Screen::ManifestPath`] screen.
pub(crate) fn handle_manifest_path(
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
pub(crate) fn render_manifest_path(
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
