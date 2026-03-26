# ADR-036: Introduce Filesystem Trait for File I/O Abstraction

## Status

Accepted

## Context

Cursus already abstracts two of its three external I/O boundaries behind traits: `CommandRunner` for subprocess execution ([ADR-011](011-command-execution-strategy.md), [ADR-017](017-late-guard-dry-run-pattern.md)) and `GitHubClient` for HTTP API calls ([ADR-005](005-github-releases.md)). Both are stored on `Env` as `Arc<dyn Trait>` and injected at the binary boundary per [ADR-030](030-bin-lib-crate-separation.md). The third boundary -- filesystem I/O -- remains unabstracted: approximately 15 modules call `std::fs` functions and `glob::glob` directly.

[ADR-030](030-bin-lib-crate-separation.md) explicitly considered and rejected a `Filesystem` trait, reasoning that "temporary directories already provide effective isolation, and filesystem operations in Cursus are straightforward reads and writes that rarely need to be mocked." That assessment was correct at the time for the testing use case alone. However, the project's goals have since expanded beyond local-only operation. Supporting non-local backends -- such as remote code forges where manifests and changelogs are accessed via API rather than disk -- requires a seam between the library and its file I/O that `std::fs` calls scattered across modules cannot provide.

The existing `CommandRunner` and `GitHubClient` abstractions demonstrate that trait-based I/O boundaries integrate well with Cursus's architecture: they compose cleanly on `Env`, support decorator patterns (e.g. `DryRunCommandRunner`), and enable test doubles without filesystem side effects. A `Filesystem` trait would complete this picture, giving the library a uniform, injectable interface for all external I/O.

Several modules also contain free helper functions (e.g. `read_cargo_toml`, `read_package_json`) that read files on behalf of an adapter. These functions sit outside any struct and call `std::fs` directly, making them impossible to intercept without a trait boundary. Accepting `&dyn Filesystem` as a parameter gives these functions access to the abstraction without requiring them to be moved onto adapter structs.

## Decision

We will introduce a `Filesystem` trait that abstracts all file I/O operations performed by the library. The trait will be stored on `Env` as `Arc<dyn Filesystem>` and accessed via `env.fs()`, following the same pattern used by `CommandRunner` and `GitHubClient`.

### Trait surface

The trait will expose nine methods covering the file operations Cursus currently performs:

- **Reading:** `read_to_string` (returns `String`) and `read` (returns `Vec<u8>`)
- **Writing:** `write` (accepts `&[u8]`)
- **Directory creation:** `create_dir_all`
- **Deletion:** `remove_file`
- **Queries:** `exists`, `is_dir`
- **Path resolution:** `canonicalize` (returns the canonical absolute path)
- **Pattern matching:** `glob` (accepts a pattern string, returns matching paths)

All methods will accept `&AbsolutePath` arguments (or `&str` for `glob` patterns) and return `anyhow::Result` where fallible. The `canonicalize` method returns `PathBuf` since it resolves the canonical path, and `glob` returns `Vec<PathBuf>` since glob expansion produces paths that may not yet be validated as `AbsolutePath`.

### Concrete implementations

`LocalFilesystem` will be the sole implementation, delegating each method to its `std::fs` or `glob::glob` counterpart. Both the binary and tests inject `LocalFilesystem` into `Env`. Tests continue to use temporary directories for filesystem isolation, consistent with the existing test strategy established in [ADR-030](030-bin-lib-crate-separation.md).

### `read` returns `Vec<u8>`, not a streaming reader

The `read` method will return `Vec<u8>` rather than `Box<dyn Read>`. Returning a trait object would create object-safety complications (the `Filesystem` trait itself must be object-safe for `Arc<dyn Filesystem>`), and Cursus's files are small enough (manifests, changelogs, changesets, config) that loading them entirely into memory is appropriate.

### `glob` as a trait method

Glob matching is a trait method rather than a free function layered on top of `read_dir`. This allows non-local implementations to optimize differently -- a remote forge backend could use a server-side file listing API with pattern filtering rather than fetching a full directory listing and matching locally.

### `canonicalize` on the trait

`canonicalize` is included because it is security-critical: `AbsolutePath::subpath()` and `AbsolutePath::safe_glob()` use it to verify that resolved paths do not escape a base directory (preventing directory traversal attacks via symlinks or `..` components). A `Filesystem` implementation that cannot canonicalize paths must provide an equivalent guarantee through its own mechanism.

### No dry-run decorator on `Filesystem`

Per [ADR-017](017-late-guard-dry-run-pattern.md), dry-run for direct filesystem writes is handled by explicit `if !dry_run` guards at call sites, not by a decorator. The `Filesystem` trait will not have a `DryRunFilesystem` wrapper. This is consistent with how `write_version`, `update_dependency_version`, and changelog generation already work: the calling code decides whether to perform the write based on the `dry_run` parameter it receives.

### `Filesystem` is required on `Env`, not optional

Unlike `GitHubClient` (which is `Option<Arc<dyn GitHubClient>>` because not all commands need GitHub access), `Filesystem` will be a required field on `Env`. Every code path in Cursus needs filesystem access -- config loading, changeset reading, manifest parsing -- so making it optional would add `None`-handling noise everywhere with no benefit.

### `AbsolutePath` instead of `&Path`

All path-accepting trait methods will take `&AbsolutePath` rather than `&Path`. A codebase audit confirmed that every production filesystem call site derives its path from an `AbsolutePath`. Using `&AbsolutePath` in the trait signature enforces this invariant at the type level, preventing accidental use with relative paths. Since `AbsolutePath` implements `Deref<Target = Path>`, the `LocalFilesystem` implementation can still delegate to `std::fs` functions that accept `AsRef<Path>` without any conversion. The `glob` method keeps `&str` since glob patterns are not paths.

### Free helpers accept `&dyn Filesystem`

Free helper functions that read files (e.g. `read_cargo_toml`, `read_package_json`) will remain as free functions rather than being moved onto adapter structs. They will accept a `&dyn Filesystem` parameter, allowing callers to pass the trait object from `Env`. This preserves the existing module structure -- these helpers are used across multiple call sites and their free-function form is a better fit than tying them to a specific adapter -- while still routing all I/O through the `Filesystem` trait.

### Streaming file upload exception

`src/github/rest.rs` retains a direct `std::fs::File::open` call for streaming file uploads to GitHub Releases. The `Filesystem` trait's `read` method returns `Vec<u8>`, which would require loading entire release artifacts into memory before uploading. Since release binaries can be tens of megabytes, streaming from a file handle is the correct approach. This exception is scoped to a single call site and will be addressed in a future ADR if a streaming read method or a dedicated upload abstraction is needed.

## Consequences

### Positive

- Completes the I/O abstraction triad (`CommandRunner`, `GitHubClient`, `Filesystem`), giving the library a fully injectable interface for all external interactions.
- Enables non-local backends (e.g. remote code forges) by providing a seam that callers can implement against their own storage.
- The pattern is already proven in this codebase by `CommandRunner` and `GitHubClient`, so the team has established conventions to follow.
- Free file-reading helpers gain an explicit `&dyn Filesystem` dependency, making their I/O visible and injectable without restructuring modules.

### Negative

- Adds an abstraction layer over simple `std::fs` calls, increasing indirection. Every file operation now goes through a trait method dispatch rather than a direct standard library call.
- All call sites that currently use `std::fs` must be updated to use `env.fs()`, which is a pervasive refactor touching approximately 15 modules.
- `Env::new()` now requires a `Filesystem` argument in addition to `CommandRunner`, adding ceremony to every test that constructs an `Env`.

### Neutral

- The dry-run mechanism is unchanged. [ADR-017](017-late-guard-dry-run-pattern.md)'s call-site guards continue to handle filesystem write suppression; the `Filesystem` trait is orthogonal to dry-run.
- `AbsolutePath::subpath()` and `safe_glob()` will need to accept a `&dyn Filesystem` (or access it through `Env`) instead of calling `std::fs::canonicalize` directly. The security invariant they enforce is preserved; only the implementation path changes.
- `src/github/rest.rs` retains a direct `std::fs::File::open` call for streaming file uploads, since the `Filesystem` trait does not support streaming reads. This is a known, scoped exception to the "all I/O through the trait" principle, to be revisited if non-local backends need to upload artifacts.
- This decision supersedes the rejection rationale in [ADR-030](030-bin-lib-crate-separation.md)'s "Full trait abstraction for filesystem access" alternative. [ADR-030](030-bin-lib-crate-separation.md)'s core decision (environment injection via `Env`) remains valid and is extended, not replaced.

## Alternatives Considered

### Read/write split trait (mirroring CommandRunner's run/run_mut)

Split the trait into read-only and mutating method sets, similar to how `CommandRunner` distinguishes `run`/`run_shell` from `run_mut`/`run_shell_mut`. This would allow a dry-run decorator to intercept writes while passing through reads. Rejected because [ADR-017](017-late-guard-dry-run-pattern.md) already establishes that filesystem dry-run is handled at call sites via `if !dry_run` guards, not by a decorator. Adding a read/write split would create a parallel dry-run mechanism that contradicts the existing convention and would need to be kept in sync with the call-site guards.

### `read_dir` with pattern matching instead of `glob`

Expose a `read_dir` method that returns directory entries and let callers do their own pattern matching, rather than including `glob` on the trait. Rejected because it would require reimplementing glob matching logic at each call site (or in a shared utility), and it prevents non-local backends from optimizing pattern-matched file listing into a single operation. The `glob` crate's semantics are well-understood and providing them as a trait method is more ergonomic.

### Optional `Filesystem` on `Env`

Store the `Filesystem` as `Option<Arc<dyn Filesystem>>` on `Env`, defaulting to `None` and falling back to direct `std::fs` calls when absent. Rejected because every code path in Cursus needs filesystem access. An `Option` would add `.expect()` or match arms at every call site for a `None` case that should never occur in practice, degrading code clarity for no real flexibility benefit.

### `&Path` parameters instead of `&AbsolutePath`

Accept generic `&Path` in trait method signatures, allowing both absolute and relative paths. Rejected because a codebase audit confirmed that no production call site passes a relative path to any filesystem operation -- every path originates from an `AbsolutePath`. Accepting `&Path` would provide no practical flexibility while weakening type safety, allowing callers to accidentally pass relative paths that could resolve differently depending on the current working directory.
