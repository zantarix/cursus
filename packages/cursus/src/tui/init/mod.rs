//! TUI wizard for initialising a Cursus configuration.

use crossterm::event::Event;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};
use ratatui_textarea::TextArea;

use super::screens::ButtonScreen;
use super::widgets::{self, KeyResult, TabStatus};
use crate::Env;
use crate::forge::github::GitHubRepo;
use crate::forge::gitlab::remote::GitLabProject;
use crate::model::config::{PackageManager, Strategy};
use crate::path::AbsolutePath;

mod choose_forge;
mod confirm_overwrite;
mod edit_github;
mod edit_gitlab;
mod enable_git;
mod git_strategy;
mod manifest_path;
mod open_editor;
mod select_pms;

#[cfg(test)]
mod tests;

use choose_forge::ForgeChoice;

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
	/// Whether GitLab releases integration is enabled.
	pub gitlab_enabled: bool,
	/// GitLab group (or namespace) path, if explicitly confirmed by the user.
	///
	/// `None` means "auto-detect from git remote". Supports subgroup paths
	/// (e.g. `acme/sub`). When `None` and `detected_gitlab_group` is `Some`,
	/// the template renders the detected value as a commented-out hint.
	pub gitlab_group: Option<String>,
	/// GitLab project name, if explicitly confirmed by the user.
	///
	/// `None` means "auto-detect from git remote". When `None` and
	/// `detected_gitlab_project` is `Some`, the template renders the detected
	/// value as a commented-out hint.
	pub gitlab_project: Option<String>,
	/// GitLab host for self-managed instances (e.g. `gitlab.example.com`).
	///
	/// `None` means the gitlab.com default is used. When `Some`, the template
	/// emits the value as `host = "..."`.
	pub gitlab_host: Option<String>,
	/// Auto-detected GitLab group from the git remote, for use as a template hint.
	pub detected_gitlab_group: Option<String>,
	/// Auto-detected GitLab project from the git remote, for use as a template hint.
	pub detected_gitlab_project: Option<String>,
	/// Auto-detected GitLab host from the git remote, for use as a template hint.
	pub detected_gitlab_host: Option<String>,
	/// Whether to open the config file in an editor after writing.
	pub open_editor: bool,
}

/// Internal state accumulated as the wizard progresses.
#[derive(Debug, Clone)]
pub(crate) struct WizardState {
	pub(crate) env: crate::Env,
	pub(crate) dry_run: bool,
	pub(crate) cargo_enabled: bool,
	pub(crate) npm_enabled: bool,
	pub(crate) cargo_path: Option<String>,
	pub(crate) npm_path: Option<String>,
	pub(crate) git_enabled: bool,
	pub(crate) git_strategy: Option<Strategy>,
	pub(crate) github_enabled: bool,
	pub(crate) github_owner: Option<String>,
	pub(crate) github_repo: Option<String>,
	pub(crate) detected_github: Option<GitHubRepo>,
	pub(crate) gitlab_enabled: bool,
	pub(crate) gitlab_group: Option<String>,
	pub(crate) gitlab_project: Option<String>,
	pub(crate) gitlab_host: Option<String>,
	pub(crate) detected_gitlab: Option<GitLabProject>,
	pub(crate) remaining_manifest_pms: Vec<PackageManager>,
}

/// Keyboard focus within the [`Screen::SelectPackageManagers`] screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PmFocus {
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
pub(crate) enum Screen {
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
	ChooseForge(ForgeChoice),
	EditGitHub {
		textarea: TextArea<'static>,
		error: bool,
	},
	EditGitLab(Box<edit_gitlab::EditGitLabState>),
	OpenEditor(bool),
}

pub(crate) type HandleResult = anyhow::Result<KeyResult<(WizardState, Screen), InitResult>>;

/// Creates a [`TextArea`] with a standard bordered block.
pub(crate) fn bordered_textarea() -> TextArea<'static> {
	let mut ta = TextArea::default();
	ta.set_block(Block::default().borders(Borders::ALL));
	ta
}

/// Advances to the next [`Screen::ManifestPath`] screen or [`Screen::EnableGit`],
/// depending on whether any package managers still need a manifest path.
pub(crate) fn advance_from_manifest_queue(mut state: WizardState) -> (WizardState, Screen) {
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
pub(crate) fn complete(state: WizardState, open_editor: bool) -> InitResult {
	let detected_github_owner = state.detected_github.as_ref().map(|gh| gh.owner.clone());
	let detected_github_repo = state.detected_github.as_ref().map(|gh| gh.repo.clone());
	let detected_gitlab_group = state.detected_gitlab.as_ref().map(|gl| gl.group.clone());
	let detected_gitlab_project = state.detected_gitlab.as_ref().map(|gl| gl.project.clone());
	let detected_gitlab_host = state.detected_gitlab.as_ref().map(|gl| gl.host.clone());
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
		gitlab_enabled: state.gitlab_enabled,
		gitlab_group: state.gitlab_group,
		gitlab_project: state.gitlab_project,
		gitlab_host: state.gitlab_host,
		detected_gitlab_group,
		detected_gitlab_project,
		detected_gitlab_host,
		open_editor,
	}
}

/// Detects which package managers have manifest files at the git root.
///
/// Returns `(cargo_detected, npm_detected)`.
pub(crate) fn detect_package_managers(git_workdir: &AbsolutePath) -> (bool, bool) {
	let cargo = git_workdir.child("Cargo.toml").as_path().exists();
	let npm = git_workdir.child("package.json").as_path().exists();
	(cargo, npm)
}

pub(crate) fn handle_event(
	state: WizardState,
	screen: Screen,
	event: Event,
	content_area: Rect,
) -> HandleResult {
	let result =
		match screen {
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
			Screen::GitStrategy(strategy) => git_strategy::GitStrategyButtons { strategy }
				.handle_event(state, event, content_area),
			Screen::ChooseForge(selected) => choose_forge::ChooseForgeButtons { selected }
				.handle_event(state, event, content_area),
			Screen::EditGitHub { textarea, error } => {
				edit_github::handle_edit_github(state, textarea, error, event)
			}
			Screen::EditGitLab(s) => edit_gitlab::handle_edit_gitlab(state, s, event),
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

/// Maps a screen to the `[Managers, Git, Forge]` tab statuses.
pub(crate) fn tab_states(screen: &Screen) -> [TabStatus; 3] {
	match screen {
		Screen::ConfirmOverwrite(_)
		| Screen::SelectPackageManagers { .. }
		| Screen::ManifestPath { .. } => [TabStatus::Current, TabStatus::Future, TabStatus::Future],
		Screen::EnableGit(_) | Screen::GitStrategy(_) => {
			[TabStatus::Completed, TabStatus::Current, TabStatus::Future]
		}
		Screen::ChooseForge(_)
		| Screen::EditGitHub { .. }
		| Screen::EditGitLab { .. }
		| Screen::OpenEditor(_) => [
			TabStatus::Completed,
			TabStatus::Completed,
			TabStatus::Current,
		],
	}
}

/// Returns the Fluent key for the third tab label based on the current screen.
fn forge_tab_key(screen: &Screen) -> &'static str {
	match screen {
		Screen::EditGitHub { .. } => "tab-github",
		Screen::EditGitLab { .. } => "tab-gitlab",
		_ => "tab-forge",
	}
}

pub(crate) const TAB_HEIGHT: u16 = 3;

fn render_tab_bar(frame: &mut Frame, tab_area: Rect, screen: &Screen) {
	let [managers, git, forge] = tab_states(screen);
	let managers_label_long = crate::t!("select-pms-tab-long");
	let managers_label_short = crate::t!("select-pms-tab-short");
	let managers_label = if tab_area.width >= 72 {
		managers_label_long.as_str()
	} else {
		managers_label_short.as_str()
	};
	let tab_git = crate::t!("tab-git");
	let tab_forge = crate::t!(forge_tab_key(screen));
	widgets::render_tabs(
		frame,
		tab_area,
		&[
			(managers_label, managers),
			(tab_git.as_str(), git),
			(tab_forge.as_str(), forge),
		],
	);
}

#[allow(clippy::too_many_lines)]
pub(crate) fn ui(frame: &mut Frame, _state: &WizardState, screen: &Screen) {
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
		Screen::ChooseForge(selected) => choose_forge::ChooseForgeButtons {
			selected: *selected,
		}
		.render(frame, content_area),
		Screen::EditGitHub { textarea, error } => {
			edit_github::render_edit_github(frame, content_area, textarea, *error)
		}
		Screen::EditGitLab(s) => edit_gitlab::render_edit_gitlab(frame, content_area, s),
		Screen::OpenEditor(yes) => {
			open_editor::OpenEditorButtons { yes: *yes }.render(frame, content_area)
		}
	}
}

/// Runs the interactive TUI init wizard for Cursus configuration.
///
/// Guides the user through selecting package managers, manifest paths,
/// git automation, forge selection (GitHub, GitLab, or Neither), per-forge
/// configuration, and opening the config file in an editor.
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
	env: &Env,
	dry_run: bool,
	detected_github: Option<crate::forge::github::remote::GitHubRepo>,
	detected_gitlab: Option<GitLabProject>,
) -> anyhow::Result<Option<InitResult>> {
	let git = env.git();
	let (cargo_detected, npm_detected) = detect_package_managers(git.path());

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
		gitlab_enabled: false,
		gitlab_group: None,
		gitlab_project: None,
		gitlab_host: None,
		detected_gitlab,
		remaining_manifest_pms: Vec::new(),
	};

	let config_path = git.path().child(".cursus/config.toml");
	let initial_screen = if !dry_run && config_path.as_path().exists() {
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
pub(crate) fn handle_key(
	state: WizardState,
	screen: Screen,
	k: crossterm::event::KeyEvent,
) -> HandleResult {
	handle_event(
		state,
		screen,
		Event::Key(k),
		Rect::new(0, TAB_HEIGHT, 80, 24 - TAB_HEIGHT),
	)
}

/// Shared test utilities used by all screen submodule test suites.
#[cfg(test)]
pub(crate) mod test_helpers {
	use super::*;
	use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
	use tempfile::TempDir;

	pub(crate) use crate::tui::test_utils::mouse_click;

	/// Content area matching an 80×24 terminal with the 3-row tab bar subtracted.
	pub(crate) const fn test_content_area() -> Rect {
		Rect::new(0, TAB_HEIGHT, 80, 24 - TAB_HEIGHT)
	}

	pub(crate) fn key(code: KeyCode) -> KeyEvent {
		KeyEvent::new(code, KeyModifiers::NONE)
	}

	pub(crate) fn temp_dir() -> TempDir {
		tempfile::tempdir().expect("Failed to create temp dir")
	}

	pub(crate) fn make_state(dir: &TempDir) -> WizardState {
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
			gitlab_enabled: false,
			gitlab_group: None,
			gitlab_project: None,
			gitlab_host: None,
			detected_gitlab: None,
			remaining_manifest_pms: Vec::new(),
		}
	}

	pub(crate) fn unwrap_continue(result: HandleResult) -> (WizardState, Screen) {
		match result.unwrap() {
			KeyResult::Continue(s) => s,
			other => panic!("Expected Continue, got {other:?}"),
		}
	}

	pub(crate) fn unwrap_complete(result: HandleResult) -> InitResult {
		match result.unwrap() {
			KeyResult::Complete(r) => r,
			other => panic!("Expected Complete, got {other:?}"),
		}
	}

	pub(crate) fn assert_cancelled(result: HandleResult) {
		assert!(
			matches!(result.unwrap(), KeyResult::Cancelled),
			"Expected Cancelled"
		);
	}
}
