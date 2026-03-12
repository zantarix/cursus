use anyhow::Context as _;
use crossterm::event::KeyCode;
use ratatui::{
	prelude::*,
	widgets::{Block, Borders, Paragraph},
};

use crate::model::changeset::ChangeType;
use crate::package_manager::Project;
use crate::tui::widgets::{self, KeyResult};

use super::{ChangeResult, HandleResult, Screen};

impl ChangeType {
	/// Returns the next change type when cycling through options in the TUI.
	pub(super) fn next(self) -> Self {
		match self {
			Self::Major => Self::Minor,
			Self::Minor => Self::Patch,
			Self::Patch => Self::Major,
		}
	}

	/// Returns the previous change type when cycling through options in the TUI.
	pub(super) fn prev(self) -> Self {
		match self {
			Self::Major => Self::Patch,
			Self::Minor => Self::Major,
			Self::Patch => Self::Minor,
		}
	}
}

/// Handles key events for the [`Screen::SelectChangeType`] screen.
pub(super) fn handle_key_change_type(
	current: ChangeType,
	selected_indices: &[usize],
	key: KeyCode,
	projects: &[Project],
) -> anyhow::Result<HandleResult> {
	let complete = |ct: ChangeType| -> anyhow::Result<HandleResult> {
		let resolved = selected_indices
			.iter()
			.map(|&i| {
				projects.get(i).cloned().with_context(|| {
					format!(
						"selected index {i} is out of range ({} projects)",
						projects.len()
					)
				})
			})
			.collect::<anyhow::Result<Vec<_>>>()?;
		Ok(KeyResult::Complete(ChangeResult {
			projects: resolved,
			change_type: ct,
		}))
	};
	match key {
		KeyCode::Left | KeyCode::Char('h') => Ok(KeyResult::Continue(Screen::SelectChangeType {
			change_type: current.prev(),
			selected_indices: selected_indices.to_vec(),
		})),
		KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => {
			Ok(KeyResult::Continue(Screen::SelectChangeType {
				change_type: current.next(),
				selected_indices: selected_indices.to_vec(),
			}))
		}
		KeyCode::Enter => complete(current),
		KeyCode::Char('m') => complete(ChangeType::Major),
		KeyCode::Char('i') => complete(ChangeType::Minor),
		KeyCode::Char('p') => complete(ChangeType::Patch),
		KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
		_ => Ok(KeyResult::Continue(Screen::SelectChangeType {
			change_type: current,
			selected_indices: selected_indices.to_vec(),
		})),
	}
}

/// Renders the [`Screen::SelectChangeType`] screen.
pub(super) fn render_select_change_type(frame: &mut Frame, chunks: &[Rect], selected: ChangeType) {
	widgets::render_question(
		frame,
		chunks[0],
		"What type of change is this?",
		Color::Yellow,
	);

	let buttons = Line::from(
		std::iter::once(Span::raw("  "))
			.chain(widgets::button_spans(
				" ",
				"M",
				"ajor ",
				selected == ChangeType::Major,
			))
			.chain(std::iter::once(Span::raw("   ")))
			.chain(widgets::button_spans(
				" M",
				"i",
				"nor ",
				selected == ChangeType::Minor,
			))
			.chain(std::iter::once(Span::raw("   ")))
			.chain(widgets::button_spans(
				" ",
				"P",
				"atch ",
				selected == ChangeType::Patch,
			))
			.chain(std::iter::once(Span::raw("  ")))
			.collect::<Vec<_>>(),
	);
	let button_para =
		Paragraph::new(buttons).block(Block::default().borders(Borders::ALL).title("Change Type"));
	frame.render_widget(button_para, chunks[1]);

	widgets::render_help(
		frame,
		chunks[2],
		"←/→/Tab: switch | m/i/p: select | Enter: confirm | Esc: cancel",
	);
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use crate::model::changeset::ChangeType;
	use crate::tui::test_utils::{buffer_to_string, create_test_terminal};

	use super::super::test_helpers::dummy_projects;
	use super::super::{ChangeResult, KeyResult, Screen, handle_key};

	fn change_type_screen(change_type: ChangeType, selected_indices: Vec<usize>) -> Screen {
		Screen::SelectChangeType {
			change_type,
			selected_indices,
		}
	}

	#[test]
	fn change_type_next_cycles_forward() {
		assert_eq!(ChangeType::Major.next(), ChangeType::Minor);
		assert_eq!(ChangeType::Minor.next(), ChangeType::Patch);
		assert_eq!(ChangeType::Patch.next(), ChangeType::Major);
	}

	#[test]
	fn change_type_prev_cycles_backward() {
		assert_eq!(ChangeType::Major.prev(), ChangeType::Patch);
		assert_eq!(ChangeType::Minor.prev(), ChangeType::Major);
		assert_eq!(ChangeType::Patch.prev(), ChangeType::Minor);
	}

	#[test]
	fn change_type_left_moves_to_previous() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Left, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Major, vec![]))
		);
	}

	#[test]
	fn change_type_right_moves_to_next() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Right, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Patch, vec![]))
		);
	}

	#[test]
	fn change_type_tab_moves_to_next() {
		let screen = change_type_screen(ChangeType::Major, vec![]);
		let result = handle_key(&screen, KeyCode::Tab, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);
	}

	#[test]
	fn change_type_h_moves_to_previous() {
		let screen = change_type_screen(ChangeType::Patch, vec![]);
		let result = handle_key(&screen, KeyCode::Char('h'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);
	}

	#[test]
	fn change_type_l_moves_to_next() {
		let screen = change_type_screen(ChangeType::Major, vec![]);
		let result = handle_key(&screen, KeyCode::Char('l'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);
	}

	#[test]
	fn change_type_enter_completes_with_selected() {
		let projects = dummy_projects(2);

		let screen = change_type_screen(ChangeType::Major, vec![0]);
		let result = handle_key(&screen, KeyCode::Enter, &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: vec![projects[0].clone()],
				change_type: ChangeType::Major,
			})
		);

		let screen = change_type_screen(ChangeType::Minor, vec![1]);
		let result = handle_key(&screen, KeyCode::Enter, &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: vec![projects[1].clone()],
				change_type: ChangeType::Minor,
			})
		);

		let screen = change_type_screen(ChangeType::Patch, vec![0, 1]);
		let result = handle_key(&screen, KeyCode::Enter, &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: projects.clone(),
				change_type: ChangeType::Patch,
			})
		);
	}

	#[test]
	fn change_type_m_selects_major() {
		let projects = dummy_projects(1);
		let screen = change_type_screen(ChangeType::Patch, vec![0]);
		let result = handle_key(&screen, KeyCode::Char('m'), &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: projects.clone(),
				change_type: ChangeType::Major,
			})
		);
	}

	#[test]
	fn change_type_i_selects_minor() {
		let projects = dummy_projects(1);
		let screen = change_type_screen(ChangeType::Patch, vec![0]);
		let result = handle_key(&screen, KeyCode::Char('i'), &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: projects.clone(),
				change_type: ChangeType::Minor,
			})
		);
	}

	#[test]
	fn change_type_p_selects_patch() {
		let projects = dummy_projects(1);
		let screen = change_type_screen(ChangeType::Major, vec![0]);
		let result = handle_key(&screen, KeyCode::Char('p'), &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: projects.clone(),
				change_type: ChangeType::Patch,
			})
		);
	}

	#[test]
	fn change_type_esc_cancels() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Esc, &[]).unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn change_type_q_cancels() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Char('q'), &[]).unwrap();
		assert_eq!(result, KeyResult::Cancelled);
	}

	#[test]
	fn change_type_other_keys_do_nothing() {
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		let result = handle_key(&screen, KeyCode::Char('x'), &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);

		let result = handle_key(&screen, KeyCode::Up, &[]).unwrap();
		assert_eq!(
			result,
			KeyResult::Continue(change_type_screen(ChangeType::Minor, vec![]))
		);
	}

	#[test]
	fn change_type_out_of_bounds_index_returns_error() {
		let projects = dummy_projects(1);
		let screen = change_type_screen(ChangeType::Patch, vec![99]);
		let err = handle_key(&screen, KeyCode::Enter, &projects).unwrap_err();
		let msg = err.to_string();
		assert!(
			msg.contains("99"),
			"error should mention the bad index: {msg}"
		);
		assert!(
			msg.contains("1 projects"),
			"error should mention the slice length: {msg}"
		);
	}

	#[test]
	fn prefilled_projects_initial_screen_completes_correctly() {
		let projects = dummy_projects(3);
		let screen = change_type_screen(ChangeType::Patch, vec![0, 2]);
		let result = handle_key(&screen, KeyCode::Enter, &projects).unwrap();
		assert_eq!(
			result,
			KeyResult::Complete(ChangeResult {
				projects: vec![projects[0].clone(), projects[2].clone()],
				change_type: ChangeType::Patch,
			})
		);
	}

	#[test]
	fn ui_renders_select_change_type_screen() {
		let mut terminal = create_test_terminal();
		let names: Vec<&str> = vec![];
		let screen = change_type_screen(ChangeType::Major, vec![]);
		terminal
			.draw(|frame| super::super::ui(frame, &screen, &names))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Major"));
		assert!(content.contains("Minor"));
		assert!(content.contains("Patch"));
	}

	#[test]
	fn ui_renders_change_type_with_minor_selected() {
		let mut terminal = create_test_terminal();
		let names: Vec<&str> = vec![];
		let screen = change_type_screen(ChangeType::Minor, vec![]);
		terminal
			.draw(|frame| super::super::ui(frame, &screen, &names))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("What type of change"));
	}

	#[test]
	fn ui_renders_change_type_with_patch_selected() {
		let mut terminal = create_test_terminal();
		let names: Vec<&str> = vec![];
		let screen = change_type_screen(ChangeType::Patch, vec![]);
		terminal
			.draw(|frame| super::super::ui(frame, &screen, &names))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Change Type"));
	}
}
