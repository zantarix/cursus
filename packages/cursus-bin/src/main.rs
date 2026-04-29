#![feature(coverage_attribute)]

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser as _;
use cursus::command::{DryRunCommandRunner, RealCommandRunner, VerboseCommandRunner};

/// A minimal `log::Log` implementation that splits output by level.
///
/// Info/Debug/Trace go to stdout; Warn/Error go to stderr.
/// Formatting mirrors the previous fern configuration.
struct CliLogger {
	stderr_is_terminal: bool,
}

#[coverage(off)]
#[mutants::skip]
impl log::Log for CliLogger {
	/// Always returns `true`; actual level filtering is handled by
	/// [`log::set_max_level`] in [`init_logging`].
	fn enabled(&self, _: &log::Metadata) -> bool {
		true
	}

	fn log(&self, record: &log::Record) {
		use std::io::Write as _;
		let target = record.target();
		let args = record.args();
		match record.level() {
			log::Level::Info => {
				let _ = writeln!(std::io::stdout().lock(), "{args}");
			}
			log::Level::Warn => {
				let stderr = std::io::stderr();
				if self.stderr_is_terminal {
					let _ = writeln!(stderr.lock(), "\x1b[33m[warning] {args}\x1b[0m");
				} else {
					let _ = writeln!(stderr.lock(), "[warning] {args}");
				}
			}
			log::Level::Error => {
				let stderr = std::io::stderr();
				if self.stderr_is_terminal {
					let _ = writeln!(stderr.lock(), "\x1b[91m[error] {args}\x1b[0m");
				} else {
					let _ = writeln!(stderr.lock(), "[error] {args}");
				}
			}
			log::Level::Debug => {
				let _ = writeln!(std::io::stdout().lock(), "debug: {target}: {args}");
			}
			log::Level::Trace => {
				let _ = writeln!(std::io::stdout().lock(), "trace: {target}: {args}");
			}
		}
	}

	fn flush(&self) {
		use std::io::Write as _;
		let _ = std::io::stdout().flush();
		let _ = std::io::stderr().flush();
	}
}

static LOGGER: std::sync::OnceLock<CliLogger> = std::sync::OnceLock::new();

#[coverage(off)]
#[mutants::skip]
fn init_logging(level: log::LevelFilter) {
	use std::io::IsTerminal as _;
	let logger = LOGGER.get_or_init(|| CliLogger {
		stderr_is_terminal: std::io::stderr().is_terminal(),
	});
	if let Err(e) = log::set_logger(logger) {
		eprintln!("warning: failed to initialize logging: {e}");
	}
	log::set_max_level(level);
}

/// Maps parsed global flags to the corresponding [`log::LevelFilter`].
///
/// `-s` / `--silent` → `Error`, default → `Info`, `-v` → `Debug`, `-vv+` → `Trace`.
#[coverage(off)]
#[mutants::skip]
fn determine_log_level(global: &cursus::cli::GlobalArgs) -> log::LevelFilter {
	if global.silent {
		log::LevelFilter::Error
	} else {
		match global.verbose {
			0 => log::LevelFilter::Info,
			1 => log::LevelFilter::Debug,
			_ => log::LevelFilter::Trace,
		}
	}
}

/// Returns the first non-empty value from the given environment variables,
/// or `None` if none are set or all are empty.
#[coverage(off)]
#[mutants::skip]
fn env_first(vars: &[&str]) -> Option<String> {
	vars.iter()
		.find_map(|name| std::env::var(name).ok().filter(|s| !s.is_empty()))
}

/// Resolves the BCP 47 locale tag for user-visible messages.
///
/// Priority order:
/// 1. `CURSUS_LOCALE` environment variable (explicit override)
/// 2. System locale via `sys_locale::get_locale()` (cross-platform)
/// 3. `"en"` fallback
#[coverage(off)]
#[mutants::skip]
fn detect_locale() -> String {
	env_first(&["CURSUS_LOCALE"])
		.or_else(sys_locale::get_locale)
		.unwrap_or_else(|| cursus::locale::DEFAULT_LOCALE.to_string())
}

#[coverage(off)]
#[mutants::skip]
#[tokio::main]
async fn main() -> ExitCode {
	// Parse args exactly once. Logging is initialised immediately after so
	// that every subsequent operation benefits from the user-requested level.
	let cli = match cursus::cli::Cli::try_parse() {
		Ok(cli) => cli,
		Err(e) => {
			// Help / version requests also come through here; initialise
			// logging at the default level and let clap print the output.
			init_logging(log::LevelFilter::Info);
			if let Err(print_err) = e.print() {
				log::error!("failed to print help: {print_err:#}");
			}
			return if e.use_stderr() {
				ExitCode::FAILURE
			} else {
				ExitCode::SUCCESS
			};
		}
	};

	init_logging(determine_log_level(&cli.global));

	match try_main(cli).await {
		Ok(code) => code,
		Err(e) => {
			log::error!("{e:#}");
			ExitCode::FAILURE
		}
	}
}

/// Builds an [`octocrab::Octocrab`] client configured with standard timeouts.
fn build_octocrab(token: &str) -> Result<octocrab::Octocrab, octocrab::Error> {
	octocrab::Octocrab::builder()
		.personal_token(token.to_string())
		.set_connect_timeout(Some(std::time::Duration::from_secs(10)))
		.set_read_timeout(Some(std::time::Duration::from_secs(30)))
		.set_write_timeout(Some(std::time::Duration::from_secs(30)))
		.build()
}

/// Attempts to construct the code forge client from environment and config.
///
/// Returns `Ok(client)` when a token is present and the repo identity can be
/// resolved, or `Err(reason)` describing why the client is unavailable.
///
/// Accepts a pre-built `octocrab` instance (shared with the Git decorator when
/// signed commits are enabled) to avoid constructing a second HTTP client.
async fn resolve_forge_client(
	env: &cursus::Env,
	config: &Option<cursus::model::config::Config>,
	octocrab: Option<Arc<octocrab::Octocrab>>,
) -> Result<Arc<dyn cursus::github::client::CodeForgeClient>, String> {
	let octocrab = match octocrab {
		Some(o) => (*o).clone(),
		None => {
			let token = env_first(&["GH_TOKEN", "GITHUB_TOKEN"])
				.ok_or_else(|| "No GitHub token found (GH_TOKEN / GITHUB_TOKEN)".to_string())?;
			build_octocrab(&token).map_err(|e| format!("Failed to build GitHub client: {e}"))?
		}
	};
	let cfg = config
		.as_ref()
		.ok_or_else(|| "No configuration file found".to_string())?;
	let repo = cursus::github::remote::GitHubRepo::resolve(&cfg.github, env.git())
		.await
		.map_err(|e| format!("{e:#}"))?;
	Ok(
		Arc::new(cursus::github::OctocrabGitHubClient::new(octocrab, repo))
			as Arc<dyn cursus::github::client::CodeForgeClient>,
	)
}

/// Runs the application after CLI parsing and logging setup.
///
/// Separated from [`main`] so that all fallible operations can use `?`
/// with a single error-handling site in the caller.
async fn try_main(cli: cursus::cli::Cli) -> anyhow::Result<ExitCode> {
	let cwd = std::env::current_dir()?;
	let cwd_abs = cursus::path::AbsolutePath::new(&cwd)?;

	let runner: Arc<dyn cursus::command::CommandRunner> =
		Arc::new(VerboseCommandRunner::new(RealCommandRunner));
	let filesystem: Arc<dyn cursus::filesystem::Filesystem> =
		Arc::new(cursus::filesystem::LocalFilesystem);

	// Wrap the runner for dry-run BEFORE creating GitWorkdir so it
	// receives the wrapped runner.
	let runner = if cli.global.dry_run {
		Arc::new(DryRunCommandRunner::new(runner)) as Arc<dyn cursus::command::CommandRunner>
	} else {
		runner
	};

	let git_workdir = cursus::git::find_workdir(&cwd_abs, &*filesystem)
		.await
		.context("No git repository found")?;

	// Load config before constructing the final Git impl so that signed_commits
	// mode can be checked. Config loading only needs the filesystem and workdir path.
	let config = cursus::model::config::load(&*filesystem, &git_workdir).await?;

	let git_inner = Arc::new(cursus::git::GitWorkdir::new(
		Arc::clone(&runner),
		git_workdir,
	));

	// Build one octocrab client for the whole process; both the Git decorator
	// (signed commits) and the forge client (PRs/releases) share it.
	let octocrab: Option<Arc<octocrab::Octocrab>> = env_first(&["GH_TOKEN", "GITHUB_TOKEN"])
		.and_then(|t| build_octocrab(&t).ok())
		.map(Arc::new);

	let git = build_git(
		git_inner,
		Arc::clone(&filesystem),
		Arc::clone(&runner),
		&config,
		cli.global.dry_run,
		octocrab.clone(),
	)
	.await?;

	let editor = env_first(&["VISUAL", "EDITOR"]);
	let oidc_environment = env_first(&["ACTIONS_ID_TOKEN_REQUEST_URL", "CI_JOB_JWT_V2"]).is_some();
	let node_auth_token_present = env_first(&["NODE_AUTH_TOKEN"]).is_some();
	let cargo_registry_token_present = env_first(&["CARGO_REGISTRY_TOKEN"]).is_some();
	let locale = detect_locale();

	let env = cursus::Env::new(runner, filesystem, git)
		.with_editor_opt(editor)
		.with_oidc_environment(oidc_environment)
		.with_node_auth_token_present(node_auth_token_present)
		.with_cargo_registry_token_present(cargo_registry_token_present)
		.with_locale(locale);

	let forge_client = resolve_forge_client(&env, &config, octocrab).await;
	let env = env.with_code_forge_client_result(forge_client);

	cursus::run(cli, env, config).await
}

/// Constructs the `Git` implementation for the current environment.
///
/// Returns a [`cursus::git::SignedCommitGit`] decorator when
/// [`resolve_signed_commits_mode`] indicates the API path is warranted, or a plain
/// [`cursus::git::GitWorkdir`] otherwise.
#[coverage(off)]
#[mutants::skip]
async fn build_git(
	inner: Arc<cursus::git::GitWorkdir>,
	filesystem: Arc<dyn cursus::filesystem::Filesystem>,
	runner: Arc<dyn cursus::command::CommandRunner>,
	config: &Option<cursus::model::config::Config>,
	dry_run: bool,
	octocrab: Option<Arc<octocrab::Octocrab>>,
) -> anyhow::Result<Arc<dyn cursus::git::Git>> {
	let mode = config
		.as_ref()
		.map(|c| c.git.signed_commits)
		.unwrap_or_default();
	let on_gha = std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true");
	let use_api = resolve_signed_commits_mode(mode, octocrab.is_some(), on_gha);

	if use_api {
		let octocrab = octocrab.context(
			"GitHub token required for signed commits but none found (GH_TOKEN / GITHUB_TOKEN)",
		)?;
		let github_config = config
			.as_ref()
			.map(|c| c.github.clone())
			.unwrap_or_default();
		let repo = cursus::github::remote::GitHubRepo::resolve(&github_config, &*inner)
			.await
			.context("cannot enable signed commits: failed to determine GitHub repository")?;
		log::info!(
			"Routing git commit and push operations through the GitHub API for verified commits."
		);
		let g: Arc<dyn cursus::git::Git> = Arc::new(cursus::git::SignedCommitGit::new(
			inner, filesystem, octocrab, runner, repo.owner, repo.repo, dry_run,
		));
		Ok(g)
	} else {
		let g: Arc<dyn cursus::git::Git> = inner;
		Ok(g)
	}
}

/// Returns `true` when the GitHub API commit path should be engaged.
///
/// Pure function over the resolved mode, token presence, and GHA detection;
/// accepts these as parameters so the policy can be tested without env mocking.
///
/// `Auto` engages when `GITHUB_ACTIONS=true` AND a token is available.
/// `Force` engages whenever a token is available.
/// `Off` never engages.
///
/// Dry-run is intentionally NOT checked here — the [`cursus::git::SignedCommitGit`]
/// decorator is constructed even in dry-run mode but short-circuits all API calls
/// via its own explicit dry-run guard (ADR-050, ADR-017 exception).
pub(crate) fn resolve_signed_commits_mode(
	mode: cursus::model::config::SignedCommitsMode,
	token_present: bool,
	on_gha: bool,
) -> bool {
	use cursus::model::config::SignedCommitsMode;
	match mode {
		SignedCommitsMode::Off => false,
		SignedCommitsMode::Force => token_present,
		SignedCommitsMode::Auto => on_gha && token_present,
	}
}

#[cfg(test)]
mod tests {
	use cursus::model::config::SignedCommitsMode;

	use super::resolve_signed_commits_mode;

	#[test]
	fn off_always_false() {
		assert!(!resolve_signed_commits_mode(
			SignedCommitsMode::Off,
			true,
			true
		));
		assert!(!resolve_signed_commits_mode(
			SignedCommitsMode::Off,
			false,
			false
		));
	}

	#[test]
	fn force_requires_only_token() {
		assert!(resolve_signed_commits_mode(
			SignedCommitsMode::Force,
			true,
			false
		));
		assert!(!resolve_signed_commits_mode(
			SignedCommitsMode::Force,
			false,
			true
		));
	}

	#[test]
	fn auto_requires_gha_and_token() {
		assert!(resolve_signed_commits_mode(
			SignedCommitsMode::Auto,
			true,
			true
		));
		assert!(!resolve_signed_commits_mode(
			SignedCommitsMode::Auto,
			true,
			false
		));
		assert!(!resolve_signed_commits_mode(
			SignedCommitsMode::Auto,
			false,
			true
		));
		assert!(!resolve_signed_commits_mode(
			SignedCommitsMode::Auto,
			false,
			false
		));
	}
}
