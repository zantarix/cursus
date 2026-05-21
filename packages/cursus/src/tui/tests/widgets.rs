use ratatui::backend::TestBackend;
use ratatui::prelude::*;

use crate::tui::test_utils::render_to_string;
use crate::tui::widgets::*;

fn make_terminal() -> Terminal<TestBackend> {
	Terminal::new(TestBackend::new(80, 5)).unwrap()
}

// paragraph_height tests
#[test]
fn paragraph_height_short_text_with_border_returns_minimum() {
	// "Hello" fits on one line at any reasonable width → border+1+border = 3
	assert_eq!(paragraph_height("Hello", 80, 2), 3);
}

#[test]
fn paragraph_height_wrapping_text_with_border_grows() {
	// At width 80, inner = 74 chars. 87-char string wraps to 2 lines → 4
	let long =
		"Git strategy? Push: commit to current branch. Branch: create release branch (for PRs).";
	assert_eq!(paragraph_height(long, 80, 2), 4);
}

#[test]
fn paragraph_height_zero_width_with_border_returns_minimum() {
	assert_eq!(paragraph_height("anything", 0, 2), 3);
}

#[test]
fn paragraph_height_no_border_short_text_returns_one() {
	assert_eq!(paragraph_height("help", 80, 0), 1);
}

#[test]
fn paragraph_height_no_border_zero_width_returns_one() {
	assert_eq!(paragraph_height("help", 0, 0), 1);
}

// button_style tests
#[test]
fn button_style_selected_is_green_bold_reversed() {
	assert_eq!(
		button_style(true),
		Style::default()
			.fg(Color::Green)
			.add_modifier(Modifier::BOLD | Modifier::REVERSED)
	);
}

#[test]
fn button_style_unselected_is_gray() {
	assert_eq!(button_style(false), Style::default().fg(Color::Gray));
}

// button_style_colored tests
#[test]
fn button_style_colored_selected_uses_given_color() {
	assert_eq!(
		button_style_colored(true, Color::Red),
		Style::default()
			.fg(Color::Red)
			.add_modifier(Modifier::BOLD | Modifier::REVERSED)
	);
}

#[test]
fn button_style_colored_unselected_is_gray_regardless_of_color() {
	assert_eq!(
		button_style_colored(false, Color::Red),
		Style::default().fg(Color::Gray)
	);
}

// render_question tests
#[test]
fn render_question_shows_text() {
	let mut terminal = make_terminal();
	let content = render_to_string(&mut terminal, |frame| {
		render_question(frame, frame.area(), "Is this correct?", Color::Yellow);
	});
	assert!(content.contains("Is this correct?"));
}

#[test]
fn render_question_renders_border() {
	let mut terminal = make_terminal();
	let content = render_to_string(&mut terminal, |frame| {
		render_question(frame, frame.area(), "Q", Color::Red);
	});
	// Bordered block renders corner characters
	assert!(content.contains('─') || content.contains('│') || content.contains('┌'));
}

// render_help tests
#[test]
fn render_help_shows_text() {
	let mut terminal = make_terminal();
	let content = render_to_string(&mut terminal, |frame| {
		render_help(frame, frame.area(), "Press Esc to cancel");
	});
	assert!(content.contains("Press Esc to cancel"));
}

// render_buttons tests
#[test]
fn render_buttons_shows_labels() {
	let backend = TestBackend::new(80, 5);
	let mut terminal = Terminal::new(backend).unwrap();
	let content = render_to_string(&mut terminal, |frame| {
		render_buttons(
			frame,
			frame.area(),
			&[
				ButtonDef {
					label: "Yes".to_string(),
					selected: true,
					color: None,
				},
				ButtonDef {
					label: "No".to_string(),
					selected: false,
					color: Some(Color::Red),
				},
			],
		);
	});
	assert!(content.contains("Yes"));
	assert!(content.contains("No"));
}

#[test]
fn render_buttons_empty_does_not_panic() {
	let mut terminal = make_terminal();
	terminal
		.draw(|frame| render_buttons(frame, frame.area(), &[]))
		.unwrap();
}

// render_tabs tests
#[test]
fn render_tabs_shows_all_labels() {
	let mut terminal = make_terminal();
	let content = render_to_string(&mut terminal, |frame| {
		render_tabs(
			frame,
			frame.area(),
			&[
				("Managers", TabStatus::Current),
				("Git", TabStatus::Future),
				("GitHub", TabStatus::Future),
			],
		);
	});
	assert!(content.contains("Managers"));
	assert!(content.contains("Git"));
	assert!(content.contains("GitHub"));
}

#[test]
fn render_tabs_empty_does_not_panic() {
	let mut terminal = make_terminal();
	terminal
		.draw(|frame| render_tabs(frame, frame.area(), &[]))
		.unwrap();
}

// button_click_index tests
#[test]
fn button_click_index_hits_first_button() {
	let area = Rect::new(0, 0, 80, 24);
	// question height=3, buttons area: y=5..8, x=2..78
	// First button occupies roughly x=2..39
	let idx = button_click_index(area, "test?", 2, 10, 6);
	assert_eq!(idx, Some(0));
}

#[test]
fn button_click_index_hits_second_button() {
	let area = Rect::new(0, 0, 80, 24);
	let idx = button_click_index(area, "test?", 2, 65, 6);
	assert_eq!(idx, Some(1));
}

#[test]
fn button_click_index_misses_above_buttons() {
	let area = Rect::new(0, 0, 80, 24);
	// Row 2 is inside the question block, not the button area
	let idx = button_click_index(area, "test?", 2, 10, 2);
	assert_eq!(idx, None);
}

#[test]
fn button_click_index_misses_below_buttons() {
	let area = Rect::new(0, 0, 80, 24);
	let idx = button_click_index(area, "test?", 2, 10, 15);
	assert_eq!(idx, None);
}

#[test]
fn button_click_index_zero_buttons_returns_none() {
	let area = Rect::new(0, 0, 80, 24);
	assert_eq!(button_click_index(area, "test?", 0, 10, 6), None);
}

// wizard_layout tests
#[test]
fn wizard_layout_returns_correct_chunk_count() {
	let area = Rect::new(0, 0, 80, 24);
	let chunks = wizard_layout(
		area,
		&[
			Constraint::Length(3),
			Constraint::Length(3),
			Constraint::Min(1),
		],
	);
	assert_eq!(chunks.len(), 3);
}

#[test]
fn wizard_layout_applies_margin() {
	let area = Rect::new(0, 0, 80, 24);
	let chunks = wizard_layout(area, &[Constraint::Min(0)]);
	// The single chunk should be inset by the 2-cell margin on each side
	assert!(chunks[0].x >= area.x + 2);
	assert!(chunks[0].y >= area.y + 2);
}
