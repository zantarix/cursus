mod enter_message;
mod single_package;

use crossterm::event::KeyCode;

use crate::model::changeset::ChangeType;
use crate::package_manager::Project;

use crate::tui::change::test_helpers::dummy_projects;
use crate::tui::change::*;

// --- reorder_projects tests ---

#[test]
fn reorder_projects_mixed_changed_and_unchanged() {
	let projects = dummy_projects(3); // project-0, project-1, project-2
	let changed_flags = vec![false, true, false];
	let ro = reorder_projects(&projects, &changed_flags);
	// Changed: [project-1], Unchanged: [project-0, project-2] (sorted by name)
	assert_eq!(ro.changed_count, 1);
	assert_eq!(ro.projects[0].name(), "project-1"); // only changed
	assert_eq!(ro.projects[1].name(), "project-0"); // unchanged, alphabetically first
	assert_eq!(ro.projects[2].name(), "project-2");
	assert_eq!(ro.orig_to_new[0], 1); // project-0 (orig 0) → new idx 1
	assert_eq!(ro.orig_to_new[1], 0); // project-1 (orig 1) → new idx 0
	assert_eq!(ro.orig_to_new[2], 2); // project-2 (orig 2) → new idx 2
}

#[test]
fn reorder_projects_all_changed() {
	let projects = dummy_projects(2);
	let ro = reorder_projects(&projects, &[true, true]);
	assert_eq!(ro.changed_count, 2);
	assert_eq!(ro.projects[0].name(), "project-0");
	assert_eq!(ro.projects[1].name(), "project-1");
	assert_eq!(ro.orig_to_new[0], 0);
	assert_eq!(ro.orig_to_new[1], 1);
}

#[test]
fn reorder_projects_all_unchanged() {
	let projects = dummy_projects(2);
	let ro = reorder_projects(&projects, &[false, false]);
	assert_eq!(ro.changed_count, 0);
	assert_eq!(ro.projects[0].name(), "project-0");
	assert_eq!(ro.projects[1].name(), "project-1");
	assert_eq!(ro.orig_to_new[0], 0);
	assert_eq!(ro.orig_to_new[1], 1);
}

#[test]
fn reorder_projects_empty() {
	let ro = reorder_projects(&[], &[]);
	assert_eq!(ro.changed_count, 0);
	assert!(ro.projects.is_empty());
	assert!(ro.orig_to_new.is_empty());
}

#[test]
fn reorder_projects_single_project() {
	let projects = dummy_projects(1);
	let ro = reorder_projects(&projects, &[true]);
	assert_eq!(ro.changed_count, 1);
	assert_eq!(ro.projects[0].name(), "project-0");
	assert_eq!(ro.orig_to_new[0], 0);
}

#[test]
fn reorder_projects_sorts_changed_group_by_name() {
	let projects = vec![
		Project::new_test("beta", "/nonexistent/beta"),
		Project::new_test("alpha", "/nonexistent/alpha"),
	];
	let ro = reorder_projects(&projects, &[true, true]);
	assert_eq!(ro.changed_count, 2);
	assert_eq!(ro.projects[0].name(), "alpha");
	assert_eq!(ro.projects[1].name(), "beta");
	assert_eq!(ro.orig_to_new[0], 1); // "beta" (orig 0) → new idx 1
	assert_eq!(ro.orig_to_new[1], 0); // "alpha" (orig 1) → new idx 0
}

#[test]
fn reorder_projects_sorts_unchanged_group_by_name() {
	let projects = vec![
		Project::new_test("zeta", "/nonexistent/zeta"),
		Project::new_test("gamma", "/nonexistent/gamma"),
	];
	let ro = reorder_projects(&projects, &[false, false]);
	assert_eq!(ro.changed_count, 0);
	assert_eq!(ro.projects[0].name(), "gamma");
	assert_eq!(ro.projects[1].name(), "zeta");
	assert_eq!(ro.orig_to_new[0], 1); // "zeta" → new idx 1
	assert_eq!(ro.orig_to_new[1], 0); // "gamma" → new idx 0
}

/// Unwrap a `Continue(Screen::SelectProjects(...))` result, panicking on mismatch.
fn unwrap_select_projects(
	result: anyhow::Result<HandleResult>,
) -> (Vec<bool>, Vec<ChangeType>, usize, bool, usize) {
	match result.unwrap() {
		KeyResult::Continue(Screen::SelectProjects(SelectProjectsState {
			selected,
			levels,
			cursor,
			error,
			changed_count,
		})) => (selected, levels, cursor, error, changed_count),
		other => panic!(
			"Expected Continue(SelectProjects), got different variant: {:?}",
			std::mem::discriminant(&other)
		),
	}
}

/// Unwrap a `Continue(Screen::EnterMessage {...})` result.
fn unwrap_enter_message(result: anyhow::Result<HandleResult>) -> Vec<(Project, ChangeType)> {
	match result.unwrap() {
		KeyResult::Continue(Screen::EnterMessage { projects, .. }) => projects,
		_ => panic!("Expected Continue(EnterMessage)"),
	}
}

#[test]
fn workflow_select_projects_then_enter_message() {
	let projects = dummy_projects(3);

	let screen = Screen::SelectProjects(SelectProjectsState {
		selected: vec![true, true, true],
		levels: vec![ChangeType::Patch; 3],
		cursor: 0,
		error: false,
		changed_count: 3,
	});

	// Deselect first project
	let (selected, levels, cursor, error, changed_count) =
		unwrap_select_projects(handle_key(screen, KeyCode::Char(' '), &projects));
	assert_eq!(selected, vec![false, true, true]);
	assert_eq!(levels, vec![ChangeType::Patch; 3]);
	assert_eq!(cursor, 0);
	assert!(!error);
	assert_eq!(changed_count, 3);

	// Change level of project at cursor (0) — but it's not selected, so no change
	let screen = Screen::SelectProjects(SelectProjectsState {
		selected: vec![false, true, true],
		levels: vec![ChangeType::Patch; 3],
		cursor: 1,
		error: false,
		changed_count: 3,
	});

	// Change level of project-1: Patch.next() == Major
	let (selected2, levels2, ..) =
		unwrap_select_projects(handle_key(screen, KeyCode::Right, &projects));
	assert_eq!(selected2, vec![false, true, true]);
	assert_eq!(levels2[1], ChangeType::Major);

	// Confirm → EnterMessage with selected projects and levels
	let screen = Screen::SelectProjects(SelectProjectsState {
		selected: vec![false, true, true],
		levels: vec![ChangeType::Patch, ChangeType::Major, ChangeType::Patch],
		cursor: 0,
		error: false,
		changed_count: 3,
	});
	let proj = unwrap_enter_message(handle_key(screen, KeyCode::Enter, &projects));
	assert_eq!(proj.len(), 2);
	assert_eq!(proj[0].0.name(), "project-1");
	assert_eq!(proj[0].1, ChangeType::Major);
	assert_eq!(proj[1].0.name(), "project-2");
	assert_eq!(proj[1].1, ChangeType::Patch);
}

/// Catches the `== 1`→`!= 1` mutation at line 169: single project → `SinglePackage`.
#[test]
fn build_initial_screen_single_project_returns_single_package() {
	let projects = dummy_projects(1);
	let ro = reorder_projects(&projects, &[true]);
	let screen = build_initial_screen(&ro, &[], false);
	assert!(
		matches!(screen, Screen::SinglePackage { .. }),
		"Expected SinglePackage for one project"
	);
}

// --- change_type short-circuit tests ---

/// When --change-type is supplied without --project, only changed projects
/// should be included in the result (not all projects).
#[test]
fn run_change_type_shortcircuit_selects_only_changed() {
	let projects = dummy_projects(3);
	let changed = vec![true, false, false];
	let options = ChangeOptions {
		change_type: Some(ChangeType::Patch),
		projects: None,
	};
	let result = run(&projects, &options, &changed).unwrap().unwrap();
	assert_eq!(result.projects.len(), 1);
	assert_eq!(result.projects[0].0.name(), "project-0");
}

/// When --change-type is supplied and no projects are changed, fall back
/// to all projects (preserves prior behaviour for clean working trees).
#[test]
fn run_change_type_shortcircuit_falls_back_to_all_when_none_changed() {
	let projects = dummy_projects(2);
	let changed = vec![false, false];
	let options = ChangeOptions {
		change_type: Some(ChangeType::Minor),
		projects: None,
	};
	let result = run(&projects, &options, &changed).unwrap().unwrap();
	assert_eq!(result.projects.len(), 2);
}

/// Explicit --project flags override changed detection even in the short-circuit.
#[test]
fn run_change_type_shortcircuit_explicit_projects_override_changed() {
	let projects = dummy_projects(3);
	// Only project-1 is "changed", but we explicitly request project-2 (index 2).
	let changed = vec![false, true, false];
	let options = ChangeOptions {
		change_type: Some(ChangeType::Patch),
		projects: Some(vec![2]),
	};
	let result = run(&projects, &options, &changed).unwrap().unwrap();
	assert_eq!(result.projects.len(), 1);
	assert_eq!(result.projects[0].0.name(), "project-2");
}

/// Catches the `<`→`<=` mutation at line 188: projects[0] (index 0 < changed_count=1)
/// should be pre-selected; projects[1] (index 1 >= 1) should not be.
#[test]
fn build_initial_screen_no_pre_selection_selects_only_changed() {
	let projects = dummy_projects(2);
	let ro = reorder_projects(&projects, &[true, false]); // 1 changed
	let screen = build_initial_screen(&ro, &[], false);
	match screen {
		Screen::SelectProjects(SelectProjectsState {
			selected,
			changed_count,
			..
		}) => {
			assert_eq!(changed_count, 1);
			assert!(selected[0], "first project (changed) should be selected");
			assert!(
				!selected[1],
				"second project (unchanged) should not be selected"
			);
		}
		_ => panic!("Expected SelectProjects"),
	}
}
