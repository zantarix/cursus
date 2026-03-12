//! TUI for selecting projects and the type of change (major, minor, patch).

use crossterm::event::KeyCode;
use ratatui::prelude::*;

use super::widgets::{self, KeyResult};
use crate::model::changeset::ChangeType;
use crate::package_manager::Project;

mod select_change_type;
mod select_projects;

/// The result of a completed change selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeResult {
	/// The projects selected by the user.
	pub projects: Vec<Project>,
	/// The type of change selected by the user.
	pub change_type: ChangeType,
}

/// Options that can be pre-filled to skip interactive steps.
#[derive(Debug, Clone, Default)]
pub struct ChangeOptions {
	/// Pre-selected change type (skips selection screen).
	pub change_type: Option<ChangeType>,
	/// Pre-selected project indices (skips project selection screen).
	pub projects: Option<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
	SelectProjects {
		selected: Vec<bool>,
		cursor: usize,
		error: bool,
	},
	SelectChangeType {
		change_type: ChangeType,
		selected_indices: Vec<usize>,
	},
}

/// Shorthand for the handle_key return type used by the internal state machine.
type HandleResult = KeyResult<Screen, ChangeResult>;

fn handle_key(screen: &Screen, key: KeyCode, projects: &[Project]) -> anyhow::Result<HandleResult> {
	match screen {
		Screen::SelectProjects {
			selected, cursor, ..
		} => Ok(select_projects::handle_key_select_projects(
			selected, *cursor, key,
		)),
		Screen::SelectChangeType {
			change_type,
			selected_indices,
		} => select_change_type::handle_key_change_type(
			*change_type,
			selected_indices,
			key,
			projects,
		),
	}
}

fn ui(frame: &mut Frame, screen: &Screen, project_names: &[&str]) {
	let chunks = widgets::wizard_layout(
		frame,
		&[
			Constraint::Length(3),
			Constraint::Min(5),
			Constraint::Length(1),
		],
	);

	match screen {
		Screen::SelectProjects {
			selected,
			cursor,
			error,
		} => {
			select_projects::render_select_projects(
				frame,
				&chunks,
				project_names,
				selected,
				*cursor,
				*error,
			);
		}
		Screen::SelectChangeType { change_type, .. } => {
			select_change_type::render_select_change_type(frame, &chunks, *change_type);
		}
	}
}

/// Runs the interactive TUI for selecting projects and a change type.
///
/// Displays a terminal UI that allows the user to select which projects
/// to include and the type of semantic version change.
///
/// # Returns
///
/// Returns `Ok(Some(ChangeResult))` if the user completes selection,
/// or `Ok(None)` if the user cancels.
///
/// # Errors
///
/// Returns an error if terminal setup or I/O operations fail.
pub fn run(projects: &[Project], options: &ChangeOptions) -> anyhow::Result<Option<ChangeResult>> {
	let project_indices = match &options.projects {
		Some(indices) => indices.clone(),
		None if projects.len() == 1 => vec![0],
		_ => {
			// Need interactive project selection - fall through to TUI
			vec![]
		}
	};

	let have_projects = !project_indices.is_empty();

	if let Some(change_type) = options.change_type {
		// Both pre-filled
		let indices = if have_projects {
			project_indices
		} else {
			(0..projects.len()).collect()
		};
		return Ok(Some(ChangeResult {
			projects: indices.into_iter().map(|i| projects[i].clone()).collect(),
			change_type,
		}));
	}

	let project_names: Vec<&str> = projects.iter().map(|p| p.name()).collect();

	let initial_screen = if have_projects {
		Screen::SelectChangeType {
			change_type: ChangeType::Patch,
			selected_indices: project_indices,
		}
	} else {
		Screen::SelectProjects {
			selected: vec![true; projects.len()],
			cursor: 0,
			error: false,
		}
	};

	let result = widgets::run_tui(
		initial_screen,
		|frame, screen| ui(frame, screen, &project_names),
		|screen, key| handle_key(&screen, key.code, projects),
	)?;

	Ok(result)
}

#[cfg(test)]
pub(super) mod test_helpers {
	use crate::package_manager::Project;

	pub(super) fn dummy_projects(n: usize) -> Vec<Project> {
		(0..n)
			.map(|i| {
				Project::new_test(
					&format!("project-{i}"),
					&format!("/nonexistent/projects/project-{i}"),
				)
			})
			.collect()
	}
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use crate::model::changeset::ChangeType;

	use super::test_helpers::dummy_projects;
	use super::*;

	#[test]
	fn workflow_select_projects_then_change_type() {
		let projects = dummy_projects(3);

		let screen = Screen::SelectProjects {
			selected: vec![true, true, true],
			cursor: 0,
			error: false,
		};

		// Deselect first project
		let screen = match handle_key(&screen, KeyCode::Char(' '), &projects).unwrap() {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		};
		assert_eq!(
			screen,
			Screen::SelectProjects {
				selected: vec![false, true, true],
				cursor: 0,
				error: false,
			}
		);

		// Confirm project selection
		let screen = match handle_key(&screen, KeyCode::Enter, &projects).unwrap() {
			KeyResult::Continue(s) => s,
			_ => panic!("Expected Continue"),
		};
		assert_eq!(
			screen,
			Screen::SelectChangeType {
				change_type: ChangeType::Patch,
				selected_indices: vec![1, 2],
			}
		);

		// Select minor
		let result = handle_key(&screen, KeyCode::Char('i'), &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: vec![projects[1].clone(), projects[2].clone()],
				change_type: ChangeType::Minor,
			})
		);
	}
}
