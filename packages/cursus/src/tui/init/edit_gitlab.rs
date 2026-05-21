use crossterm::event::{Event, KeyCode};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui_textarea::TextArea;

use crate::forge::gitlab::remote::GitLabProject;
use crate::tui::widgets::{self, KeyResult};

use super::{HandleResult, Screen, WizardState, bordered_textarea};

/// Focus position within the [`Screen::EditGitLab`] screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::init) enum GitLabFocus {
	Project,
	SelfManaged,
	Host,
}

/// State carried by the [`Screen::EditGitLab`] screen.
///
/// Wrapped in [`Box`] inside the [`Screen`] enum to keep the enum compact
/// (the two embedded `TextArea`s are large).
#[derive(Debug)]
pub(in crate::tui::init) struct EditGitLabState {
	pub project_textarea: TextArea<'static>,
	pub host_textarea: TextArea<'static>,
	pub self_managed: bool,
	pub focus: GitLabFocus,
	pub error: bool,
}

fn editing_state(
	project_textarea: TextArea<'static>,
	host_textarea: TextArea<'static>,
	self_managed: bool,
	focus: GitLabFocus,
	error: bool,
) -> Box<EditGitLabState> {
	Box::new(EditGitLabState {
		project_textarea,
		host_textarea,
		self_managed,
		focus,
		error,
	})
}

/// Builds the [`Screen::EditGitLab`] screen pre-populated from any detected GitLab remote.
///
/// When the detected remote points at a host other than `gitlab.com`, the
/// self-managed checkbox starts on and the host textarea is pre-populated.
pub(super) fn make_edit_gitlab_screen(state: &WizardState) -> Screen {
	let mut project_textarea = bordered_textarea();
	let mut host_textarea = bordered_textarea();
	let mut self_managed = false;
	if let Some(ref gl) = state.detected_gitlab {
		project_textarea.insert_str(format!("{}/{}", gl.group, gl.project));
		if gl.host != "gitlab.com" {
			self_managed = true;
			host_textarea.insert_str(&gl.host);
		}
	}
	Screen::EditGitLab(editing_state(
		project_textarea,
		host_textarea,
		self_managed,
		GitLabFocus::Project,
		false,
	))
}

fn split_group_project(input: &str) -> Option<(&str, &str)> {
	input.rsplit_once('/')
}

fn invalid_screen(s: &EditGitLabState) -> Screen {
	Screen::EditGitLab(editing_state(
		s.project_textarea.clone(),
		s.host_textarea.clone(),
		s.self_managed,
		GitLabFocus::Project,
		true,
	))
}

fn handle_enter(mut state: WizardState, s: Box<EditGitLabState>) -> HandleResult {
	let project_trimmed = s
		.project_textarea
		.lines()
		.first()
		.cloned()
		.unwrap_or_default()
		.trim()
		.to_string();
	let host_trimmed = s
		.host_textarea
		.lines()
		.first()
		.cloned()
		.unwrap_or_default()
		.trim()
		.to_string();

	// "Unchanged auto-detect hint" path: the user pressed Enter without editing
	// the pre-populated detected values. Treat as None so the rendered config
	// shows commented-out hints rather than baking the detected values in.
	let matches_detected = state.detected_gitlab.as_ref().is_some_and(|gl| {
		format!("{}/{}", gl.group, gl.project) == project_trimmed
			&& (if s.self_managed {
				host_trimmed == gl.host
			} else {
				gl.host == "gitlab.com"
			})
	});
	if project_trimmed.is_empty() || matches_detected {
		state.gitlab_group = None;
		state.gitlab_project = None;
		state.gitlab_host = None;
		return Ok(KeyResult::Continue((state, Screen::OpenEditor(false))));
	}

	let Some((group, project)) = split_group_project(&project_trimmed) else {
		return Ok(KeyResult::Continue((state, invalid_screen(&s))));
	};

	// Self-managed checked + empty host is contradictory — refuse rather than
	// silently fall back to gitlab.com.
	if s.self_managed && host_trimmed.is_empty() {
		return Ok(KeyResult::Continue((state, invalid_screen(&s))));
	}

	let host_for_validation = if s.self_managed {
		host_trimmed.clone()
	} else {
		"gitlab.com".to_string()
	};

	match GitLabProject::new(host_for_validation, group, project) {
		Ok(parsed) => {
			state.gitlab_group = Some(parsed.group);
			state.gitlab_project = Some(parsed.project);
			state.gitlab_host = s.self_managed.then_some(parsed.host);
			Ok(KeyResult::Continue((state, Screen::OpenEditor(false))))
		}
		Err(_) => Ok(KeyResult::Continue((state, invalid_screen(&s)))),
	}
}

fn cycle_focus(focus: GitLabFocus, self_managed: bool) -> GitLabFocus {
	match focus {
		GitLabFocus::Project => GitLabFocus::SelfManaged,
		GitLabFocus::SelfManaged if self_managed => GitLabFocus::Host,
		GitLabFocus::SelfManaged => GitLabFocus::Project,
		GitLabFocus::Host => GitLabFocus::Project,
	}
}

fn cycle_focus_back(focus: GitLabFocus, self_managed: bool) -> GitLabFocus {
	match focus {
		GitLabFocus::Project if self_managed => GitLabFocus::Host,
		GitLabFocus::Project => GitLabFocus::SelfManaged,
		GitLabFocus::SelfManaged => GitLabFocus::Project,
		GitLabFocus::Host => GitLabFocus::SelfManaged,
	}
}

fn toggle_self_managed(s: Box<EditGitLabState>) -> Box<EditGitLabState> {
	let now_on = !s.self_managed;
	let (focus, host_textarea) = if now_on {
		(GitLabFocus::SelfManaged, s.host_textarea)
	} else {
		(GitLabFocus::Project, bordered_textarea())
	};
	editing_state(s.project_textarea, host_textarea, now_on, focus, s.error)
}

fn pass_through_typing(
	mut s: Box<EditGitLabState>,
	key: crossterm::event::KeyEvent,
) -> Box<EditGitLabState> {
	match s.focus {
		GitLabFocus::Project => {
			s.project_textarea.input(key);
		}
		GitLabFocus::Host if s.self_managed => {
			s.host_textarea.input(key);
		}
		GitLabFocus::SelfManaged | GitLabFocus::Host => {}
	}
	s.error = false;
	s
}

/// Handles events for the [`Screen::EditGitLab`] screen.
///
/// On Enter the screen submits the form regardless of focus. Tab cycles focus
/// across the project field, the self-managed checkbox, and (when checked) the
/// host field. Space toggles the self-managed checkbox when it has focus.
pub(super) fn handle_edit_gitlab(
	state: WizardState,
	mut s: Box<EditGitLabState>,
	event: Event,
) -> HandleResult {
	let Event::Key(key) = event else {
		return Ok(KeyResult::Continue((state, Screen::EditGitLab(s))));
	};

	match key.code {
		KeyCode::Esc => Ok(KeyResult::Cancelled),
		KeyCode::Enter => handle_enter(state, s),
		KeyCode::Tab => {
			s.focus = cycle_focus(s.focus, s.self_managed);
			Ok(KeyResult::Continue((state, Screen::EditGitLab(s))))
		}
		KeyCode::BackTab => {
			s.focus = cycle_focus_back(s.focus, s.self_managed);
			Ok(KeyResult::Continue((state, Screen::EditGitLab(s))))
		}
		KeyCode::Char(' ') if s.focus == GitLabFocus::SelfManaged => Ok(KeyResult::Continue((
			state,
			Screen::EditGitLab(toggle_self_managed(s)),
		))),
		_ => Ok(KeyResult::Continue((
			state,
			Screen::EditGitLab(pass_through_typing(s, key)),
		))),
	}
}

fn checkbox_text(self_managed: bool) -> String {
	let marker = if self_managed { "[x]" } else { "[ ]" };
	format!("{marker} {}", crate::t!("edit-gitlab-self-managed-label"))
}

fn render_textarea_with_focus(
	frame: &mut Frame,
	area: Rect,
	textarea: &TextArea<'static>,
	focused: bool,
) {
	let mut block = Block::default().borders(Borders::ALL);
	if focused {
		block = block.border_style(Style::default().fg(Color::Yellow));
	}
	let mut widget = textarea.clone();
	widget.set_block(block);
	frame.render_widget(&widget, area);
}

fn build_constraints(area: Rect, question: &str, self_managed: bool) -> Vec<Constraint> {
	let mut constraints = vec![
		Constraint::Length(widgets::paragraph_height(question, area.width, 2)),
		Constraint::Length(3),
		Constraint::Length(1),
	];
	if self_managed {
		let host_question = crate::t!("edit-gitlab-host-question");
		constraints.push(Constraint::Length(widgets::paragraph_height(
			&host_question,
			area.width,
			2,
		)));
		constraints.push(Constraint::Length(3));
	}
	constraints.push(Constraint::Min(1));
	constraints
}

fn render_checkbox(frame: &mut Frame, area: Rect, focused: bool, self_managed: bool) {
	let style = if focused {
		Style::default()
			.fg(Color::Yellow)
			.add_modifier(Modifier::BOLD)
	} else {
		Style::default()
	};
	frame.render_widget(
		Paragraph::new(checkbox_text(self_managed)).style(style),
		area,
	);
}

/// Renders the [`Screen::EditGitLab`] screen.
pub(super) fn render_edit_gitlab(frame: &mut Frame, area: Rect, s: &EditGitLabState) {
	let question = if s.error {
		crate::t!("edit-gitlab-invalid-question")
	} else {
		crate::t!("edit-gitlab-question")
	};
	let color = if s.error { Color::Red } else { Color::Yellow };

	let chunks = widgets::wizard_layout(area, &build_constraints(area, &question, s.self_managed));
	widgets::render_question(frame, chunks[0], &question, color);
	render_textarea_with_focus(
		frame,
		chunks[1],
		&s.project_textarea,
		s.focus == GitLabFocus::Project,
	);
	render_checkbox(
		frame,
		chunks[2],
		s.focus == GitLabFocus::SelfManaged,
		s.self_managed,
	);

	if s.self_managed {
		let host_question = crate::t!("edit-gitlab-host-question");
		widgets::render_question(frame, chunks[3], &host_question, Color::Yellow);
		render_textarea_with_focus(
			frame,
			chunks[4],
			&s.host_textarea,
			s.focus == GitLabFocus::Host,
		);
	}

	let help_index = chunks.len() - 1;
	widgets::render_help(frame, chunks[help_index], &crate::t!("edit-gitlab-help"));
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;
	use ratatui_textarea::TextArea;

	use crate::forge::gitlab::remote::GitLabProject;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};
	use super::*;

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
		let (new_state, s) =
			unwrap_continue(handle_key(state, empty_screen(), key(KeyCode::Enter)));
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
		state.detected_gitlab =
			Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap());
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
		state.detected_gitlab =
			Some(GitLabProject::new("gitlab.example.com", "acme", "app").unwrap());
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
			.draw(|frame| super::super::ui(frame, &wstate, &empty_screen()))
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
			.draw(|frame| super::super::ui(frame, &wstate, &screen))
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
			.draw(|frame| super::super::ui(frame, &wstate, &screen))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Invalid"));
	}
}
