//! TUI for selecting projects and the type of change (major, minor, patch).

use crossterm::event::Event;
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
		/// Number of projects in the "Changed" group (always the first slice).
		changed_count: usize,
	},
	SelectChangeType {
		change_type: ChangeType,
		selected_indices: Vec<usize>,
	},
}

/// Shorthand for the handle_event return type used by the internal state machine.
type HandleResult = KeyResult<Screen, ChangeResult>;

/// Output of [`reorder_projects`]: projects sorted changed-first, with index mapping.
struct ReorderedProjects {
	/// Projects reordered: changed first (sorted by name), then unchanged (sorted by name).
	projects: Vec<Project>,
	/// Number of projects in the changed group (the first `changed_count` entries).
	changed_count: usize,
	/// Maps original index → new index in `projects`.
	orig_to_new: Vec<usize>,
}

/// Partitions and sorts `projects` into changed-first order based on `changed_flags`.
///
/// Both groups are sorted by project name within their group. Returns the
/// reordered list together with the boundary count and an index mapping.
///
/// # Precondition
///
/// `changed_flags.len()` must equal `projects.len()`. The caller in [`run`]
/// ensures this by normalising the slice before calling this function.
fn reorder_projects(projects: &[Project], changed_flags: &[bool]) -> ReorderedProjects {
	let changed_count = changed_flags.iter().filter(|&&c| c).count();
	let mut changed_pairs: Vec<(usize, Project)> = projects
		.iter()
		.enumerate()
		.filter(|(i, _)| changed_flags[*i])
		.map(|(i, p)| (i, p.clone()))
		.collect();
	let mut unchanged_pairs: Vec<(usize, Project)> = projects
		.iter()
		.enumerate()
		.filter(|(i, _)| !changed_flags[*i])
		.map(|(i, p)| (i, p.clone()))
		.collect();
	changed_pairs.sort_by_key(|(_, p)| p.name().to_string());
	unchanged_pairs.sort_by_key(|(_, p)| p.name().to_string());
	let reordered_pairs: Vec<(usize, Project)> =
		changed_pairs.into_iter().chain(unchanged_pairs).collect();
	let mut orig_to_new = vec![0usize; projects.len()];
	for (new_idx, (orig_idx, _)) in reordered_pairs.iter().enumerate() {
		orig_to_new[*orig_idx] = new_idx;
	}
	let reordered = reordered_pairs.into_iter().map(|(_, p)| p).collect();
	ReorderedProjects {
		projects: reordered,
		changed_count,
		orig_to_new,
	}
}

fn handle_event(
	screen: &Screen,
	event: Event,
	area: Rect,
	projects: &[Project],
) -> anyhow::Result<HandleResult> {
	match screen {
		Screen::SelectProjects {
			selected,
			cursor,
			changed_count,
			..
		} => Ok(select_projects::handle_event_select_projects(
			selected,
			*cursor,
			*changed_count,
			event,
			area,
		)),
		Screen::SelectChangeType {
			change_type,
			selected_indices,
		} => match event {
			Event::Key(key) => select_change_type::handle_key_change_type(
				*change_type,
				selected_indices,
				key.code,
				projects,
			),
			_ => Ok(KeyResult::Continue(Screen::SelectChangeType {
				change_type: *change_type,
				selected_indices: selected_indices.to_vec(),
			})),
		},
	}
}

fn ui(frame: &mut Frame, screen: &Screen, project_names: &[&str]) {
	let area = frame.area();
	match screen {
		Screen::SelectProjects {
			selected,
			cursor,
			error,
			changed_count,
		} => {
			select_projects::render_select_projects(
				frame,
				area,
				project_names,
				selected,
				*cursor,
				*error,
				*changed_count,
			);
		}
		Screen::SelectChangeType { change_type, .. } => {
			let chunks = widgets::wizard_layout(
				area,
				&[
					Constraint::Length(3),
					Constraint::Min(5),
					Constraint::Length(1),
				],
			);
			select_change_type::render_select_change_type(frame, &chunks, *change_type);
		}
	}
}

/// Runs the interactive TUI for selecting projects and a change type.
///
/// Displays a terminal UI that allows the user to select which projects
/// to include and the type of semantic version change. Projects are split
/// into "Changed" (pre-selected) and "Unchanged" (unselected) groups based
/// on the provided `changed` classification.
///
/// # Returns
///
/// Returns `Ok(Some(ChangeResult))` if the user completes selection,
/// or `Ok(None)` if the user cancels.
///
/// # Errors
///
/// Returns an error if terminal setup or I/O operations fail.
pub fn run(
	projects: &[Project],
	options: &ChangeOptions,
	changed: &[bool],
) -> anyhow::Result<Option<ChangeResult>> {
	let changed_flags: Vec<bool> = if changed.len() == projects.len() {
		changed.to_vec()
	} else {
		vec![true; projects.len()] // length mismatch: treat all as changed
	};

	let ro = reorder_projects(projects, &changed_flags);

	let project_indices = match &options.projects {
		Some(indices) => indices.iter().map(|&i| ro.orig_to_new[i]).collect(),
		None if ro.projects.len() == 1 => vec![0],
		_ => vec![], // Need interactive project selection
	};

	let have_projects = !project_indices.is_empty();

	if let Some(change_type) = options.change_type {
		let indices = if have_projects {
			project_indices
		} else {
			(0..ro.projects.len()).collect()
		};
		return Ok(Some(ChangeResult {
			projects: indices
				.into_iter()
				.map(|i| ro.projects[i].clone())
				.collect(),
			change_type,
		}));
	}

	let project_names: Vec<&str> = ro.projects.iter().map(|p| p.name()).collect();

	let initial_screen = if have_projects {
		Screen::SelectChangeType {
			change_type: ChangeType::Patch,
			selected_indices: project_indices,
		}
	} else {
		let selected: Vec<bool> = (0..ro.projects.len())
			.map(|i| i < ro.changed_count)
			.collect();
		Screen::SelectProjects {
			selected,
			cursor: 0,
			error: false,
			changed_count: ro.changed_count,
		}
	};

	let result = widgets::run_tui(
		initial_screen,
		|frame, screen| ui(frame, screen, &project_names),
		|screen, event, area| handle_event(&screen, event, area, &ro.projects),
	)?;

	Ok(result)
}

/// Thin keyboard-only wrapper around `handle_event` for use in unit tests.
///
/// Passes a default 80×24 content area so tests don't need to supply one.
#[cfg(test)]
fn handle_key(
	screen: &Screen,
	key: crossterm::event::KeyCode,
	projects: &[Project],
) -> anyhow::Result<HandleResult> {
	use crossterm::event::{KeyEvent, KeyModifiers};
	handle_event(
		screen,
		Event::Key(KeyEvent::new(key, KeyModifiers::NONE)),
		Rect::new(0, 0, 80, 24),
		projects,
	)
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
	use crate::package_manager::Project;

	use super::test_helpers::dummy_projects;
	use super::*;

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

	#[test]
	fn workflow_select_projects_then_change_type() {
		let projects = dummy_projects(3);

		let screen = Screen::SelectProjects {
			selected: vec![true, true, true],
			cursor: 0,
			error: false,
			changed_count: 3,
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
				changed_count: 3,
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
