use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ratatui::prelude::*;

use crate::model::config::Strategy;
use crate::tui::widgets::{self, ButtonDef, KeyResult};

use super::{HandleResult, Screen, WizardState, edit_github::make_edit_github_screen};

const QUESTION: &str =
	"Git strategy? Push: commit to current branch. Branch: create release branch (for PRs).";

fn enter_action(mut state: WizardState, strategy: Strategy) -> HandleResult {
	state.git_strategy = Some(strategy);
	match strategy {
		Strategy::Push => Ok(KeyResult::Continue((state, Screen::EnableGitHub(false)))),
		Strategy::Branch => {
			// Branch implies GitHub enabled
			state.github_enabled = true;
			let screen = make_edit_github_screen(&state);
			Ok(KeyResult::Continue((state, screen)))
		}
	}
}

/// Handles events for the [`Screen::GitStrategy`] screen.
///
/// Selecting `Branch` auto-enables GitHub integration and jumps directly to
/// [`Screen::EditGitHub`], skipping the [`Screen::EnableGitHub`] prompt.
pub(super) fn handle_git_strategy(
	state: WizardState,
	strategy: Strategy,
	event: Event,
	content_area: Rect,
) -> HandleResult {
	match event {
		Event::Key(KeyEvent { code, .. }) => match code {
			KeyCode::Left
			| KeyCode::Right
			| KeyCode::Tab
			| KeyCode::Char('h')
			| KeyCode::Char('l') => {
				let toggled = match strategy {
					Strategy::Push => Strategy::Branch,
					Strategy::Branch => Strategy::Push,
				};
				Ok(KeyResult::Continue((state, Screen::GitStrategy(toggled))))
			}
			KeyCode::Enter => enter_action(state, strategy),
			KeyCode::Esc | KeyCode::Char('q') => Ok(KeyResult::Cancelled),
			_ => Ok(KeyResult::Continue((state, Screen::GitStrategy(strategy)))),
		},
		Event::Mouse(me) if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) => {
			if let Some(idx) =
				widgets::button_click_index(content_area, QUESTION, 2, me.column, me.row)
			{
				let clicked_strategy = if idx == 0 {
					Strategy::Push
				} else {
					Strategy::Branch
				};
				enter_action(state, clicked_strategy)
			} else {
				Ok(KeyResult::Continue((state, Screen::GitStrategy(strategy))))
			}
		}
		_ => Ok(KeyResult::Continue((state, Screen::GitStrategy(strategy)))),
	}
}

/// Renders the [`Screen::GitStrategy`] screen.
pub(super) fn render_git_strategy(frame: &mut Frame, area: Rect, strategy: Strategy) {
	let chunks = widgets::wizard_layout(
		area,
		&[
			Constraint::Length(widgets::paragraph_height(QUESTION, area.width, 2)),
			Constraint::Length(3),
			Constraint::Length(1),
			Constraint::Min(1),
		],
	);
	widgets::render_question(frame, chunks[0], QUESTION, Color::Yellow);
	widgets::render_yes_no_buttons(
		frame,
		chunks[1],
		&[
			ButtonDef {
				label: "Push",
				selected: strategy == Strategy::Push,
				color: None,
			},
			ButtonDef {
				label: "Branch",
				selected: strategy == Strategy::Branch,
				color: None,
			},
		],
	);
	widgets::render_help(
		frame,
		chunks[3],
		"←/→/Tab or click to switch, Enter or click to confirm, Esc to cancel",
	);
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use crate::model::config::Strategy;

	use super::super::test_helpers::*;
	use super::super::{Screen, handle_key};
	use super::*;

	#[test]
	fn git_strategy_toggle() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (_, s) = unwrap_continue(handle_key(
			state,
			Screen::GitStrategy(Strategy::Push),
			key(KeyCode::Tab),
		));
		assert!(matches!(s, Screen::GitStrategy(Strategy::Branch)));
	}

	#[test]
	fn git_strategy_push_advances_to_enable_github() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::GitStrategy(Strategy::Push),
			key(KeyCode::Enter),
		));
		assert_eq!(new_state.git_strategy, Some(Strategy::Push));
		assert!(matches!(s, Screen::EnableGitHub(_)));
	}

	#[test]
	fn git_strategy_branch_skips_enable_github_and_enables_it() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let (new_state, s) = unwrap_continue(handle_key(
			state,
			Screen::GitStrategy(Strategy::Branch),
			key(KeyCode::Enter),
		));
		assert_eq!(new_state.git_strategy, Some(Strategy::Branch));
		assert!(new_state.github_enabled);
		assert!(matches!(s, Screen::EditGitHub { .. }));
	}

	#[test]
	fn git_strategy_esc_cancels() {
		let dir = temp_dir();
		let state = make_state(&dir);
		assert_cancelled(handle_key(
			state,
			Screen::GitStrategy(Strategy::Push),
			key(KeyCode::Esc),
		));
	}

	#[test]
	fn git_strategy_click_push_button_advances_to_enable_github() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (new_state, s) = unwrap_continue(handle_git_strategy(
			state,
			Strategy::Branch,
			mouse_click(10, area.y + 7),
			area,
		));
		assert_eq!(new_state.git_strategy, Some(Strategy::Push));
		assert!(matches!(s, Screen::EnableGitHub(_)));
	}

	#[test]
	fn git_strategy_click_branch_button_enables_github_and_goes_to_edit_github() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (new_state, s) = unwrap_continue(handle_git_strategy(
			state,
			Strategy::Push,
			mouse_click(65, area.y + 7),
			area,
		));
		assert_eq!(new_state.git_strategy, Some(Strategy::Branch));
		assert!(new_state.github_enabled);
		assert!(matches!(s, Screen::EditGitHub { .. }));
	}

	#[test]
	fn git_strategy_click_outside_does_nothing() {
		let dir = temp_dir();
		let state = make_state(&dir);
		let area = test_content_area();
		let (_, s) = unwrap_continue(handle_git_strategy(
			state,
			Strategy::Push,
			mouse_click(10, area.y + 18),
			area,
		));
		assert!(matches!(s, Screen::GitStrategy(Strategy::Push)));
	}

	#[test]
	fn ui_renders_git_strategy() {
		use crate::tui::test_utils::{buffer_to_string, create_test_terminal};
		let mut terminal = create_test_terminal();
		let dir = temp_dir();
		let state = make_state(&dir);
		terminal
			.draw(|frame| super::super::ui(frame, &state, &Screen::GitStrategy(Strategy::Push)))
			.unwrap();
		let content = buffer_to_string(terminal.backend().buffer());
		assert!(content.contains("Push"));
		assert!(content.contains("Branch"));
	}
}
