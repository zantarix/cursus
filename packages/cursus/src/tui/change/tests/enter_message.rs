use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::model::changeset::ChangeType;
use crate::tui::change::enter_message::*;
use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
use crate::tui::widgets::KeyResult;

use crate::tui::change::test_helpers::dummy_projects;
use crate::tui::change::{BackState, ChangeResult, Screen};

fn key(code: KeyCode) -> Event {
	Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn key_with_mods(code: KeyCode, mods: KeyModifiers) -> Event {
	Event::Key(KeyEvent::new(code, mods))
}

fn empty_back() -> BackState {
	BackState::SinglePackage {
		level: ChangeType::Patch,
	}
}

fn projects_with_patch() -> Vec<(crate::package_manager::Project, ChangeType)> {
	let p = dummy_projects(1);
	vec![(p.into_iter().next().unwrap(), ChangeType::Patch)]
}

#[test]
fn enter_submits_with_current_text() {
	let mut ta = initial_textarea();
	ta.insert_str("My change description");
	let result =
		handle_event_enter_message(ta, projects_with_patch(), empty_back(), key(KeyCode::Enter))
			.unwrap();
	match result {
		KeyResult::Complete(ChangeResult {
			message: Some(msg), ..
		}) => {
			assert_eq!(msg, "My change description");
		}
		_ => panic!("Expected Complete with message"),
	}
}

#[test]
fn enter_submits_with_empty_text() {
	let ta = initial_textarea();
	let result =
		handle_event_enter_message(ta, projects_with_patch(), empty_back(), key(KeyCode::Enter))
			.unwrap();
	match result {
		KeyResult::Complete(ChangeResult {
			message: Some(msg), ..
		}) => {
			assert_eq!(msg, "");
		}
		_ => panic!("Expected Complete with empty message"),
	}
}

#[test]
fn ctrl_e_completes_with_none_message() {
	let ta = initial_textarea();
	let result = handle_event_enter_message(
		ta,
		projects_with_patch(),
		empty_back(),
		key_with_mods(KeyCode::Char('e'), KeyModifiers::CONTROL),
	)
	.unwrap();
	match result {
		KeyResult::Complete(ChangeResult { message: None, .. }) => {}
		_ => panic!("Expected Complete with None message"),
	}
}

#[test]
fn esc_goes_back_to_single_package() {
	let ta = initial_textarea();
	let back = BackState::SinglePackage {
		level: ChangeType::Minor,
	};
	let result =
		handle_event_enter_message(ta, projects_with_patch(), back, key(KeyCode::Esc)).unwrap();
	match result {
		KeyResult::Continue(Screen::SinglePackage { level }) => {
			assert_eq!(level, ChangeType::Minor);
		}
		_ => panic!("Expected Continue(SinglePackage)"),
	}
}

#[test]
fn esc_goes_back_to_select_projects() {
	use crate::tui::change::SelectProjectsState;
	let ta = initial_textarea();
	let back = BackState::MultiPackage(SelectProjectsState {
		selected: vec![true, false],
		levels: vec![ChangeType::Major, ChangeType::Patch],
		cursor: 0,
		error: false,
		changed_count: 1,
	});
	let result =
		handle_event_enter_message(ta, projects_with_patch(), back, key(KeyCode::Esc)).unwrap();
	match result {
		KeyResult::Continue(Screen::SelectProjects(SelectProjectsState {
			selected,
			levels,
			cursor,
			error,
			changed_count,
		})) => {
			assert_eq!(selected, vec![true, false]);
			assert_eq!(levels, vec![ChangeType::Major, ChangeType::Patch]);
			assert_eq!(cursor, 0);
			assert!(!error);
			assert_eq!(changed_count, 1);
		}
		_ => panic!("Expected Continue(SelectProjects)"),
	}
}

#[test]
fn shift_enter_inserts_newline() {
	let ta = initial_textarea();
	let event = key_with_mods(KeyCode::Enter, KeyModifiers::SHIFT);
	let result =
		handle_event_enter_message(ta, projects_with_patch(), empty_back(), event).unwrap();
	match result {
		KeyResult::Continue(Screen::EnterMessage { textarea, .. }) => {
			// TextArea should have 2 lines after a newline insertion
			assert_eq!(textarea.lines().len(), 2);
		}
		_ => panic!("Expected Continue(EnterMessage)"),
	}
}

#[test]
fn other_keys_are_forwarded_to_textarea() {
	let ta = initial_textarea();
	let result = handle_event_enter_message(
		ta,
		projects_with_patch(),
		empty_back(),
		key(KeyCode::Char('x')),
	)
	.unwrap();
	match result {
		KeyResult::Continue(Screen::EnterMessage { textarea, .. }) => {
			assert_eq!(textarea.lines()[0], "x");
		}
		_ => panic!("Expected Continue(EnterMessage)"),
	}
}

#[test]
fn enter_preserves_projects() {
	let projects = {
		let p = dummy_projects(2);
		vec![
			(p[0].clone(), ChangeType::Major),
			(p[1].clone(), ChangeType::Minor),
		]
	};
	let ta = initial_textarea();
	let result =
		handle_event_enter_message(ta, projects, empty_back(), key(KeyCode::Enter)).unwrap();
	match result {
		KeyResult::Complete(ChangeResult { projects: proj, .. }) => {
			assert_eq!(proj.len(), 2);
			assert_eq!(proj[0].1, ChangeType::Major);
			assert_eq!(proj[1].1, ChangeType::Minor);
		}
		_ => panic!("Expected Complete"),
	}
}

/// Catches `&&`→`||` mutation at line 57-58: Alt+Enter must NOT submit —
/// the guard requires BOTH ALT and SHIFT to be absent.
/// With `||`, `!ALT || !SHIFT` is true when only one is absent → submits (wrong).
#[test]
fn alt_enter_does_not_submit() {
	let ta = initial_textarea();
	let event = key_with_mods(KeyCode::Enter, KeyModifiers::ALT);
	let result =
		handle_event_enter_message(ta, projects_with_patch(), empty_back(), event).unwrap();
	// Alt+Enter should fall through to the textarea (not Complete)
	assert!(
		matches!(result, KeyResult::Continue(Screen::EnterMessage { .. })),
		"Alt+Enter must not submit"
	);
}

#[test]
fn ui_renders_enter_message_screen() {
	crate::locale::set_locale("en");
	let mut terminal = create_test_terminal();
	let projects = dummy_projects(1);
	let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let ta = initial_textarea();
	let screen = Screen::EnterMessage {
		textarea: ta,
		projects: vec![(projects[0].clone(), ChangeType::Patch)],
		back: empty_back(),
	};
	terminal
		.draw(|frame| crate::tui::change::ui(frame, &screen, &names))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Describe this change"));
	assert!(content.contains("Message"));
}
