# ADR-030: Separate Binary and Library Crates with Environment Injection

## Status

Accepted

## Context

Cursus is structured as both a standalone CLI binary and a reusable Rust library ([ADR-000](000-founding-constraints.md) established this dual-artifact goal). A key architectural question is where the boundary between the binary and library sits, and how the library interacts with the outside world (environment variables, the filesystem, subprocesses, network APIs).

If the library reads environment variables, inspects the current working directory, or constructs its own HTTP clients, it becomes tightly coupled to the ambient process environment. This makes testing difficult: tests inherit whatever state the test runner's process happens to have, leading to flaky results, accidental network calls, and an inability to run tests in parallel against isolated scenarios. It also makes the library harder to embed in other contexts (e.g. a future GitHub Action or a programmatic integration) because the caller cannot control what the library sees.

The project needed a design that keeps the library fully deterministic and isolated while still allowing the binary to operate naturally against the real environment.

## Decision

We will maintain a strict separation between the binary crate (`src/main.rs`) and the library crate (`src/lib.rs` and all modules under `src/`). The core invariant is that **the library never reaches out to the ambient environment on its own** -- all external interactions are injected into it by the caller.

### The binary crate is a thin environment-reading shell

The binary crate (`src/main.rs`) is responsible for:

- Parsing CLI arguments via `clap`
- Reading environment variables (`VISUAL`, `EDITOR`, `GH_TOKEN`, `GITHUB_TOKEN`)
- Determining the current working directory via `std::env::current_dir()`
- Constructing concrete implementations of injected types (`RealCommandRunner`, `RestGitHubClient`)
- Initializing logging (the `CliLogger` lives in the binary, not the library)
- Calling `cursus::run_with()` with the fully-assembled environment

The binary contains no domain logic. It is annotated with `#[mutants::skip]` and `#[coverage(off)]` because it is intentionally untestable -- it is pure glue between the OS and the library.

### The library receives all external capabilities through `Env`

The `Env` struct (defined in `src/env.rs`) encapsulates the operating environment:

- A `CommandRunner` (trait object behind `Arc<dyn CommandRunner>`) for all subprocess execution, with read-only (`run`, `run_shell`) and mutating (`run_mut`, `run_shell_mut`, `run_interactive`) variants
- An optional editor name (resolved from `VISUAL`/`EDITOR` by the binary)
- An optional `GitHubClient` (trait object behind `Arc<dyn GitHubClient>`) for GitHub API access

`Env` is constructed by the binary and passed into `run_with()`. The library never calls `std::env::var()` or constructs its own HTTP clients.

### Derived types are assembled inside the library from injected primitives

`GitWorkdir` is not injected directly. It is constructed inside `run_with()` by combining the injected `Env` with a git repository root path that the library discovers by walking the filesystem upward from the caller-provided working directory. This is an internal assembly step, not an injection -- the library builds `GitWorkdir` from components it was given.

### Filesystem access is the controlled exception

The library does perform direct filesystem I/O (reading config files, writing changesets, updating changelogs and manifests). However, it never relies on the current working directory (`std::env::current_dir()`). Every filesystem operation receives an explicit directory path from the caller. The binary is responsible for resolving the real CWD at startup and passing it in; tests pass paths to temporary directories instead.

### The `CommandRunner` trait enables testing and dry-run

The `CommandRunner` trait abstracts all subprocess execution. The binary provides `RealCommandRunner` (wrapped in `VerboseCommandRunner` for logging). Tests provide `RecordingCommandRunner` which captures invocations without executing them. The `DryRunCommandRunner` decorator ([ADR-017](017-late-guard-dry-run-pattern.md)) intercepts mutating calls, and is applied automatically inside `run_with()` when `--dry-run` is active -- both the binary and tests benefit from the same dry-run mechanism.

## Consequences

### Positive

- The library is fully testable in isolation. Integration tests construct their own `Env` with a `RecordingCommandRunner` and point at temporary directories, with no ambient environment state leaking in.
- Tests can run in parallel without interference because each test owns its entire environment.
- The dry-run mechanism ([ADR-017](017-late-guard-dry-run-pattern.md)) works uniformly because all subprocess calls flow through the same `CommandRunner` trait.
- The library can be embedded in contexts other than a CLI binary (e.g. a programmatic Rust integration or a future GitHub Action) by constructing an appropriate `Env`.
- The binary is trivially small and changes rarely, reducing the surface area of untestable code.

### Negative

- Every new piece of environment state (e.g. a new token, a new env var flag) requires updating both the binary (to read the real value) and `Env` (to carry it), creating a small amount of ceremony.
- Filesystem access is not fully abstracted behind a trait, so tests that exercise config loading or changeset writing hit the real filesystem (in temporary directories). This is pragmatic but means those tests are not pure unit tests.
- The `Arc<dyn Trait>` pattern for `CommandRunner` and `GitHubClient` introduces dynamic dispatch overhead, though this is negligible for a CLI tool that spends most of its time waiting on subprocesses and I/O.

### Neutral

- `GitWorkdir` is an internal type (`pub(crate)`) assembled from injected primitives. Callers outside the crate interact with `Env` and the `run`/`run_with` entry points, not with `GitWorkdir` directly.
- The `CliLogger` implementation lives in the binary rather than the library. Logging is initialized before any library code runs, so the library uses the `log` facade without knowing or caring about the backend ([ADR-013](013-logging-infrastructure.md), [ADR-018](018-replace-fern-with-cli-logger.md)).
- This pattern aligns with the Rust convention of keeping `main.rs` thin and placing logic in `lib.rs`.

## Alternatives Considered

### Full trait abstraction for filesystem access

The filesystem could be abstracted behind a trait (e.g. a `Filesystem` trait with `read_file`, `write_file`, `create_dir` methods), making the library entirely pure with respect to I/O. This was not adopted because the added abstraction layer would significantly increase code complexity for modest testing benefit -- temporary directories already provide effective isolation, and filesystem operations in Cursus are straightforward reads and writes that rarely need to be mocked.

### Environment variables read directly in the library

The library could call `std::env::var()` wherever it needs environment state, avoiding the need for `Env` entirely. This was rejected because it makes tests dependent on the test runner's process environment, prevents parallel test execution on environment-sensitive code paths, and makes the library unusable in embedded contexts where the caller wants to provide synthetic environment values.

### Passing individual parameters instead of an `Env` struct

Instead of bundling capabilities into `Env`, each function could accept the specific dependencies it needs (a runner here, a GitHub client there). This was rejected because it would result in long parameter lists threaded through many call sites, and adding a new capability would require updating every function signature in the call chain. `Env` provides a single stable carrier that grows gracefully.
