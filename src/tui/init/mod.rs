//! TUI wizard for initialising a Chronicle configuration.

use std::path::Path;

use crossterm::event::KeyEvent;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};
use ratatui_textarea::TextArea;

use super::widgets::{self, KeyResult};
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
	git_workdir: std::path::PathBuf,
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
fn detect_package_managers(git_workdir: &Path) -> (bool, bool) {
	let cargo = git_workdir.join("Cargo.toml").exists();
	let npm = git_workdir.join("package.json").exists();
	(cargo, npm)
}

fn handle_key(state: WizardState, screen: Screen, key: KeyEvent) -> HandleResult {
	match screen {
		Screen::ConfirmOverwrite(yes) => {
			confirm_overwrite::handle_confirm_overwrite(state, yes, key)
		}
		Screen::SelectPackageManagers { cargo, npm, focus } => {
			select_pms::handle_select_pms(state, cargo, npm, focus, key)
		}
		Screen::ManifestPath { pm, textarea } => {
			manifest_path::handle_manifest_path(state, pm, textarea, key)
		}
		Screen::EnableGit(yes) => enable_git::handle_enable_git(state, yes, key),
		Screen::GitStrategy(strategy) => git_strategy::handle_git_strategy(state, strategy, key),
		Screen::EnableGitHub(yes) => enable_github::handle_enable_github(state, yes, key),
		Screen::EditGitHub { textarea, error } => {
			edit_github::handle_edit_github(state, textarea, error, key)
		}
		Screen::OpenEditor(yes) => open_editor::handle_open_editor(state, yes, key),
	}
}

fn ui(frame: &mut Frame, _state: &WizardState, screen: &Screen) {
	match screen {
		Screen::ConfirmOverwrite(yes) => confirm_overwrite::render_confirm_overwrite(frame, *yes),
		Screen::SelectPackageManagers { cargo, npm, focus } => {
			select_pms::render_select_pms(frame, *cargo, *npm, *focus)
		}
		Screen::ManifestPath { pm, textarea } => {
			manifest_path::render_manifest_path(frame, *pm, textarea)
		}
		Screen::EnableGit(yes) => enable_git::render_enable_git(frame, *yes),
		Screen::GitStrategy(strategy) => git_strategy::render_git_strategy(frame, *strategy),
		Screen::EnableGitHub(yes) => enable_github::render_enable_github(frame, *yes),
		Screen::EditGitHub { textarea, error } => {
			edit_github::render_edit_github(frame, textarea, *error)
		}
		Screen::OpenEditor(yes) => open_editor::render_open_editor(frame, *yes),
	}
}

/// Runs the interactive TUI init wizard for Chronicle configuration.
///
/// Guides the user through selecting package managers, manifest paths,
/// git automation, GitHub integration, and opening the config file in an editor.
///
/// # Returns
///
/// Returns `Ok(Some(InitResult))` if the user completes setup, or `Ok(None)` if
/// the user cancels.
///
/// # Errors
///
/// Returns an error if terminal setup or I/O operations fail.
pub fn run(git_workdir: &AbsolutePath, env: &Env) -> anyhow::Result<Option<InitResult>> {
	let (cargo_detected, npm_detected) = detect_package_managers(git_workdir.as_ref());
	let initial_npm = npm_detected || !cargo_detected;

	let git = GitWorkdir::new(env, git_workdir.clone());
	let detected_github = crate::github::remote::GitHubRepo::detect_in(&git)
		.ok()
		.flatten();

	let initial_state = WizardState {
		git_workdir: git_workdir.as_ref().to_path_buf(),
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

	let initial_screen = if config_exists(git_workdir.as_ref()) {
		Screen::ConfirmOverwrite(false)
	} else {
		Screen::SelectPackageManagers {
			cargo: cargo_detected,
			npm: initial_npm,
			focus: PmFocus::Cargo,
		}
	};

	widgets::run_tui(
		(initial_state, initial_screen),
		|frame, (state, screen)| ui(frame, state, screen),
		|(state, screen), key| handle_key(state, screen, key),
	)
}

/// Shared test utilities used by all screen submodule test suites.
#[cfg(test)]
pub(super) mod test_helpers {
	use super::*;
	use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
	use tempfile::TempDir;

	pub(super) fn key(code: KeyCode) -> KeyEvent {
		KeyEvent::new(code, KeyModifiers::NONE)
	}

	pub(super) fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	pub(super) fn make_state(dir: &TempDir) -> WizardState {
		WizardState {
			git_workdir: dir.path().to_path_buf(),
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

	// --- detect_package_managers ---

	#[test]
	fn detect_package_managers_defaults_to_neither() {
		let dir = temp_dir();
		let (cargo, npm) = detect_package_managers(dir.path());
		assert!(!cargo);
		assert!(!npm);
	}

	#[test]
	fn detect_package_managers_detects_cargo() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		let (cargo, npm) = detect_package_managers(dir.path());
		assert!(cargo);
		assert!(!npm);
	}

	#[test]
	fn detect_package_managers_detects_npm() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("package.json"), "{}").unwrap();
		let (cargo, npm) = detect_package_managers(dir.path());
		assert!(!cargo);
		assert!(npm);
	}

	#[test]
	fn detect_package_managers_detects_both() {
		let dir = temp_dir();
		std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
		std::fs::write(dir.path().join("package.json"), "{}").unwrap();
		let (cargo, npm) = detect_package_managers(dir.path());
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
}
