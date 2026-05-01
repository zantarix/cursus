# ADR-046: Stream Output of User-Configurable Shell Commands

## Status

Accepted (2026-04-19)

## Context

Two user-configurable command fields currently route through the `CommandRunner` trait's `run_shell_mut` method: the GitHub release `build_command` (introduced by [ADR-005](005-github-releases.md)) and the npm `lock_command` (introduced by [ADR-009](009-javascript-package-manager-strategy.md)). Both execute via the platform shell per [ADR-011](011-command-execution-strategy.md) and [ADR-033](033-windows-shell-execution.md), and both respect the late-guard dry-run pattern from [ADR-017](017-late-guard-dry-run-pattern.md) by way of the `DryRunCommandRunner` decorator.

`run_shell_mut` captures `stdout` and `stderr` into a `std::process::Output` struct and returns it to the caller. The present callers do not use the captured bytes; they check only `output.status.success()`. The bytes are surfaced only on failure, as a buffered error dump after the command has already exited.

This behavior produces a poor user experience for long-running commands:

- A `build_command` that compiles release binaries can run for many minutes with no visible output. Users cannot tell whether the command is making progress or hung.
- Progress indicators, compilation warnings, and informational output from tools like `cargo build`, `npm install`, and build scripts are entirely hidden during a successful run.
- The pre-log emitted at the `build_command` call site (`info!("Running build command: ...")`) is the only indication the command started; there is no further feedback until it returns.
- On failure, the full buffered output dumps to the terminal at once, which is awkward to read for commands that produced hundreds of lines of progress output.

The `CommandRunner` trait also carries a `run_shell` (read-only) method that has zero production call sites. It exists symmetrically alongside `run_shell_mut` but is unused. Keeping an unused method on the trait costs surface area in every implementation (`RealCommandRunner`, `VerboseCommandRunner`, `DryRunCommandRunner`, `RecordingCommandRunner`) without providing corresponding value.

A third shell method, `run_shell_interactive`, is used for editor invocation and genuinely requires inherited `stdin` so the editor can accept user input. It is distinct from the build-command case and must remain.

## Decision

We will add a new method `run_streaming` to the `CommandRunner` trait, migrate both `build_command` and `lock_command` to it, and remove the two unused shell methods (`run_shell` and `run_shell_mut`) from the trait. `run_shell_interactive` stays unchanged.

### `run_streaming` semantics

The method will be shell-interpreted, using the same platform shell helpers (`shell_program()` and `shell_flag()`) that `run_shell_mut` uses today, so it inherits the cross-platform behavior established by [ADR-011](011-command-execution-strategy.md) and [ADR-033](033-windows-shell-execution.md).

The method will be mutating and therefore suppressed by `DryRunCommandRunner` following the late-guard pattern from [ADR-017](017-late-guard-dry-run-pattern.md).

The child process will inherit the parent's `stdout` and `stderr`, so all output appears live on the user's terminal as the command produces it. The child's `stdin` will be set to null (redirected to the platform equivalent of `/dev/null`) so that a misconfigured command cannot accidentally block waiting for user input that will never arrive.

The return type will be `anyhow::Result<ExitStatus>`, consistent with `run_interactive` and `run_shell_interactive`. There is no captured output to return, because nothing is captured.

`RealCommandRunner::run_streaming` will emit `log::info!("Running: {command}")` before spawning the child. This centralizes the pre-log so every call site gets a standardized message. Callers must not add their own pre-log; any existing ad-hoc pre-log at a migrating call site will be deleted as part of the migration.

### Decorator behavior

`VerboseCommandRunner::run_streaming` will emit a `debug!` log and delegate to the inner runner, matching the pattern the other methods on that decorator already follow.

`DryRunCommandRunner::run_streaming` will suppress execution, emit `info!("[dry-run] would run (streaming): {command:?} (cwd: {})", cwd.display())`, and return a synthetic success `ExitStatus`. This mirrors how `DryRunCommandRunner::run_interactive` and `DryRunCommandRunner::run_shell_interactive` already behave.

The decorator composition order at application startup (`DryRunCommandRunner` → `VerboseCommandRunner` → `RealCommandRunner`) is unchanged.

### Trait surface reduction

`run_shell` (the read-only shell method) has zero production callers today and will be removed from the trait as part of this change.

`run_shell_mut` will have zero production callers after the `build_command` and `lock_command` migrations complete. It will be removed from the trait in the same change. If a future use case needs non-streaming shell mutation with captured output, the method can be reintroduced at that time; the trait should not carry speculative methods.

`run_shell_interactive` is retained unchanged. The editor invocation it serves requires inherited `stdin` for user interaction, which is the opposite of `run_streaming`'s null-`stdin` policy.

### Call-site migration

The two production call sites move from `run_shell_mut` to `run_streaming`. At the `github_releases.rs` call site, the existing `info!("Running build command: ...")` pre-log is deleted because `run_streaming`'s centralized log replaces it. At the npm `lock_command` call site, no pre-log exists to remove.

## Consequences

### Positive

- Users see live output from `build_command` and `lock_command` as those commands run. Long-running builds no longer appear to hang.
- Progress indicators, warnings, and informational output from underlying tools flow through to the terminal uninterrupted, matching what users expect from any CLI wrapping an external build tool.
- The trait surface shrinks: two unused or soon-to-be-unused methods (`run_shell` and `run_shell_mut`) are removed, simplifying every `CommandRunner` implementation.
- A single centralized `Running: {command}` pre-log in `RealCommandRunner::run_streaming` ensures consistent formatting across all current and future streaming call sites, eliminating per-call-site drift.
- Null `stdin` prevents a class of latent hangs where a misconfigured build command accidentally issues a prompt (e.g., `sudo`, credential entry) that would otherwise block indefinitely.

### Negative

- Callers lose programmatic access to command output. This is not a regression for the current call sites (they ignored the output), but it means `run_streaming` is not a general replacement for `run_shell_mut`. Any future need for captured output from a shell-interpreted mutation would require either reintroducing a capturing method or adopting the tee-pipe alternative.
- Because the child inherits the parent's `stdout`/`stderr` directly, cursus cannot post-process, filter, or reformat the command's output. If cursus later needs to, for example, prefix each line of build output or suppress specific lines, that capability would have to be rebuilt on top of piped streams.
- Removing `run_shell` and `run_shell_mut` is a breaking change to the internal trait. External consumers of the library that implemented `CommandRunner` (if any) would need to adjust. The library is pre-1.0 and the trait is a library-internal abstraction, so this is acceptable.

### Neutral

- The late-guard dry-run behavior is preserved identically: `DryRunCommandRunner` intercepts the new method exactly as it intercepts the other mutating methods.
- The cross-platform shell selection logic established by [ADR-033](033-windows-shell-execution.md) is reused unchanged; `run_streaming` does not introduce new platform-specific code paths.
- The choice to standardize the pre-log inside `RealCommandRunner` rather than at each call site commits cursus to that wording. Changing the pre-log format in the future is a one-line change, but it changes the output of every streaming call uniformly.
- `run_shell_interactive` remains the sole shell-interpreted method on the trait that is not `run_streaming`, reflecting its distinct requirement for inherited `stdin`.

## Alternatives Considered

### Tee pipe: stream to terminal and capture to buffer

Spawn the command with `Stdio::piped()` and fan each chunk out to both the parent terminal and an in-memory buffer, so callers retain programmatic access to the output while the user also sees it live.

Rejected for the current scope. It adds meaningful implementation complexity (async stream forwarding, buffered capture, line-buffering concerns), and no present or near-term caller needs the captured buffer. The current callers ignore the captured bytes entirely. If a future caller needs both streaming and capture, this design can be revisited without disturbing `run_streaming`'s current contract.

### Keep using `run_shell_mut`

Leave the two call sites on `run_shell_mut` and accept that users see no output during long builds.

Rejected. The lack of live output is the motivating problem; preserving the status quo does not address it.

### Add `run_streaming` alongside but retain `run_shell_mut`

Introduce the new method for the two migrating call sites but keep `run_shell_mut` on the trait in case a future use case needs non-streaming shell mutation with captured output.

Partially rejected. After migration, `run_shell_mut` has zero production callers. Keeping speculative methods on a trait inflates the implementation burden for every conformer (production runners, decorators, test-support runners) without a concrete use case to validate the signature. If the need arises later, the method can be reintroduced then, with a call site driving the exact signature required. The same reasoning applies to `run_shell`, which is removed in the same change.

### Inherit `stdin` as well

Let the child process inherit the parent's `stdin` so `run_streaming` could serve both the build-command case and the editor case.

Rejected. The editor case already has `run_shell_interactive`, which is designed for it and will remain unchanged. Inheriting `stdin` in `run_streaming` would open the door to a build command accidentally blocking on a prompt from an underlying tool (for instance, a build script that runs `sudo` or requests a credential). Nulling `stdin` turns that failure mode into a fast, visible error (whatever the tool does when it cannot read from `stdin`) instead of an opaque hang.

## Errata

- **2026-05-01**: [ADR-052](052-credential-redaction-in-error-messages.md) places `run_streaming` explicitly out of scope for credential redaction. Because the child process inherits the parent's stdout and stderr file descriptors, cursus never sees the streamed bytes and has no point at which it could redact them before they reach the terminal or CI log. Operators configuring `github.build_command` or `npm.lock_command` must continue to rely on CI-side secret masking (e.g. `::add-mask::`) for any credentials those commands may print.
