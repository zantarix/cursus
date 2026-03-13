# ADR-013: Adopt the log crate with fern for application logging

## Status

Accepted (backend sub-decision superseded by [ADR-018](018-replace-fern-with-cli-logger.md))

## Context

Cursus currently writes all user-facing output via raw `println!()` and `eprintln!()` calls scattered across the codebase. There are roughly three categories of output today:

1. **Operational progress and results** -- messages like "Published foo@1.2.3 to npm", version bump summaries, and dry-run previews. These are written with `println!()` to stdout.

2. **Warnings** -- non-fatal issues like dependency cycle warnings, changelog read failures, and partial GitHub Release asset upload failures. These are written with `eprintln!()` to stderr.

3. **Errors** -- fatal failures surfaced by `main()` via `eprintln!("Error: {e:#}")`.

This approach has several problems. There is no way to control verbosity -- users see everything or nothing. There is no consistent formatting. There is no mechanism for future diagnostic output to flow through the same channel. And because output is written directly to stdout/stderr, it is difficult to test what Cursus actually prints.

Rust's `log` crate is the de-facto logging facade. It provides macros (`error!`, `warn!`, `info!`, `debug!`, `trace!`) that are decoupled from any specific logger implementation. Code emits log records; a pluggable backend decides what to do with them. This separation is what Cursus needs: a stable API for emitting messages with a backend that can evolve independently.

## Decision

We will add the `log` crate as a dependency for the logging facade and `fern` as the logger backend. All Cursus output will flow through `log` macros.

**Facade vs. backend separation:**

The `log` crate (facade) will be a dependency of the library crate. The library -- including `run()` and all modules beneath it -- will only use `log` macros (`info!`, `warn!`, `error!`, `debug!`, `trace!`). It will never initialise a logger or depend on any concrete backend. This is a deliberate design decision: if another program embeds the cursus library, it is free to install its own `log` backend (or none at all) without conflicting with a logger that cursus initialised.

The concrete backend (`fern`) will be a dependency of the binary crate only and will be initialised in `main()` in `src/main.rs`, before calling `run()`. The level filter will default to `Info`, which preserves Cursus's current behaviour: operational messages, warnings, and errors are all visible.

[ADR-014](014-verbose-mode.md) will later replace this hardcoded default with dynamic control via the `--verbose` / `-v` and `--silent` / `-s` flags, allowing users to raise the level filter to `Debug` or `Trace`, or lower it to `Error`. Because initialisation already lives in `main()`, this extension only requires changing the level filter value passed to the fern dispatch.

**Stream routing:**

Log output will be split across stdout and stderr based on severity, preserving Cursus's current stream conventions:

| Level               | Stream |
|---------------------|--------|
| `Info`, `Debug`, `Trace` | stdout |
| `Warn`, `Error`          | stderr |

This is implemented using fern's `Dispatch` composition. A parent dispatch sets the global level filter and format, then chains two child dispatches: one that filters to `Info` and below and writes to stdout, and another that filters to `Warn` and above and writes to stderr. This split preserves the existing behaviour where operational output (`println!()`) goes to stdout and warnings/errors (`eprintln!()`) go to stderr.

**Log level conventions:**

| Level   | Purpose                                                        | Example                                                        |
|---------|----------------------------------------------------------------|----------------------------------------------------------------|
| `Error` | Fatal or semi-fatal failures the user must act on              | "Failed to publish foo@1.2.3: permission denied"               |
| `Warn`  | Non-fatal issues that deserve attention                        | "Circular dependencies detected between: a, b"                 |
| `Info`  | Normal operational output the user expects to see              | "Published foo@1.2.3 to npm", "Created tag v1.2.3"            |
| `Debug` | Diagnostic detail, not shown by default                        | "Running: git tag v1.2.3 in /path/to/repo"                    |
| `Trace` | Maximum detail, not shown by default                           | (unused initially)                                             |

**Migration scope:**

Existing `println!()` and `eprintln!()` calls will be migrated to `log` macros as part of implementing this ADR. The mapping is:

- `println!()` calls that report progress or results become `log::info!()`
- `eprintln!("Warning: ...")` calls become `log::warn!()`
- `eprintln!("Error: ...")` / `eprintln!("Failed to ...")` calls become `log::error!()`

This migration is a necessary part of establishing the logging infrastructure so that the level filter actually controls all output. However, the migration is mechanical and does not change Cursus's behaviour at the default `Info` level -- the same messages appear on the same streams, just routed through `log`.

**Format:**

Fern will be configured with a minimal format: no timestamps, no module paths, no level prefix for `Info` (since that is the "normal" output level). `Warn` and above will include a level prefix (e.g., `warn: circular dependencies detected`). `Debug` and `Trace` will include the level and module target for diagnostic clarity.

**Why fern:**

Fern is a lightweight, composable logging backend whose only mandatory dependency is `log` itself. Its `Dispatch` builder pattern allows per-level stream routing out of the box: child dispatches can filter by level and chain to different output targets (stdout, stderr, files). This makes the stdout/stderr split a first-class configuration concern rather than a workaround. Fern's format callback provides full control over output formatting without pulling in terminal colour or timestamp dependencies unless opted in.

## Consequences

### Positive

- All Cursus output flows through a single, level-filtered channel with consistent formatting
- The stdout/stderr split is preserved: operational output stays on stdout, warnings and errors stay on stderr
- Call sites use stable `log` macros that are decoupled from the backend, so the logger implementation can be swapped later without changing library code
- The library crate has no opinion on the logging backend, so consumers that embed cursus as a library can install their own `log` implementation
- The infrastructure is ready for [ADR-014](014-verbose-mode.md)'s `--verbose` and `--silent` flags to plug in -- only the level filter value in `main()` needs to change
- Fern's only mandatory dependency is `log`, keeping the transitive dependency footprint minimal

### Negative

- Adds `log` as a library dependency and `fern` as a binary-only dependency
- Migrating existing `println!()`/`eprintln!()` calls is a moderate amount of mechanical churn across `publish.rs`, `release.rs`, `init.rs`, `git/mod.rs`, `npm.rs`, and `main.rs`
- The split-stream dispatch configuration is slightly more verbose than a single-stream logger setup, though it is a one-time cost in `main()`

### Neutral

- The `log` facade is a compile-time zero-cost abstraction when log calls are filtered out, so there is no runtime overhead for suppressed levels

## Alternatives Considered

### env_logger

The most widely used `log` backend in the Rust ecosystem. It was rejected because its `Builder::target()` method sets the output stream globally for all log levels -- it cannot route `Info` to stdout and `Warn` to stderr. Achieving split-stream output would require bypassing `env_logger`'s write mechanism with a custom format function that writes directly to different streams, which is fragile and defeats the purpose of using a well-tested backend. Fern handles this natively via dispatch composition.

### Hand-rolled log::Log implementation

Writing a minimal struct that implements the `log::Log` trait with split-stream routing. This avoids adding any backend dependency. It was rejected because fern is small (one mandatory dependency: `log`), well-tested, and provides useful features (format callbacks, per-module level overrides, dispatch composition) that would need to be reimplemented. The marginal dependency cost does not justify the maintenance burden.

### tracing crate

Using `tracing` instead of `log` for structured, span-based instrumentation. This was rejected as over-engineered for Cursus's needs. Cursus is a short-lived CLI tool, not a long-running service. It does not need spans, async instrumentation, or structured event fields. The `tracing` crate and its ecosystem (`tracing-subscriber`, `tracing-fmt`) are significantly heavier dependencies. If Cursus eventually needs structured logging, `tracing` is compatible with `log` (via `tracing-log`), so this decision does not close that door.

### simplelog crate

Using `simplelog` as the backend. Its `TermLogger` supports splitting output between stdout and stderr by level. However, `TermLogger` requires the `termcolor` dependency for colour support even when colours are not used, and its level-to-stream mapping is less configurable than fern's dispatch composition. Fern provides more precise control with fewer mandatory dependencies.

### No migration of existing output

Adopting `log` only for new diagnostic output while leaving existing `println!()`/`eprintln!()` calls in place. This was rejected because it would create two parallel output systems -- some messages controlled by the level filter and others always printed regardless. This defeats the purpose of having a unified logging infrastructure and would make it impossible for a future verbose flag to act as a single knob for output control.

## Errata

- **2026-03-11:** The backend sub-decision (choosing `fern` as the `log::Log` implementation) is superseded by [ADR-018](018-replace-fern-with-cli-logger.md), which replaces fern with a hand-rolled `CliLogger`. The `log` facade decision and all other aspects of this ADR remain valid.
