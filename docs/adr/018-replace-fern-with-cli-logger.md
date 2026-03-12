# ADR-018: Replace fern with hand-rolled log::Log implementation

## Status

Accepted

## Context

Renovate flagged the `fern` crate (pinned at 0.7.1) as unmaintained. Fern served as the logging backend initialised in `src/main.rs`, providing per-level formatting and stdout/stderr stream splitting as described in [ADR-013](013-logging-infrastructure.md). In practice, the fern configuration amounted to roughly 25 lines of trivial dispatch setup: a format callback with five match arms (one per log level), and two child dispatches to split output between stdout and stderr based on severity.

The `log` facade crate remains the correct choice for Chronicle's logging API. Only the backend -- the concrete `log::Log` implementation -- needs to change. The question is whether to adopt another third-party backend or hand-roll a minimal implementation.

## Decision

We will replace fern with a hand-rolled `CliLogger` struct that implements the `log::Log` trait directly in `src/main.rs`.

The implementation will be approximately 30 lines with zero new dependencies:

- The `log()` method will match on log level for formatting (the same five arms as the former fern configuration) and write to stdout for `Info`, `Debug`, and `Trace` levels, or to stderr for `Warn` and `Error` levels.
- The `flush()` method will flush both stdout and stderr.
- A static instance (`static LOGGER: CliLogger = CliLogger;`) will be registered via `log::set_logger()` in `main()`.
- The `fern` dependency will be removed from `Cargo.toml`.

The `log` facade decision from [ADR-013](013-logging-infrastructure.md) is unchanged. All call sites continue to use `log::info!()`, `log::warn!()`, etc. The level filter, stream routing, and formatting behaviour are preserved exactly.

## Consequences

### Positive

- Zero additional dependencies beyond the `log` facade that Chronicle already uses
- Eliminates the maintenance liability of depending on an unmaintained crate
- The implementation is trivially simple and fully under Chronicle's control
- Formatting and stream routing behaviour is preserved exactly, so no user-visible change occurs

### Negative

- Chronicle now owns the logging backend code, which means any future enhancements (e.g., colour support, per-module filtering) must be implemented manually rather than configured through a library

### Neutral

- The `log` facade API is unchanged -- no call-site modifications are needed anywhere in the codebase
- [ADR-014](014-verbose-mode.md)'s verbose/silent flag mechanism continues to work identically, as it only adjusts the level filter value passed to `log::set_max_level()`

## Alternatives Considered

### env_logger

The most widely used `log` backend. Rejected because its `Builder::target()` method sets the output stream globally for all log levels -- it cannot route `Info` to stdout and `Warn` to stderr. This was the same reason it was rejected in [ADR-013](013-logging-infrastructure.md), and nothing has changed.

### tracing

A structured, span-based instrumentation framework. Rejected as overkill for a short-lived CLI tool. Chronicle does not need spans, async instrumentation, or structured event fields. The `tracing` ecosystem (`tracing-subscriber`, `tracing-fmt`) would be significantly heavier dependencies than the 30 lines being replaced.

### structured-logger

A JSON-oriented logging backend. Rejected because it pulls in `serde`, `serde_json`, `parking_lot`, and `tokio` as dependencies, and routes log records by target rather than by level. Chronicle needs level-based stream splitting, not JSON output.

### flexi_logger

A flexible logging backend that supports multiple output channels. Rejected because achieving level-based stdout/stderr splitting still requires implementing a custom `LogWriter` trait -- the same amount of work as hand-rolling a `log::Log` implementation, but with an additional dependency.
