use crossterm::event::KeyCode;
use ratatui::prelude::Rect;

use crate::model::changeset::ChangeType;
use crate::tui::change::single_package::*;
use crate::tui::screens::ButtonScreen;
use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
use crate::tui::widgets::KeyResult;

use crate::tui::change::test_helpers::dummy_projects;
use crate::tui::change::{HandleResult, Screen, handle_key};

fn single_package_screen(level: ChangeType) -> Screen {
	Screen::SinglePackage { level }
}

/// Unwrap a `Continue(SinglePackage { level })` result.
fn unwrap_single_package(result: anyhow::Result<HandleResult>) -> ChangeType {
	match result.unwrap() {
		KeyResult::Continue(Screen::SinglePackage { level }) => level,
		_ => panic!("Expected Continue(SinglePackage)"),
	}
}

#[test]
fn single_package_right_cycles_to_next_level() {
	let projects = dummy_projects(1);
	let level = unwrap_single_package(handle_key(
		single_package_screen(ChangeType::Patch),
		KeyCode::Right,
		&projects,
	));
	assert_eq!(level, ChangeType::Major);
}

#[test]
fn single_package_left_cycles_to_prev_level() {
	let projects = dummy_projects(1);
	let level = unwrap_single_package(handle_key(
		single_package_screen(ChangeType::Patch),
		KeyCode::Left,
		&projects,
	));
	assert_eq!(level, ChangeType::Minor);
}

#[test]
fn single_package_tab_cycles_forward() {
	let projects = dummy_projects(1);
	let level = unwrap_single_package(handle_key(
		single_package_screen(ChangeType::Major),
		KeyCode::Tab,
		&projects,
	));
	assert_eq!(level, ChangeType::Minor);
}

#[test]
fn single_package_enter_advances_to_enter_message() {
	let projects = dummy_projects(1);
	let result = handle_key(
		single_package_screen(ChangeType::Minor),
		KeyCode::Enter,
		&projects,
	)
	.unwrap();
	match result {
		KeyResult::Continue(Screen::EnterMessage { projects: proj, .. }) => {
			assert_eq!(proj.len(), 1);
			assert_eq!(proj[0].0.name(), "project-0");
			assert_eq!(proj[0].1, ChangeType::Minor);
		}
		_ => panic!("Expected Continue(EnterMessage)"),
	}
}

#[test]
fn single_package_esc_cancels() {
	let projects = dummy_projects(1);
	let result = handle_key(
		single_package_screen(ChangeType::Patch),
		KeyCode::Esc,
		&projects,
	)
	.unwrap();
	assert!(matches!(result, KeyResult::Cancelled));
}

#[test]
fn single_package_next_cycles_all_three() {
	assert_eq!(
		SinglePackageButtons {
			level: ChangeType::Major
		}
		.next()
		.level,
		ChangeType::Minor
	);
	assert_eq!(
		SinglePackageButtons {
			level: ChangeType::Minor
		}
		.next()
		.level,
		ChangeType::Patch
	);
	assert_eq!(
		SinglePackageButtons {
			level: ChangeType::Patch
		}
		.next()
		.level,
		ChangeType::Major
	);
}

#[test]
fn single_package_prev_cycles_all_three() {
	assert_eq!(
		SinglePackageButtons {
			level: ChangeType::Major
		}
		.prev()
		.level,
		ChangeType::Patch
	);
	assert_eq!(
		SinglePackageButtons {
			level: ChangeType::Minor
		}
		.prev()
		.level,
		ChangeType::Major
	);
	assert_eq!(
		SinglePackageButtons {
			level: ChangeType::Patch
		}
		.prev()
		.level,
		ChangeType::Minor
	);
}

#[test]
fn single_package_with_index_maps_correctly() {
	assert_eq!(
		SinglePackageButtons {
			level: ChangeType::Patch
		}
		.with_index(0)
		.level,
		ChangeType::Major
	);
	assert_eq!(
		SinglePackageButtons {
			level: ChangeType::Patch
		}
		.with_index(1)
		.level,
		ChangeType::Minor
	);
	assert_eq!(
		SinglePackageButtons {
			level: ChangeType::Patch
		}
		.with_index(2)
		.level,
		ChangeType::Patch
	);
}

#[test]
fn ui_renders_single_package_screen() {
	crate::locale::set_locale("en");
	let mut terminal = create_test_terminal();
	let projects = dummy_projects(1);
	let names: Vec<&str> = projects.iter().map(|p| p.name()).collect();
	let screen = single_package_screen(ChangeType::Minor);
	terminal
		.draw(|frame| crate::tui::change::ui(frame, &screen, &names))
		.unwrap();
	let content = buffer_to_string(terminal.backend().buffer());
	assert!(content.contains("Major"));
	assert!(content.contains("Minor"));
	assert!(content.contains("Patch"));
	assert!(content.contains("What type of change"));
}

#[test]
fn single_package_click_major_button_advances_to_enter_message() {
	use crate::tui::test_utils::mouse_click;
	let projects = dummy_projects(1);
	let area = Rect::new(0, 0, 80, 24);
	let buttons = SinglePackageButtons {
		level: ChangeType::Patch,
	};
	let result = buttons
		.handle_event(vec![projects[0].clone()], mouse_click(10, 5), area)
		.unwrap();
	match result {
		KeyResult::Continue((_, Screen::EnterMessage { projects: proj, .. })) => {
			assert_eq!(proj[0].1, ChangeType::Major);
		}
		_ => panic!("Expected Continue(EnterMessage)"),
	}
}
