# ADR-037: Make the Library Crate Async with Tokio as the Runtime

## Status

Accepted

## Context

Cursus is structured as a Cargo workspace with a library crate (`packages/cursus`) and a binary crate (`packages/cursus-bin`), per [ADR-030](030-bin-lib-crate-separation.md). The library exposes a synchronous `pub fn run(cli: Cli, env: Env) -> Result<ExitCode>` entry point and five core traits -- `CommandRunner`, `Git`, `Filesystem`, `PackageManagerAdapter`, and `GitHubClient` -- all of which are synchronous and stored on `Env` as `Arc<dyn Trait>` ([ADR-035](035-git-trait-abstraction.md), [ADR-036](036-filesystem-trait-abstraction.md)).

A separate project, cursus-bot (a GitHub App webhook service built on Axum/Tokio), consumes the cursus library crate. Because the library is synchronous and cursus-bot operates in an async context, there is significant friction at the boundary: cursus-bot uses `tokio::task::block_in_place` combined with `block_on` approximately ten times throughout its `GitHubBackend` implementation to call async APIs from within cursus's synchronous trait methods. Each of these bridges blocks a tokio worker thread, reducing the service's ability to handle concurrent webhooks and creating a pattern that is fragile and unidiomatic for an async Rust service.

The Rust async ecosystem has converged on tokio as the de facto standard runtime. All mainstream HTTP clients (reqwest, hyper) and higher-level API libraries (octocrab) depend on tokio. Both known consumers of the cursus library -- cursus-bin and cursus-bot -- already use or will use tokio. The synchronous library boundary forces every async consumer to build ad-hoc bridging code, and this cost will only grow as more operations (GitHub API calls, concurrent publishing, asset uploads) move to async implementations.

## Decision

We will make the cursus library crate async, with tokio as the accepted async runtime dependency.

### Entry point

The library's public entry point will change from `pub fn run(cli: Cli, env: Env) -> Result<ExitCode>` to `pub async fn run(cli: Cli, env: Env) -> Result<ExitCode>`. All internal orchestration code (`init`, `change`, `prepare`, `publish`, `ci`, `verify`) will become async functions.

### Core traits become async

All five core traits will have their methods converted to async:

- `CommandRunner` (6 methods: `run`, `run_mut`, `run_shell`, `run_shell_mut`, `run_interactive`, `run_shell_interactive`)
- `Git` (20 methods across identity, read-only queries, and mutations)
- `Filesystem` (9 methods: `read_to_string`, `read`, `write`, `create_dir_all`, `remove_file`, `exists`, `is_dir`, `canonicalize`, `glob`)
- `PackageManagerAdapter` (5 methods: `enumerate_projects`, `write_version`, `update_lock_file`, `publish`, `registry_name`)
- `GitHubClient` (all methods for release creation, PR management, and asset uploads)

Since all traits are used as `Arc<dyn Trait>` (object-safe, dynamically dispatched), async methods require boxing the returned futures. We will use the `async-trait` proc macro initially. This will be revisited when Rust stabilises native object-safe async fn in traits with dynamic dispatch support.

### Runtime ownership

The library crate will never construct or spawn a tokio runtime. The binary crate (`cursus-bin`) will annotate its `main` with `#[tokio::main]` and own the runtime. Other consumers (cursus-bot) will call into the library from within their own pre-existing tokio runtime. This preserves [ADR-030](030-bin-lib-crate-separation.md)'s principle that the library never reaches out to the ambient environment.

### TUI integration

ratatui and crossterm are synchronous libraries. TUI wizards (the `src/tui/` module) will be wrapped in `tokio::task::spawn_blocking` at the call site in the binary crate when invoking interactive flows. The TUI code itself remains synchronous; only the bridge to the async world changes. TUI handler functions and renderers remain pure and testable without a runtime.

### Tokio feature flags

The library will depend on tokio with a minimal feature set sufficient for `spawn_blocking` and the traits' async machinery. The binary crate will enable the full `rt-multi-thread` and `macros` features needed for `#[tokio::main]`. This keeps the library's tokio footprint as small as practical.

### Test migration

All library and integration tests that call async functions will use `#[tokio::test]`. Tests for pure synchronous logic (TUI handlers, parsers, model types) will remain as regular `#[test]`.

## Consequences

### Positive

- cursus-bot can implement all five core traits with native async code, eliminating approximately ten `block_in_place`/`block_on` bridge calls and unblocking tokio worker threads for concurrent webhook processing
- Opens the door to concurrent operations within cursus itself: parallel package publishing, concurrent GitHub asset uploads, and batched git queries
- Aligns with Rust ecosystem norms where tokio is the standard runtime, reducing friction for any future consumer of the library
- The `Env` injection pattern ([ADR-030](030-bin-lib-crate-separation.md)) and trait-based I/O boundaries ([ADR-035](035-git-trait-abstraction.md), [ADR-036](036-filesystem-trait-abstraction.md)) mean the async conversion is largely mechanical -- each trait method signature changes but the architecture remains unchanged

### Negative

- Significant migration effort: approximately 40 trait methods across five core traits, roughly 200 call sites that must gain `.await`, and approximately 100 tests that must migrate to `#[tokio::test]`
- tokio becomes a hard dependency of the library crate, increasing binary size by an estimated 1.5 MB for the CLI (relevant given [ADR-000](000-founding-constraints.md)'s static binary distribution goal and [ADR-022](022-distribution-strategy.md)'s seven-target matrix)
- `async-trait` adds a heap allocation (boxed future) per trait method call; negligible for this workload (CLI and webhook service, not a hot loop) but a permanent cost until native async traits support dynamic dispatch
- TUI integration becomes slightly more complex: `spawn_blocking` wrapping is required at each wizard invocation point in the binary
- All downstream code that calls into the library must be async-aware; there is no synchronous facade

### Neutral

- The `DryRunCommandRunner` decorator pattern ([ADR-017](017-late-guard-dry-run-pattern.md)) translates directly to async -- it will wrap the inner runner's async methods and return early with synthetic results for mutating calls
- `RecordingCommandRunner` and `RecordingGitHubClient` test doubles will become async but their recording logic is trivial to adapt
- The library's public API surface (`run`, `Env`, `Cli`) changes signature but not semantics
- Pure model types, parsers, and TUI handler logic remain synchronous and unaffected

## Alternatives Considered

### Keep the library synchronous

Leave the library as-is and accept that async consumers must bridge with `block_in_place`/`block_on`. This works today but blocks tokio worker threads in cursus-bot, is unidiomatic for async Rust services handling concurrent webhooks, and scales poorly as more operations become inherently async (HTTP API calls, concurrent publishing). Every new async consumer would need to build the same bridging infrastructure. Rejected because the friction cost is paid repeatedly and grows over time.

### Runtime-agnostic async (feature-flagged runtimes)

Define async traits without depending on any specific runtime, using only `core::future::Future`. Consumers would bring their own runtime via feature flags (e.g., `tokio`, `async-std`). Rejected because no mainstream Rust HTTP client is runtime-agnostic -- reqwest, hyper, and octocrab all assume tokio internally. This would prevent sharing HTTP client implementations between consumers and add combinatorial complexity to CI and testing for zero practical benefit, since both known consumers use tokio.

### Targeted parallelism only (std::thread or rayon)

Only parallelise specific CPU-bound or I/O-bound operations (e.g., publishing multiple packages) using OS threads or rayon, keeping the library synchronous. This addresses performance but does not solve the cursus-bot bridging problem at all -- the trait methods would remain synchronous, and cursus-bot would still need `block_in_place` for every call into async-backed trait implementations. Rejected because the primary motivation is consumer ergonomics, not parallelism.

### Make only GitHubClient async

Convert only the `GitHubClient` trait to async since it is the most obviously network-bound trait. This is the minimal change but leaves `Git`, `Filesystem`, `CommandRunner`, and `PackageManagerAdapter` synchronous. cursus-bot would still need `block_in_place` bridges for those traits, reducing the benefit from ten bridge sites to perhaps six or seven. Rejected because it creates an inconsistent API surface (some traits async, others not) and does not justify the migration effort if most of the bridging pain remains.
