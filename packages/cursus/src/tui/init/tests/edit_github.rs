use crossterm::event::KeyCode;
use ratatui_textarea::TextArea;

use crate::forge::github::GitHubRepo;

use crate::tui::init::test_helpers::*;
use crate::tui::init::{Screen, WizardState, handle_key};

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
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let state = make_state(&dir);
	terminal
		.draw(|frame| {
			crate::tui::init::ui(
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
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let state = make_state(&dir);
	terminal
		.draw(|frame| {
			crate::tui::init::ui(
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
