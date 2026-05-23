use crossterm::event::{Event, KeyCode};
use ratatui::prelude::*;
use ratatui_textarea::TextArea;

use crate::forge::github::GitHubRepo;
use crate::tui::widgets::{self, KeyResult};

use super::{HandleResult, Screen, WizardState, bordered_textarea};

/// Builds the [`Screen::EditGitHub`] screen pre-populated with the detected owner/repo.
pub(crate) fn make_edit_github_screen(state: &WizardState) -> Screen {
	let mut textarea = bordered_textarea();
	if let Some(ref gh) = state.detected_github {
		textarea.insert_str(format!("{}/{}", gh.owner, gh.repo));
	}
	Screen::EditGitHub {
		textarea,
		error: false,
	}
}

fn handle_enter(mut state: WizardState, textarea: TextArea<'static>) -> HandleResult {
	let text = textarea.lines().first().cloned().unwrap_or_default();
	let trimmed = text.trim().to_string();
	let matches_detected = state
		.detected_github
		.as_ref()
		.is_some_and(|gh| format!("{}/{}", gh.owner, gh.repo) == trimmed);
	if trimmed.is_empty() || matches_detected {
		state.github_owner = None;
		state.github_repo = None;
		return Ok(KeyResult::Continue((state, Screen::OpenEditor(false))));
	}
	let Some((owner, repo)) = trimmed.split_once('/') else {
		return Ok(KeyResult::Continue((
			state,
			Screen::EditGitHub {
				textarea,
				error: true,
			},
		)));
	};
	match GitHubRepo::new(owner, repo) {
		Ok(gh) => {
			state.github_owner = Some(gh.owner);
			state.github_repo = Some(gh.repo);
			Ok(KeyResult::Continue((state, Screen::OpenEditor(false))))
		}
		Err(_) => Ok(KeyResult::Continue((
			state,
			Screen::EditGitHub {
				textarea,
				error: true,
			},
		))),
	}
}

/// Handles events for the [`Screen::EditGitHub`] screen.
///
/// On Enter, accepts empty input (auto-detect at runtime) or `owner/repo` format.
/// If the text matches the pre-populated detected value it is treated as empty so
/// the template renders it as a commented-out hint rather than an explicit value.
pub(crate) fn handle_edit_github(
	state: WizardState,
	mut textarea: TextArea<'static>,
	error: bool,
	event: Event,
) -> HandleResult {
	match event {
		Event::Key(key) => match key.code {
			KeyCode::Enter => handle_enter(state, textarea),
			KeyCode::Esc => Ok(KeyResult::Cancelled),
			_ => {
				textarea.input(key);
				Ok(KeyResult::Continue((
					state,
					Screen::EditGitHub {
						textarea,
						error: false,
					},
				)))
			}
		},
		_ => Ok(KeyResult::Continue((
			state,
			Screen::EditGitHub { textarea, error },
		))),
	}
}

/// Renders the [`Screen::EditGitHub`] screen.
pub(crate) fn render_edit_github(
	frame: &mut Frame,
	area: Rect,
	textarea: &TextArea<'static>,
	error: bool,
) {
	let question = if error {
		crate::t!("edit-github-invalid-question")
	} else {
		crate::t!("edit-github-question")
	};
	let color = if error { Color::Red } else { Color::Yellow };
	let chunks = widgets::wizard_layout(
		area,
		&[
			Constraint::Length(widgets::paragraph_height(&question, area.width, 2)),
			Constraint::Length(3),
			Constraint::Min(1),
		],
	);
	widgets::render_question(frame, chunks[0], &question, color);
	frame.render_widget(textarea, chunks[1]);
	widgets::render_help(frame, chunks[2], &crate::t!("edit-github-help"));
}
