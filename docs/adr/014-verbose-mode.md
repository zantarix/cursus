# ADR-014: Add verbose and silent modes via global CLI flags

## Status

Accepted

## Context

Cursus is a release management CLI that orchestrates external commands (git operations via `CommandRunner`) and HTTP requests (GitHub Releases via `GitHubClient`). When something goes wrong, users currently see only the high-level `anyhow` error chain, which may not contain enough detail to diagnose the root cause. Two pain points stand out:

1. **Failed HTTP requests**: The `RestGitHubClient` wraps ureq errors with `anyhow::Context`, but the request URL, request body, response status, and response body are lost. A 422 from the GitHub API might mean a duplicate tag, a permissions issue, or a malformed payload, but the user only sees "Failed to create GitHub Release for tag 'v1.2.3'".

2. **Shell command execution**: The `CommandRunner` trait runs git commands, lock-file updates, and user-configurable `build_command`/`lock_command` strings. When a command fails, users may not know exactly what was invoked, in which directory, or what stderr contained. This is especially opaque for shell commands passed through `/bin/sh -c`.

Cursus already has a `GlobalArgs` struct that carries cross-cutting CLI concerns (`--interactive`/`--no-interactive`). Verbosity flags fit naturally alongside these. Beyond increasing verbosity for diagnostics, there is also a need to suppress output entirely -- for example, when running Cursus in CI pipelines or scripts where only the exit code matters and any non-error output is noise. The question is how these flags map to the logging infrastructure ([ADR-013](013-logging-infrastructure.md)) and what the initial scope of verbose output covers.

## Decision

We will add two global CLI flags to `GlobalArgs`:

- `--verbose` / `-v`: stackable flag that increases the log level by one step per occurrence.
- `--silent` / `-s`: boolean flag that restricts log output to errors only.

These flags are **mutually exclusive**. Passing both `--verbose` and `--silent` in the same invocation is a usage error and will be rejected by clap at parse time (via `conflicts_with`). As established in [ADR-013](013-logging-infrastructure.md), logger initialisation lives in `main()`. The flag values from `GlobalArgs` will be used in `main()` to set the logger's level filter before calling `run()`.

**Log level mapping:**

| Flags     | Level filter | What is shown                                                  |
|-----------|-------------|----------------------------------------------------------------|
| `-s`      | `Error`     | Errors only; all informational and warning output is suppressed |
| (default) | `Info`      | Normal operational output (progress, results)                  |
| `-v`      | `Debug`     | Diagnostic detail (commands run, HTTP failure summaries)        |
| `-vv`     | `Trace`     | Maximum detail (full HTTP request/response bodies)              |

**Initial verbose logging scope:**

1. **Failed HTTP requests**: When an HTTP request to the GitHub API fails and the error would propagate to the user, two levels of detail are logged:
   - At `Debug`: the request method, URL, response status code, and status message. This is enough to identify which call failed and why at a glance (e.g., "POST <https://api.github.com/.../releases> -> 422 Unprocessable Entity").
   - At `Trace`: the full request body and response body. These can be large and may contain sensitive data, so they are gated behind the highest verbosity level.

   Successful requests will not be logged at either level.

2. **CommandRunner invocations**: All commands executed via the `CommandRunner` trait will be logged at `Debug`. This includes the program name, arguments, and working directory. This applies to all three `CommandRunner` methods: `run`, `run_shell`, and `run_interactive`.

**Propagation strategy:**

Once the logger is initialised with the appropriate level filter, all verbose output flows through `log::debug!()` and `log::trace!()` macros. No explicit threading of verbose state to individual components is needed.

The `CommandRunner` trait itself will not be modified; instead, verbose logging for commands will be implemented via a decorator (wrapper) that implements `CommandRunner`, emits `log::debug!()` calls for invocations, and delegates to the inner runner. This preserves the existing `CommandRunner` implementations unchanged and makes verbose logging composable and testable.

For the `GitHubClient`, verbose logging will be added within the `RestGitHubClient` implementation. Failed requests will emit a `log::debug!()` summary (method, URL, status code, status message) and a `log::trace!()` record with the full request and response bodies. The abstract `GitHubClient` trait will not be modified.

## Consequences

### Positive

- Users can diagnose HTTP and command failures without resorting to external tools like `strace` or network proxies
- The decorator pattern for `CommandRunner` keeps verbose concerns separated from execution logic and is independently testable
- Both flags follow the established `GlobalArgs` pattern, so no new propagation mechanism is needed
- Stackable `-v` flags provide room to grow without a new ADR if finer-grained verbosity is needed later
- `--silent` enables clean CI/script usage where only the exit code and error output matter
- The `log` facade means verbose output sites are just `log::debug!()` and `log::trace!()` calls with no awareness of how verbosity is controlled

### Negative

- Verbose HTTP logging for failed requests must be careful not to leak sensitive headers (e.g., the `Authorization` bearer token) into output. Request and response bodies are gated behind `Trace` to reduce accidental exposure, but the risk is not eliminated
- `--silent` suppresses warnings, so non-fatal issues (e.g., dependency cycle warnings) will go unnoticed unless the user checks separately

### Neutral

- The initial scope is intentionally narrow (HTTP failures and command invocations only). Future work may expand verbose mode to cover changeset parsing, config loading, version resolution, or other internal operations, but that is out of scope for this decision
- The `Trace` level is used only for full HTTP request/response bodies in this initial scope. Additional `Trace`-level output may be added in the future as more subsystems adopt verbose logging

## Alternatives Considered

### Environment variable (e.g., CURSUS_VERBOSE or RUST_LOG)

Using an environment variable instead of a CLI flag. This was rejected because Cursus's existing UX pattern is flag-driven (`--interactive`, `--no-interactive`, `--dry-run`), and a flag is more discoverable via `--help`. An environment variable could be added later as a complement, but the flag should be the primary interface.

### Binary --verbose flag (no stacking)

A single boolean `--verbose` that maps to a fixed level (e.g., `Debug`). This was rejected because a stackable flag costs almost nothing to implement with clap's `ArgAction::Count` and provides a natural upgrade path. With only two states, any future need for finer granularity would require a breaking UX change or a separate flag.

### --quiet / -q instead of --silent / -s

Using `--quiet` / `-q` as the flag name, which is the more common convention in Unix tools (e.g., `make -q`, `grep -q`). Either name would work; `--silent` was chosen because it more clearly communicates the intent (only errors are shown, everything else is suppressed) and avoids ambiguity about how "quiet" the tool becomes. This is a naming preference, not a technical distinction. A `--quiet` alias could be added later if users expect it.

### Stackable --silent (e.g., -s suppresses Info, -ss suppresses Warn)

Making `--silent` stackable like `--verbose`, where each `-s` suppresses one additional level. This was rejected because the only useful quiet level below `Info` is `Error` -- suppressing `Warn` but not `Info` is not a meaningful distinction for Cursus's output. A single boolean flag is simpler and sufficient.

### Modify CommandRunner trait to accept a verbose parameter

Adding a `verbose: bool` parameter to every `CommandRunner` method. This was rejected because it would change the trait signature, requiring updates to all implementations (including test fakes), and it conflates execution with presentation. The decorator pattern achieves the same result without modifying the trait contract.
