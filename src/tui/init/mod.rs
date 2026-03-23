//! TUI wizard for initialising a Cursus configuration.

use crossterm::event::Event;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};
use ratatui_textarea::TextArea;

use super::screens::ButtonScreen;
use super::widgets::{self, KeyResult, TabStatus};
use crate::Env;
use crate::git::GitWorkdir;
use crate::github::GitHubRepo;
use crate::model::config::{PackageManager, Strategy, exists as config_exists};
use crate::path::AbsolutePath;

mod confirm_overwrite;
mod edit_github;
mod enable_git;
mod enable_github;
mod git_strategy;
mod manifest_path;
mod open_editor;
mod select_pms;

/// The result of completing the init wizard.
#[derive(Debug, Clone)]
pub struct InitResult {
	/// Whether Cargo is enabled as a package manager.
	pub cargo_enabled: bool,
	/// Whether npm is enabled as a package manager.
	pub npm_enabled: bool,
	/// Optional path override for the Cargo workspace root.
	pub cargo_path: Option<String>,
	/// Optional path override for the npm workspace root.
	pub npm_path: Option<String>,
	/// Whether git lifecycle automation is enabled.
	pub git_enabled: bool,
	/// Git automation strategy, if git is enabled.
	pub git_strategy: Option<Strategy>,
	/// Whether GitHub Releases integration is enabled.
	pub github_enabled: bool,
	/// GitHub repository owner, if explicitly confirmed by the user.
	///
	/// `None` means "auto-detect from git remote". When `None` and
	/// `detected_github_owner` is `Some`, the template renders the detected
	/// value as a commented-out hint.
	pub github_owner: Option<String>,
	/// GitHub repository name, if explicitly confirmed by the user.
	///
	/// `None` means "auto-detect from git remote". When `None` and
	/// `detected_github_repo` is `Some`, the template renders the detected
	/// value as a commented-out hint.
	pub github_repo: Option<String>,
	/// Auto-detected GitHub owner from the git remote, for use as a template hint.
	pub detected_github_owner: Option<String>,
	/// Auto-detected GitHub repo from the git remote, for use as a template hint.
	pub detected_github_repo: Option<String>,
	/// Whether to open the config file in an editor after writing.
	pub open_editor: bool,
}

/// Internal state accumulated as the wizard progresses.
#[derive(Debug, Clone)]
struct WizardState {
	env: crate::Env,
	dry_run: bool,
	cargo_enabled: bool,
	npm_enabled: bool,
	cargo_path: Option<String>,
	npm_path: Option<String>,
	git_enabled: bool,
	git_strategy: Option<Strategy>,
	github_enabled: bool,
	github_owner: Option<String>,
	github_repo: Option<String>,
	detected_github: Option<GitHubRepo>,
	remaining_manifest_pms: Vec<PackageManager>,
}

/// Keyboard focus within the [`Screen::SelectPackageManagers`] screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PmFocus {
	Cargo,
	Npm,
}

impl PmFocus {
	fn toggle(self) -> Self {
		match self {
			Self::Cargo => Self::Npm,
			Self::Npm => Self::Cargo,
		}
	}
}

/// TUI wizard screens.
#[derive(Debug)]
enum Screen {
	ConfirmOverwrite(bool),
	SelectPackageManagers {
		cargo: bool,
		npm: bool,
		focus: PmFocus,
	},
	ManifestPath {
		pm: PackageManager,
		textarea: TextArea<'static>,
	},
	EnableGit(bool),
	GitStrategy(Strategy),
	EnableGitHub(bool),
	EditGitHub {
		textarea: TextArea<'static>,
		error: bool,
	},
	OpenEditor(bool),
}

type HandleResult = anyhow::Result<KeyResult<(WizardState, Screen), InitResult>>;

/// Creates a [`TextArea`] with a standard bordered block.
fn bordered_textarea() -> TextArea<'static> {
	let mut ta = TextArea::default();
	ta.set_block(Block::default().borders(Borders::ALL));
	ta
}

/// Advances to the next [`Screen::ManifestPath`] screen or [`Screen::EnableGit`],
/// depending on whether any package managers still need a manifest path.
fn advance_from_manifest_queue(mut state: WizardState) -> (WizardState, Screen) {
	if state.remaining_manifest_pms.is_empty() {
		return (state, Screen::EnableGit(true));
	}
	let pm = state.remaining_manifest_pms.remove(0);
	(
		state,
		Screen::ManifestPath {
			pm,
			textarea: bordered_textarea(),
		},
	)
}

/// Converts the accumulated state plus the final `open_editor` flag into an [`InitResult`].
fn complete(state: WizardState, open_editor: bool) -> InitResult {
	let detected_github_owner = state.detected_github.as_ref().map(|gh| gh.owner.clone());
	let detected_github_repo = state.detected_github.as_ref().map(|gh| gh.repo.clone());
	InitResult {
		cargo_enabled: state.cargo_enabled,
		npm_enabled: state.npm_enabled,
		cargo_path: state.cargo_path,
		npm_path: state.npm_path,
		git_enabled: state.git_enabled,
		git_strategy: state.git_strategy,
		github_enabled: state.github_enabled,
		github_owner: state.github_owner,
		github_repo: state.github_repo,
		detected_github_owner,
		detected_github_repo,
		open_editor,
	}
}

/// Detects which package managers have manifest files at the git root.
///
/// Returns `(cargo_detected, npm_detected)`.
fn detect_package_managers(
	git_workdir: &AbsolutePath,
	fs: &dyn crate::filesystem::Filesystem,
) -> (bool, bool) {
	let cargo = fs.exists(&git_workdir.child("Cargo.toml"));
	let npm = fs.exists(&git_workdir.child("package.json"));
	(cargo, npm)
}

fn handle_event(
	state: WizardState,
	screen: Screen,
	event: Event,
	content_area: Rect,
) -> HandleResult {
	let result = match screen {
		Screen::ConfirmOverwrite(yes) => confirm_overwrite::ConfirmOverwriteButtons { yes }
			.handle_event(state, event, content_area),
		Screen::SelectPackageManagers { cargo, npm, focus } => {
			select_pms::handle_select_pms(state, cargo, npm, focus, event, content_area)
		}
		Screen::ManifestPath { pm, textarea } => {
			manifest_path::handle_manifest_path(state, pm, textarea, event)
		}
		Screen::EnableGit(yes) => {
			enable_git::EnableGitButtons { yes }.handle_event(state, event, content_area)
		}
		Screen::GitStrategy(strategy) => {
			git_strategy::GitStrategyButtons { strategy }.handle_event(state, event, content_area)
		}
		Screen::EnableGitHub(yes) => {
			enable_github::EnableGitHubButtons { yes }.handle_event(state, event, content_area)
		}
		Screen::EditGitHub { textarea, error } => {
			edit_github::handle_edit_github(state, textarea, error, event)
		}
		Screen::OpenEditor(yes) => {
			open_editor::OpenEditorButtons { yes }.handle_event(state, event, content_area)
		}
	}?;
	// In dry-run mode the config is never written to disk, so skip the
	// OpenEditor screen and complete immediately with open_editor = false.
	Ok(match result {
		KeyResult::Continue((state, Screen::OpenEditor(_))) if state.dry_run => {
			KeyResult::Complete(complete(state, false))
		}
		other => other,
	})
}

/// Maps a screen to the `[Managers, Git, GitHub]` tab statuses.
fn tab_states(screen: &Screen) -> [TabStatus; 3] {
	match screen {
		Screen::ConfirmOverwrite(_)
		| Screen::SelectPackageManagers { .. }
		| Screen::ManifestPath { .. } => [TabStatus::Current, TabStatus::Future, TabStatus::Future],
		Screen::EnableGit(_) | Screen::GitStrategy(_) => {
			[TabStatus::Completed, TabStatus::Current, TabStatus::Future]
		}
		Screen::EnableGitHub(_) | Screen::EditGitHub { .. } | Screen::OpenEditor(_) => [
			TabStatus::Completed,
			TabStatus::Completed,
			TabStatus::Current,
		],
	}
}

const TAB_HEIGHT: u16 = 3;

fn render_tab_bar(frame: &mut Frame, tab_area: Rect, screen: &Screen) {
	let [managers, git, github] = tab_states(screen);
	let managers_label_long = crate::t!("select-pms-tab-long");
	let managers_label_short = crate::t!("select-pms-tab-short");
	let managers_label = if tab_area.width >= 72 {
		managers_label_long.as_str()
	} else {
		managers_label_short.as_str()
	};
	let tab_git = crate::t!("tab-git");
	let tab_github = crate::t!("tab-github");
	widgets::render_tabs(
		frame,
		tab_area,
		&[
			(managers_label, managers),
			(tab_git.as_str(), git),
			(tab_github.as_str(), github),
		],
	);
}

fn ui(frame: &mut Frame, _state: &WizardState, screen: &Screen) {
	let full = frame.area();
	let tab_area = Rect {
		height: TAB_HEIGHT,
		..full
	};
	render_tab_bar(frame, tab_area, screen);
	let content_area = Rect {
		y: full.y + TAB_HEIGHT,
		height: full.height.saturating_sub(TAB_HEIGHT),
		..full
	};

	match screen {
		Screen::ConfirmOverwrite(yes) => {
			confirm_overwrite::ConfirmOverwriteButtons { yes: *yes }.render(frame, content_area)
		}
		Screen::SelectPackageManagers { cargo, npm, focus } => {
			select_pms::render_select_pms(frame, content_area, *cargo, *npm, *focus)
		}
		Screen::ManifestPath { pm, textarea } => {
			manifest_path::render_manifest_path(frame, content_area, *pm, textarea)
		}
		Screen::EnableGit(yes) => {
			enable_git::EnableGitButtons { yes: *yes }.render(frame, content_area)
		}
		Screen::GitStrategy(strategy) => git_strategy::GitStrategyButtons {
			strategy: *strategy,
		}
		.render(frame, content_area),
		Screen::EnableGitHub(yes) => {
			enable_github::EnableGitHubButtons { yes: *yes }.render(frame, content_area)
		}
		Screen::EditGitHub { textarea, error } => {
			edit_github::render_edit_github(frame, content_area, textarea, *error)
		}
		Screen::OpenEditor(yes) => {
			open_editor::OpenEditorButtons { yes: *yes }.render(frame, content_area)
		}
	}
}

/// Runs the interactive TUI init wizard for Cursus configuration.
///
/// Guides the user through selecting package managers, manifest paths,
/// git automation, GitHub integration, and opening the config file in an editor.
///
/// When `dry_run` is `true`, the wizard skips the overwrite-confirmation prompt
/// (since nothing will be written) and skips the "open in editor" screen (since
/// there is no file to open).
///
/// # Returns
///
/// Returns `Ok(Some(InitResult))` if the user completes setup, or `Ok(None)` if
/// the user cancels.
///
/// # Errors
///
/// Returns an error if terminal setup or I/O operations fail.
pub fn run(
	git_workdir: &AbsolutePath,
	env: &Env,
	dry_run: bool,
) -> anyhow::Result<Option<InitResult>> {
	let (cargo_detected, npm_detected) = detect_package_managers(git_workdir, env.fs());

	let git = GitWorkdir::new(env.runner(), git_workdir.clone());
	let detected_github = crate::github::remote::GitHubRepo::detect_in(&git)
		.ok()
		.flatten();

	let initial_state = WizardState {
		env: env.clone(),
		dry_run,
		cargo_enabled: false,
		npm_enabled: false,
		cargo_path: None,
		npm_path: None,
		git_enabled: false,
		git_strategy: None,
		github_enabled: false,
		github_owner: None,
		github_repo: None,
		detected_github,
		remaining_manifest_pms: Vec::new(),
	};

	let initial_screen = if !dry_run && config_exists(env) {
		Screen::ConfirmOverwrite(false)
	} else {
		Screen::SelectPackageManagers {
			cargo: cargo_detected,
			npm: npm_detected,
			focus: PmFocus::Cargo,
		}
	};

	widgets::run_tui(
		(initial_state, initial_screen),
		|frame, (state, screen)| ui(frame, state, screen),
		|(state, screen), event, frame_area| {
			let content_area = Rect {
				y: frame_area.y + TAB_HEIGHT,
				height: frame_area.height.saturating_sub(TAB_HEIGHT),
				..frame_area
			};
			handle_event(state, screen, event, content_area)
		},
	)
}

/// Thin wrapper used by tests so existing test code can call `handle_key(state, screen, key)`
/// without needing to construct an `Event` or supply a content area.
#[cfg(test)]
fn handle_key(state: WizardState, screen: Screen, k: crossterm::event::KeyEvent) -> HandleResult {
	handle_event(
		state,
		screen,
		Event::Key(k),
		Rect::new(0, TAB_HEIGHT, 80, 24 - TAB_HEIGHT),
	)
}

/// Shared test utilities used by all screen submodule test suites.
#[cfg(test)]
pub(super) mod test_helpers {
	use super::*;
	use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
	use tempfile::TempDir;

	pub(super) use crate::tui::test_utils::mouse_click;

	/// Content area matching an 80×24 terminal with the 3-row tab bar subtracted.
	pub(super) const fn test_content_area() -> Rect {
		Rect::new(0, TAB_HEIGHT, 80, 24 - TAB_HEIGHT)
	}

	pub(super) fn key(code: KeyCode) -> KeyEvent {
		KeyEvent::new(code, KeyModifiers::NONE)
	}

	pub(super) fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	pub(super) fn make_state(dir: &TempDir) -> WizardState {
		use std::sync::Arc;
		let path = crate::path::AbsolutePath::new(dir.path()).unwrap();
		let runner: Arc<dyn crate::command::CommandRunner> =
			Arc::new(crate::command::test_support::RecordingCommandRunner::new(0));
		let git = Arc::new(crate::git::GitWorkdir::new(Arc::clone(&runner), path));
		let env = crate::Env::new(runner, Arc::new(crate::filesystem::LocalFilesystem), git);
		WizardState {
			env,
			dry_run: false,
			cargo_enabled: false,
			npm_enabled: false,
			cargo_path: None,
			npm_path: None,
			git_enabled: false,
			git_strategy: None,
			github_enabled: false,
			github_owner: None,
			github_repo: None,
			detected_github: None,
			remaining_manifest_pms: Vec::new(),
		}
	}

	pub(super) fn unwrap_continue(result: HandleResult) -> (WizardState, Screen) {
		match result.unwrap() {
			KeyResult::Continue(s) => s,
			other => panic!("Expected Continue, got {other:?}"),
		}
	}

	pub(super) fn unwrap_complete(result: HandleResult) -> InitResult {
		match result.unwrap() {
			KeyResult::Complete(r) => r,
			other => panic!("Expected Complete, got {other:?}"),
		}
	}

	pub(super) fn assert_cancelled(result: HandleResult) {
		assert!(
			matches!(result.unwrap(), KeyResult::Cancelled),
			"Expected Cancelled"
		);
	}
}

#[cfg(test)]
mod tests {
	use crossterm::event::KeyCode;

	use super::test_helpers::*;
	use super::*;

	// --- tab_states ---

	#[test]
	fn tab_states_managers_screens_show_current_managers() {
		let [m, g, gh] = tab_states(&Screen::SelectPackageManagers {
			cargo: true,
			npm: false,
			focus: PmFocus::Cargo,
		});
		assert_eq!(m, TabStatus::Current);
		assert_eq!(g, TabStatus::Future);
		assert_eq!(gh, TabStatus::Future);
	}

	#[test]
	fn tab_states_git_screens_show_completed_managers() {
		let [m, g, gh] = tab_states(&Screen::EnableGit(true));
		assert_eq!(m, TabStatus::Completed);
		assert_eq!(g, TabStatus::Current);
		assert_eq!(gh, TabStatus::Future);
	}

	#[test]
	fn tab_states_github_screens_show_completed_git() {
		let [m, g, gh] = tab_states(&Screen::OpenEditor(false));
		assert_eq!(m, TabStatus::Completed);
		assert_eq!(g, TabStatus::Completed);
		assert_eq!(gh, TabStatus::Current);
	}

	// --- detect_package_managers ---

	#[test]
	fn detect_package_managers_defaults_to_neither() {
		let dir = temp_dir();
		let (cargo, npm) = detect_package_managers(
			&crate::path::AbsolutePath::new(dir.path()).unwrap(),
			&crate::filesystem::LocalFilesystem,
		);
		assert!(!cargo);
		assert!(!npm);
	}

	#[test]
	fn detect_package_managers_detects_cargo() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		let (cargo, npm) = detect_package_managers(
			&crate::path::AbsolutePath::new(dir.path()).unwrap(),
			&crate::filesystem::LocalFilesystem,
		);
		assert!(cargo);
		assert!(!npm);
	}

	#[test]
	fn detect_package_managers_detects_npm() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("package.json"), "{}").unwrap();
		let (cargo, npm) = detect_package_managers(
			&crate::path::AbsolutePath::new(dir.path()).unwrap(),
			&crate::filesystem::LocalFilesystem,
		);
		assert!(!cargo);
		assert!(npm);
	}

	#[test]
	fn detect_package_managers_detects_both() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		std::fs::write(dir.path().join("package.json"), "{}").unwrap();
		let (cargo, npm) = detect_package_managers(
			&crate::path::AbsolutePath::new(dir.path()).unwrap(),
			&crate::filesystem::LocalFilesystem,
		);
		assert!(cargo);
		assert!(npm);
	}

	// --- Workflow tests ---

	#[test]
	fn workflow_cargo_only_git_disabled() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: true,
			npm: false,
			focus: PmFocus::Cargo,
		};

		let (state, screen) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert!(matches!(screen, Screen::EnableGit(_)));

		let (state, screen) = unwrap_continue(handle_key(
			state,
			Screen::EnableGit(false),
			key(KeyCode::Enter),
		));
		assert!(matches!(screen, Screen::OpenEditor(_)));

		let result = unwrap_complete(handle_key(
			state,
			Screen::OpenEditor(false),
			key(KeyCode::Enter),
		));
		assert!(result.cargo_enabled);
		assert!(!result.npm_enabled);
		assert!(!result.git_enabled);
		assert!(!result.github_enabled);
		assert!(!result.open_editor);
	}

	#[test]
	fn workflow_branch_strategy_skips_enable_github() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		let mut state = make_state(&dir);
		state.git_enabled = true;

		let (state, screen) = unwrap_continue(handle_key(
			state,
			Screen::GitStrategy(Strategy::Branch),
			key(KeyCode::Enter),
		));
		assert!(state.github_enabled);
		assert!(matches!(screen, Screen::EditGitHub { .. }));

		let (_, screen) = unwrap_continue(handle_key(state, screen, key(KeyCode::Enter)));
		assert!(matches!(screen, Screen::OpenEditor(_)));
	}

	#[test]
	fn workflow_complete_state_preserved_in_result() {
		let dir = temp_dir();
		let mut state = make_state(&dir);
		state.cargo_enabled = true;
		state.npm_enabled = true;
		state.git_enabled = true;
		state.git_strategy = Some(Strategy::Push);
		state.github_enabled = true;
		state.github_owner = Some("acme".to_string());
		state.github_repo = Some("my-app".to_string());

		let result = unwrap_complete(handle_key(
			state,
			Screen::OpenEditor(true),
			key(KeyCode::Enter),
		));
		assert!(result.cargo_enabled);
		assert!(result.npm_enabled);
		assert!(result.git_enabled);
		assert_eq!(result.git_strategy, Some(Strategy::Push));
		assert!(result.github_enabled);
		assert_eq!(result.github_owner, Some("acme".to_string()));
		assert_eq!(result.github_repo, Some("my-app".to_string()));
		assert!(result.open_editor);
	}

	/// Catches the `state.dry_run` guard deletion at line 209: when dry_run is set,
	/// transitioning to OpenEditor should immediately Complete rather than Continue.
	#[test]
	fn dry_run_skips_open_editor_and_completes() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		let mut state = make_state(&dir);
		state.dry_run = true;
		// EnableGit(false) + Enter → would normally give OpenEditor, but dry_run
		// should intercept and immediately Complete.
		let result = handle_key(state, Screen::EnableGit(false), key(KeyCode::Enter));
		let init_result = unwrap_complete(result);
		assert!(!init_result.open_editor, "dry_run must skip the editor");
	}

	/// Catches the `>= 72`→`> 72` or `>= 72`→`>= 73` mutation at line 242:
	/// a terminal narrower than 72 columns must show the short label "Managers".
	#[test]
	fn narrow_terminal_uses_short_managers_label() {
		use ratatui::Terminal;
		use ratatui::backend::TestBackend;
		// Width 71 is below the 72-column threshold
		let backend = TestBackend::new(71, 24);
		let mut terminal = Terminal::new(backend).unwrap();
		let dir = temp_dir();
		let state = make_state(&dir);
		let screen = Screen::SelectPackageManagers {
			cargo: true,
			npm: false,
			focus: PmFocus::Cargo,
		};
		terminal.draw(|frame| ui(frame, &state, &screen)).unwrap();
		let content = crate::tui::test_utils::buffer_to_string(terminal.backend().buffer());
		// Only inspect the tab bar (first TAB_HEIGHT rows); the screen body can say
		// "Package Managers" independently of the tab label.
		let tab_area: String = content
			.lines()
			.take(TAB_HEIGHT as usize)
			.collect::<Vec<_>>()
			.join("\n");
		assert!(
			tab_area.contains("Managers"),
			"Short label must appear in tab on narrow terminal, got: {tab_area:?}"
		);
		assert!(
			!tab_area.contains("Package Managers"),
			"Long label must not appear in tab on narrow terminal, got: {tab_area:?}"
		);
	}
}
