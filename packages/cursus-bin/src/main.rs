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

/// Attempts to construct the code forge client from environment and config.
///
/// Returns `Ok(client)` when a token is present and the repo identity can be
/// resolved, or `Err(reason)` describing why the client is unavailable.
async fn resolve_forge_client(
	env: &cursus::Env,
	config: &Option<cursus::model::config::Config>,
) -> Result<Arc<dyn cursus::github::client::CodeForgeClient>, String> {
	let token = env_first(&["GH_TOKEN", "GITHUB_TOKEN"])
		.ok_or_else(|| "No GitHub token found (GH_TOKEN / GITHUB_TOKEN)".to_string())?;
	let octocrab = octocrab::Octocrab::builder()
		.personal_token(token)
		.set_connect_timeout(Some(std::time::Duration::from_secs(10)))
		.set_read_timeout(Some(std::time::Duration::from_secs(30)))
		.set_write_timeout(Some(std::time::Duration::from_secs(30)))
		.build()
		.map_err(|e| format!("Failed to build GitHub client: {e}"))?;
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
	let git = Arc::new(cursus::git::GitWorkdir::new(
		Arc::clone(&runner),
		git_workdir,
	));

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

	let config = cursus::model::config::load(env.fs(), env.git().path()).await?;
	let forge_client = resolve_forge_client(&env, &config).await;
	let env = env.with_code_forge_client_result(forge_client);

	cursus::run(cli, env, config).await
}
