use crossterm::event::KeyCode;
use ratatui_textarea::TextArea;

use crate::forge::gitlab::remote::GitLabProject;

use crate::tui::init::edit_gitlab::*;
use crate::tui::init::test_helpers::*;
use crate::tui::init::{Screen, handle_key};

fn state(
	project: &str,
	host: &str,
	self_managed: bool,
	focus: GitLabFocus,
	error: bool,
) -> Box<EditGitLabState> {
	let mut project_textarea = TextArea::default();
	if !project.is_empty() {
		project_textarea.insert_str(project);
	}
	let mut host_textarea = TextArea::default();
	if !host.is_empty() {
		host_textarea.insert_str(host);
	}
	Box::new(EditGitLabState {
		project_textarea,
		host_textarea,
		self_managed,
		focus,
		error,
	})
}

fn empty_screen() -> Screen {
	Screen::EditGitLab(state("", "", false, GitLabFocus::Project, false))
}

fn screen_with_project(text: &str) -> Screen {
	Screen::EditGitLab(state(text, "", false, GitLabFocus::Project, false))
}

#[test]
fn empty_project_advances_with_none() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, s) = unwrap_continue(handle_key(state, empty_screen(), key(KeyCode::Enter)));
	assert_eq!(new_state.gitlab_group, None);
	assert_eq!(new_state.gitlab_project, None);
	assert_eq!(new_state.gitlab_host, None);
	assert!(matches!(s, Screen::OpenEditor(_)));
}

#[test]
fn valid_group_project_advances_with_explicit_values() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, s) = unwrap_continue(handle_key(
		state,
		screen_with_project("acme/my-app"),
		key(KeyCode::Enter),
	));
	assert_eq!(new_state.gitlab_group, Some("acme".to_string()));
	assert_eq!(new_state.gitlab_project, Some("my-app".to_string()));
	assert_eq!(new_state.gitlab_host, None);
	assert!(matches!(s, Screen::OpenEditor(_)));
}

#[test]
fn subgroup_path_parses_group_and_project() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (new_state, _) = unwrap_continue(handle_key(
		state,
		screen_with_project("acme/sub/app"),
		key(KeyCode::Enter),
	));
	assert_eq!(new_state.gitlab_group, Some("acme/sub".to_string()));
	assert_eq!(new_state.gitlab_project, Some("app".to_string()));
}

#[test]
fn no_slash_shows_error() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (_, s) = unwrap_continue(handle_key(
		state,
		screen_with_project("notvalid"),
		key(KeyCode::Enter),
	));
	assert!(matches!(s, Screen::EditGitLab(b) if b.error));
}

#[test]
fn invalid_chars_show_error() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (_, s) = unwrap_continue(handle_key(
		state,
		screen_with_project("bad group/app"),
		key(KeyCode::Enter),
	));
	assert!(matches!(s, Screen::EditGitLab(b) if b.error));
}

#[test]
fn unmodified_detected_value_leaves_fields_none() {
	let dir = temp_dir();
	let mut state = make_state(&dir);
	state.detected_gitlab = Some(GitLabProject::new("gitlab.com", "acme", "my-app").unwrap());
	let screen = screen_with_project("acme/my-app");
	let (new_state, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert_eq!(new_state.gitlab_group, None);
	assert_eq!(new_state.gitlab_project, None);
	assert!(matches!(s, Screen::OpenEditor(_)));
}

#[test]
fn unmodified_detected_self_managed_remote_leaves_fields_none() {
	let dir = temp_dir();
	let mut state = make_state(&dir);
	state.detected_gitlab = Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap());
	// `make_edit_gitlab_screen` pre-populates this exact state for a self-managed remote.
	let screen = Screen::EditGitLab(self::state(
		"acme/app",
		"gitlab.example.com",
		true,
		GitLabFocus::Project,
		false,
	));
	let (new_state, s) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
	assert_eq!(new_state.gitlab_group, None);
	assert_eq!(new_state.gitlab_project, None);
	assert_eq!(new_state.gitlab_host, None);
	assert!(matches!(s, Screen::OpenEditor(_)));
}

#[test]
fn self_managed_with_empty_host_is_an_error() {
	let dir = temp_dir();
	let wstate = make_state(&dir);
	// The "self-managed" affordance is checked but the host wasn't filled in —
	// contradictory, so refuse to advance.
	let screen = Screen::EditGitLab(state("acme/app", "", true, GitLabFocus::Host, false));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::Enter)));
	assert!(matches!(s, Screen::EditGitLab(b) if b.error));
}

#[test]
fn make_screen_hides_host_for_gitlab_com() {
	let dir = temp_dir();
	let mut state = make_state(&dir);
	state.detected_gitlab = Some(GitLabProject::new("gitlab.com", "acme", "app").unwrap());
	let screen = make_edit_gitlab_screen(&state);
	match screen {
		Screen::EditGitLab(s) => {
			assert!(!s.self_managed);
			assert_eq!(s.project_textarea.lines(), vec!["acme/app"]);
		}
		_ => panic!("expected EditGitLab"),
	}
}

#[test]
fn make_screen_shows_host_for_self_managed_remote() {
	let dir = temp_dir();
	let mut state = make_state(&dir);
	state.detected_gitlab = Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap());
	let screen = make_edit_gitlab_screen(&state);
	match screen {
		Screen::EditGitLab(s) => {
			assert!(s.self_managed);
			assert_eq!(s.host_textarea.lines(), vec!["gitlab.example.com"]);
		}
		_ => panic!("expected EditGitLab"),
	}
}

#[test]
fn tab_cycles_focus_skipping_hidden_host() {
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let (_, s) = unwrap_continue(handle_key(wstate, empty_screen(), key(KeyCode::Tab)));
	assert!(matches!(&s, Screen::EditGitLab(b) if b.focus == GitLabFocus::SelfManaged));

	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state("", "", false, GitLabFocus::SelfManaged, false));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::Tab)));
	assert!(matches!(&s, Screen::EditGitLab(b) if b.focus == GitLabFocus::Project));
}

#[test]
fn tab_visits_host_when_self_managed() {
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state("", "", true, GitLabFocus::SelfManaged, false));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::Tab)));
	assert!(matches!(&s, Screen::EditGitLab(b) if b.focus == GitLabFocus::Host));
}

#[test]
fn back_tab_cycles_focus_in_reverse() {
	// Project → SelfManaged → Host when self_managed; reverse should go
	// Project → Host → SelfManaged → Project.
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state("", "", true, GitLabFocus::Project, false));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::BackTab)));
	assert!(matches!(&s, Screen::EditGitLab(b) if b.focus == GitLabFocus::Host));

	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state("", "", true, GitLabFocus::Host, false));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::BackTab)));
	assert!(matches!(&s, Screen::EditGitLab(b) if b.focus == GitLabFocus::SelfManaged));

	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state("", "", false, GitLabFocus::Project, false));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::BackTab)));
	// With self_managed off, host is hidden, so reverse from Project is SelfManaged.
	assert!(matches!(&s, Screen::EditGitLab(b) if b.focus == GitLabFocus::SelfManaged));
}

#[test]
fn space_on_self_managed_toggles_on() {
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state("", "", false, GitLabFocus::SelfManaged, false));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::Char(' '))));
	assert!(
		matches!(&s, Screen::EditGitLab(b) if b.self_managed && b.focus == GitLabFocus::SelfManaged)
	);
}

#[test]
fn space_on_self_managed_toggles_off_and_clears_host() {
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state(
		"",
		"gitlab.example.com",
		true,
		GitLabFocus::SelfManaged,
		false,
	));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::Char(' '))));
	match s {
		Screen::EditGitLab(b) => {
			assert!(!b.self_managed);
			assert!(matches!(b.focus, GitLabFocus::Project));
			assert!(b.host_textarea.is_empty());
		}
		_ => panic!("expected EditGitLab"),
	}
}

#[test]
fn self_managed_with_host_advances_and_sets_gitlab_host() {
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state(
		"acme/app",
		"gitlab.example.com",
		true,
		GitLabFocus::Host,
		false,
	));
	let (new_state, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::Enter)));
	assert_eq!(new_state.gitlab_group, Some("acme".to_string()));
	assert_eq!(new_state.gitlab_project, Some("app".to_string()));
	assert_eq!(
		new_state.gitlab_host,
		Some("gitlab.example.com".to_string())
	);
	assert!(matches!(s, Screen::OpenEditor(_)));
}

#[test]
fn invalid_host_shows_error() {
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state(
		"acme/app",
		"not a host",
		true,
		GitLabFocus::Host,
		false,
	));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::Enter)));
	assert!(matches!(s, Screen::EditGitLab(b) if b.error));
}

#[test]
fn esc_cancels() {
	let dir = temp_dir();
	let state = make_state(&dir);
	assert_cancelled(handle_key(state, empty_screen(), key(KeyCode::Esc)));
}

#[test]
fn typing_in_project_field_inserts_character() {
	let dir = temp_dir();
	let state = make_state(&dir);
	let (_, s) = unwrap_continue(handle_key(state, empty_screen(), key(KeyCode::Char('a'))));
	match s {
		Screen::EditGitLab(b) => assert_eq!(b.project_textarea.lines(), vec!["a"]),
		_ => panic!("expected EditGitLab"),
	}
}

#[test]
fn typing_in_host_field_inserts_character_when_self_managed() {
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state("", "", true, GitLabFocus::Host, false));
	let (_, s) = unwrap_continue(handle_key(wstate, screen, key(KeyCode::Char('g'))));
	match s {
		Screen::EditGitLab(b) => assert_eq!(b.host_textarea.lines(), vec!["g"]),
		_ => panic!("expected EditGitLab"),
	}
}

#[test]
fn ui_renders_edit_gitlab() {
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let wstate = make_state(&dir);
	terminal
		.draw(|frame| crate::tui::init::ui(frame, &wstate, &empty_screen()))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("group/project"));
	assert!(content.contains("Self-managed"));
}

#[test]
fn ui_renders_edit_gitlab_with_host_when_self_managed() {
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state("", "", true, GitLabFocus::Host, false));
	terminal
		.draw(|frame| crate::tui::init::ui(frame, &wstate, &screen))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("GitLab host"));
}

#[test]
fn ui_renders_edit_gitlab_error() {
	crate::locale::set_locale("en");
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
	let mut terminal = create_test_terminal();
	let dir = temp_dir();
	let wstate = make_state(&dir);
	let screen = Screen::EditGitLab(state("", "", false, GitLabFocus::Project, true));
	terminal
		.draw(|frame| crate::tui::init::ui(frame, &wstate, &screen))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Invalid"));
}
