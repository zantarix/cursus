//! TUI for selecting projects and the type of change (major, minor, patch).

use anyhow::Context;
use crossterm::event::Event;
use ratatui::prelude::*;
use ratatui_textarea::TextArea;

use super::screens::ButtonScreen;
use super::widgets::{self, KeyResult};
use crate::model::changeset::ChangeType;
use crate::package_manager::Project;

mod enter_message;
mod select_projects;
mod single_package;

/// The result of a completed change selection.
#[derive(Debug, Clone)]
pub struct ChangeResult {
	/// The projects and their per-package change types selected by the user.
	pub projects: Vec<(Project, ChangeType)>,
	/// The changeset description. `None` means launch the editor.
	pub message: Option<String>,
}

/// Options that can be pre-filled to skip interactive steps.
#[derive(Debug, Clone, Default)]
pub struct ChangeOptions {
	/// Pre-selected change type (skips selection screen).
	pub change_type: Option<ChangeType>,
	/// Pre-selected project indices (skips project selection screen).
	pub projects: Option<Vec<usize>>,
}

/// State for the [`Screen::SelectProjects`] screen.
#[derive(Debug)]
pub(crate) struct SelectProjectsState {
	pub(crate) selected: Vec<bool>,
	/// Per-project change level (shown only for selected projects).
	pub(crate) levels: Vec<ChangeType>,
	pub(crate) cursor: usize,
	pub(crate) error: bool,
	/// Number of projects in the "Changed" group (always the first slice).
	pub(crate) changed_count: usize,
}

/// State carried when navigating back from [`Screen::EnterMessage`].
pub(crate) enum BackState {
	MultiPackage(SelectProjectsState),
	SinglePackage { level: ChangeType },
}

pub(crate) enum Screen {
	SelectProjects(SelectProjectsState),
	SinglePackage {
		level: ChangeType,
	},
	EnterMessage {
		textarea: Box<TextArea<'static>>,
		projects: Vec<(Project, ChangeType)>,
		back: BackState,
	},
}

/// Shorthand for the handle_event return type used by the internal state machine.
pub(super) type HandleResult = KeyResult<Screen, ChangeResult>;

/// Output of [`reorder_projects`]: projects sorted changed-first, with index mapping.
pub(crate) struct ReorderedProjects {
	/// Projects reordered: changed first (sorted by name), then unchanged (sorted by name).
	pub(crate) projects: Vec<Project>,
	/// Number of projects in the changed group (the first `changed_count` entries).
	pub(crate) changed_count: usize,
	/// Maps original index → new index in `projects`.
	pub(crate) orig_to_new: Vec<usize>,
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
pub(crate) fn reorder_projects(projects: &[Project], changed_flags: &[bool]) -> ReorderedProjects {
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
	changed_pairs.sort_by(|a, b| a.1.name().cmp(b.1.name()));
	unchanged_pairs.sort_by(|a, b| a.1.name().cmp(b.1.name()));
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

pub(super) fn handle_event(
	screen: Screen,
	event: Event,
	area: Rect,
	projects: &[Project],
) -> anyhow::Result<HandleResult> {
	match screen {
		Screen::SelectProjects(state) => Ok(select_projects::handle_event_select_projects(
			state, event, area, projects,
		)),
		Screen::SinglePackage { level } => {
			let project = projects
				.first()
				.context("SinglePackage screen requires at least one project")?
				.clone();
			let buttons = single_package::SinglePackageButtons { level };
			match buttons.handle_event(vec![project], event, area)? {
				KeyResult::Continue((_, screen)) => Ok(KeyResult::Continue(screen)),
				KeyResult::Complete(cr) => Ok(KeyResult::Complete(cr)),
				KeyResult::Cancelled => Ok(KeyResult::Cancelled),
			}
		}
		Screen::EnterMessage {
			textarea,
			projects: proj,
			back,
		} => enter_message::handle_event_enter_message(textarea, proj, back, event),
	}
}

pub(crate) fn ui(frame: &mut Frame, screen: &Screen, project_names: &[&str]) {
	let area = frame.area();
	match screen {
		Screen::SelectProjects(state) => {
			select_projects::render_select_projects(frame, area, project_names, state);
		}
		Screen::SinglePackage { level } => {
			single_package::SinglePackageButtons { level: *level }.render(frame, area);
		}
		Screen::EnterMessage { textarea, .. } => {
			enter_message::render_enter_message(frame, area, textarea);
		}
	}
}

/// Constructs the initial [`Screen`] for the TUI based on how many projects
/// there are and whether the caller pre-selected any of them.
pub(crate) fn build_initial_screen(
	ro: &ReorderedProjects,
	project_indices: &[usize],
	have_projects: bool,
) -> Screen {
	if ro.projects.len() == 1 {
		return Screen::SinglePackage {
			level: ChangeType::Patch,
		};
	}
	if have_projects {
		let mut selected = vec![false; ro.projects.len()];
		for &i in project_indices {
			selected[i] = true;
		}
		Screen::SelectProjects(SelectProjectsState {
			selected,
			levels: vec![ChangeType::Patch; ro.projects.len()],
			cursor: 0,
			error: false,
			changed_count: ro.changed_count,
		})
	} else {
		let selected = (0..ro.projects.len())
			.map(|i| i < ro.changed_count)
			.collect();
		Screen::SelectProjects(SelectProjectsState {
			selected,
			levels: vec![ChangeType::Patch; ro.projects.len()],
			cursor: 0,
			error: false,
			changed_count: ro.changed_count,
		})
	}
}

/// Runs the interactive TUI for selecting projects and a change type.
///
/// When `options.change_type` is `Some`, no terminal is opened. Projects are
/// selected immediately: if `options.projects` is supplied those explicit
/// indices take precedence; otherwise the "changed" group (first
/// `changed_count` entries of the reordered list) is used, falling back to
/// all projects when `changed_count == 0`.
///
/// When `options.change_type` is `None`, a terminal UI is displayed. Projects
/// are split into "Changed" (pre-selected) and "Unchanged" (unselected) groups
/// based on the provided `changed` classification.
///
/// # Returns
///
/// Returns `Ok(Some(ChangeResult))` if selection completes (either via the
/// early-return path or user confirmation in the TUI), or `Ok(None)` if the
/// user cancels.
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

	let project_indices: Vec<usize> = match &options.projects {
		Some(indices) => indices.iter().map(|&i| ro.orig_to_new[i]).collect(),
		None if ro.projects.len() == 1 => vec![0],
		_ => vec![], // Need interactive project selection
	};

	let have_projects = !project_indices.is_empty();

	if let Some(change_type) = options.change_type {
		let indices = if have_projects {
			project_indices
		} else if ro.changed_count > 0 {
			(0..ro.changed_count).collect()
		} else {
			(0..ro.projects.len()).collect()
		};
		return Ok(Some(ChangeResult {
			projects: indices
				.into_iter()
				.map(|i| (ro.projects[i].clone(), change_type))
				.collect(),
			message: None,
		}));
	}

	let project_names: Vec<&str> = ro.projects.iter().map(|p| p.name()).collect();
	let initial_screen = build_initial_screen(&ro, &project_indices, have_projects);

	let result = widgets::run_tui(
		initial_screen,
		|frame, screen| ui(frame, screen, &project_names),
		|screen, event, area| handle_event(screen, event, area, &ro.projects),
	)?;

	Ok(result)
}

/// Thin keyboard-only wrapper around `handle_event` for use in unit tests.
///
/// Passes a default 80×24 content area so tests don't need to supply one.
#[cfg(test)]
pub(super) fn handle_key(
	screen: Screen,
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
pub(crate) mod test_helpers {
	use crate::package_manager::Project;

	pub(crate) fn dummy_projects(n: usize) -> Vec<Project> {
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
mod tests;
