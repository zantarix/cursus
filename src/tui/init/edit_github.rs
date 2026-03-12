use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui_textarea::TextArea;

use crate::github::GitHubRepo;
use crate::tui::widgets::{self, KeyResult};

use super::{HandleResult, Screen, WizardState, bordered_textarea};

/// Builds the [`Screen::EditGitHub`] screen pre-populated with the detected owner/repo.
pub(super) fn make_edit_github_screen(state: &WizardState) -> Screen {
	let mut textarea = bordered_textarea();
	if let Some(ref gh) = state.detected_github {
		textarea.insert_str(format!("{}/{}", gh.owner, gh.repo));
	}
	Screen::EditGitHub {
		textarea,
		error: false,
	}
}

/// Handles key events for the [`Screen::EditGitHub`] screen.
///
/// On Enter, accepts empty input (auto-detect at runtime) or `owner/repo` format.
/// If the text matches the pre-populated detected value it is treated as empty so
/// the template renders it as a commented-out hint rather than an explicit value.
pub(super) fn handle_edit_github(
	mut state: WizardState,
	mut textarea: TextArea<'static>,
	_error: bool,
	key: KeyEvent,
) -> HandleResult {
	match key.code {
		KeyCode::Enter => {
			let text = textarea.lines().first().cloned().unwrap_or_default();
			let trimmed = text.trim().to_string();
			let matches_detected = state
				.detected_github
				.as_ref()
				.is_some_and(|gh| format!("{}/{}", gh.owner, gh.repo) == trimmed);
			if trimmed.is_empty() || matches_detected {
				// Empty or unchanged auto-detected value: let Chronicle auto-detect at runtime.
				state.github_owner = None;
				state.github_repo = None;
				Ok(KeyResult::Continue((state, Screen::OpenEditor(false))))
			} else if let Some((owner, repo)) = trimmed.split_once('/') {
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
			} else {
				// No slash → invalid format
				Ok(KeyResult::Continue((
					state,
					Screen::EditGitHub {
						textarea,
						error: true,
					},
				)))
			}
		}
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
	}
}

/// Renders the [`Screen::EditGitHub`] screen.
pub(super) fn render_edit_github(frame: &mut Frame, textarea: &TextArea<'static>, error: bool) {
	let chunks = widgets::wizard_layout(
		frame,
		&[
			Constraint::Length(3),
			Constraint::Length(3),
			Constraint::Min(1),
		],
	);
	let question = if error {
		"Invalid format. Enter owner/repo (e.g. acme/my-app), or leave empty:"
	} else {
		"GitHub repository (owner/repo, e.g. acme/my-app, or leave empty):"
	};
	let color = if error { Color::Red } else { Color::Yellow };
	widgets::render_question(frame, chunks[0], question, color);
	frame.render_widget(textarea, chunks[1]);
	widgets::render_help(frame, chunks[2], "Enter: confirm | Esc: cancel");
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;
	use ratatui_textarea::TextArea;

	use crate::github::GitHubRepo;

	use super::super::test_helpers::*;
	use super::super::{Screen, WizardState, handle_key};

	#[test]
	fn edit_github_empty_advances_with_no_owner_repo() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::EditGitHub {
			textarea: TextArea::default(),
			error: false,
		};
		let (new_state, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert_eq!(new_state.github_owner, None);
		assert_eq!(new_state.github_repo, None);
		assert!(matches!(s, Screen::OpenEditor(_)));
	}

	#[test]
	fn edit_github_valid_owner_repo_advances() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let mut textarea = TextArea::default();
		textarea.insert_str("acme/my-app");
		let screen = Screen::EditGitHub {
			textarea,
			error: false,
		};
		let (new_state, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert_eq!(new_state.github_owner, Some("acme".to_string()));
		assert_eq!(new_state.github_repo, Some("my-app".to_string()));
		assert!(matches!(s, Screen::OpenEditor(_)));
	}

	#[test]
	fn edit_github_unmodified_detected_value_leaves_owner_repo_none() {
		let dir = temp_dir();
		let mut state = make_state(&dir);
		state.detected_github = Some(GitHubRepo {
			owner: "acme".to_string(),
			repo: "my-app".to_string(),
		});
		let mut textarea = TextArea::default();
		textarea.insert_str("acme/my-app");
		let screen = Screen::EditGitHub {
			textarea,
			error: false,
		};
		let (new_state, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert_eq!(new_state.github_owner, None);
		assert_eq!(new_state.github_repo, None);
		assert!(matches!(s, Screen::OpenEditor(_)));
	}

	#[test]
	fn edit_github_modified_detected_value_sets_explicit_owner_repo() {
		let dir = temp_dir();
		let mut state: WizardState = make_state(&dir);
		state.detected_github = Some(GitHubRepo {
			owner: "acme".to_string(),
			repo: "my-app".to_string(),
		});
		let mut textarea = TextArea::default();
		textarea.insert_str("acme/other-repo");
		let screen = Screen::EditGitHub {
			textarea,
			error: false,
		};
		let (new_state, _) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert_eq!(new_state.github_owner, Some("acme".to_string()));
		assert_eq!(new_state.github_repo, Some("other-repo".to_string()));
	}

	#[test]
	fn edit_github_invalid_no_slash_shows_error() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let mut textarea = TextArea::default();
		textarea.insert_str("notvalid");
		let screen = Screen::EditGitHub {
			textarea,
			error: false,
		};
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert!(matches!(s, Screen::EditGitHub { error: true, .. }));
	}

	#[test]
	fn edit_github_invalid_chars_shows_error() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let mut textarea = TextArea::default();
		textarea.insert_str("bad owner/repo");
		let screen = Screen::EditGitHub {
			textarea,
			error: false,
		};
		let (_, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert!(matches!(s, Screen::EditGitHub { error: true, .. }));
	}

	#[test]
	fn edit_github_esc_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::EditGitHub {
			textarea: TextArea::default(),
			error: false,
		};
		assert_cancelled(handle_key(state, screen, key(KeyCode::Esc)));
	}

	#[test]
	fn edit_github_q_key_types_character_not_cancel() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::EditGitHub {
			textarea: TextArea::default(),
			error: false,
		};
		let (_, next) = unwrap_continue(handle_key(state, screen, key(KeyCode::Char('q'))));
		assert!(matches!(next, Screen::EditGitHub { .. }));
	}

	#[test]
	fn ui_renders_edit_github() {
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| {
				super::super::ui(
					frame,
					&state,
					&Screen::EditGitHub {
						textarea: TextArea::default(),
						error: false,
					},
				)
			})
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("owner/repo"));
	}

	#[test]
	fn ui_renders_edit_github_error() {
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| {
				super::super::ui(
					frame,
					&state,
					&Screen::EditGitHub {
						textarea: TextArea::default(),
						error: true,
					},
				)
			})
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Invalid"));
	}
}
