# ADR-038: Replace ureq-Based RestGitHubClient with Shared Octocrab Implementation

## Status

Accepted

## Context

The cursus library crate contains a `GitHubClient` trait ([ADR-030](030-bin-lib-crate-separation.md)) with six methods for GitHub API operations: `create_release`, `upload_asset`, `create_pull_request`, `find_open_pull_request`, `update_pull_request`, and `publish_release`. The production implementation, `RestGitHubClient` in `src/github/rest.rs`, uses ureq (a synchronous HTTP client) to make REST API calls, constructing request/response types manually with serde.

A separate project, cursus-bot (a GitHub App webhook service), also implements the `GitHubClient` trait but uses octocrab, an async GitHub API client with typed bindings. This means the GitHub REST API interaction logic -- endpoint URLs, request bodies, response parsing, error handling -- is duplicated across the two projects, with each maintaining its own implementation of the same six operations.

With [ADR-037](037-async-library-with-tokio-runtime.md) converting the library to async with tokio, the synchronous ureq client no longer fits the async trait signatures. The library can now host an async `GitHubClient` implementation that both cursus-bin and cursus-bot can share, eliminating the duplication.

The `GitHubClient` trait is stored on `Env` as `Arc<dyn GitHubClient>` and is constructed by the consumer, not the library ([ADR-030](030-bin-lib-crate-separation.md)). This injection pattern naturally separates authentication strategy (which differs between consumers) from API operation logic (which is identical).

## Decision

We will replace `RestGitHubClient` with an `OctocrabGitHubClient` in the library crate and drop ureq as a dependency entirely.

### Client injection via pre-configured Octocrab instance

The `OctocrabGitHubClient` will accept an `octocrab::Octocrab` instance at construction time. The library will not construct the octocrab client, handle authentication, or read tokens from the environment. Consumers provide a fully configured `Octocrab`:

- cursus-bin constructs octocrab with a personal access token sourced from environment variables (e.g. `GITHUB_TOKEN`), as the current `RestGitHubClient` does today.
- cursus-bot constructs octocrab with GitHub App installation authentication (JWT + installation token exchange), as it does today with its own octocrab instance.

This preserves [ADR-030](030-bin-lib-crate-separation.md)'s principle that the library never reads the ambient environment.

### Shared implementation of GitHubClient trait

`OctocrabGitHubClient` will implement the `GitHubClient` trait using octocrab's typed API bindings for all six operations. This replaces the hand-rolled serde request/response types and manual URL construction in `rest.rs`.

### Drop ureq dependency

With octocrab (and its underlying reqwest/hyper stack) handling all HTTP needs, ureq will be removed from both the library and workspace `Cargo.toml`. The `build.rs` certificate bundling for ureq (if any) will also be removed.

### GitHubClient trait unchanged

The `GitHubClient` trait interface remains as-is. Method signatures do not change beyond the async conversion already covered by [ADR-037](037-async-library-with-tokio-runtime.md). `RecordingGitHubClient` (test double) is unaffected. Consumers that provide custom implementations continue to work.

### Asset upload streaming

The current `rest.rs` streams file uploads via `std::fs::File::open`, which is the known [ADR-036](036-filesystem-trait-abstraction.md) exception to filesystem trait abstraction. Octocrab's asset upload API accepts byte vectors or streams. The `upload_asset` implementation will read the file into memory or use tokio's async file I/O, replacing the direct `std::fs::File::open` call. This resolves the ADR-036 exception for this code path.

## Consequences

### Positive

- Single shared GitHub API implementation eliminates duplication between cursus and cursus-bot, reducing the maintenance surface for six API operations
- Octocrab's typed GitHub API bindings replace hand-rolled serde request/response structs and manual URL construction, reducing boilerplate and the risk of API contract errors
- Authentication strategy remains fully flexible through the injected `Octocrab` instance -- personal access tokens, GitHub App tokens, and any future auth mechanism octocrab supports all work without library changes
- Removes ureq as a dependency, consolidating HTTP handling onto a single stack (reqwest/hyper via octocrab)
- Resolves the [ADR-036](036-filesystem-trait-abstraction.md) `rest.rs` streaming upload exception by moving to octocrab's upload API

### Negative

- Octocrab is a heavier dependency than ureq, pulling in reqwest, hyper, tower, and rustls -- this increases compile time and binary size, which is relevant given the static binary distribution goal ([ADR-000](000-founding-constraints.md)) and seven-target build matrix ([ADR-022](022-distribution-strategy.md))
- The library gains a direct dependency on octocrab's API surface; breaking changes in octocrab require coordinated updates across cursus and cursus-bot
- This ADR depends on [ADR-037](037-async-library-with-tokio-runtime.md) being implemented first, since octocrab is async-only and requires a tokio runtime
- Octocrab's API may not cover all operations with first-class typed methods (e.g. asset uploads may require raw HTTP calls through octocrab's lower-level API), potentially reducing the typed-bindings benefit for some operations

### Neutral

- The `RecordingGitHubClient` test double is unaffected -- it does not use ureq or octocrab and will continue to work after async conversion
- Dry-run behaviour is unaffected -- `GitHubClient` calls are gated at the orchestration level per [ADR-017](017-late-guard-dry-run-pattern.md), not at the HTTP layer
- The `PullRequest` and `GitHubRepo` domain types remain library-owned; they are not replaced by octocrab types

## Alternatives Considered

### Keep ureq and wrap in spawn_blocking

Keep the synchronous ureq-based `RestGitHubClient` and wrap each call in `tokio::task::spawn_blocking` to satisfy the async trait signatures from [ADR-037](037-async-library-with-tokio-runtime.md). This avoids adding octocrab as a dependency but does not reduce duplication between cursus and cursus-bot -- cursus-bot would still maintain its own octocrab-based implementation. It also adds unnecessary sync-to-async bridging overhead and keeps ureq as an additional HTTP dependency alongside whatever async HTTP client other traits use. Rejected because it fails to address the primary motivation of eliminating duplicated GitHub API logic.

### Use reqwest directly instead of octocrab

Replace ureq with reqwest and write raw HTTP requests against the GitHub REST API. This would unify on an async HTTP client without the weight of octocrab's full GitHub API bindings. However, cursus-bot already uses octocrab and would not benefit from a reqwest-based implementation in the library -- the duplication would persist. Additionally, reqwest requires the same manual URL construction, serde types, and error handling that `rest.rs` already does, offering no reduction in boilerplate. Rejected because it does not achieve code sharing with cursus-bot and provides no improvement over the status quo beyond async compatibility.

### Keep separate implementations per consumer

Each project continues to maintain its own `GitHubClient` implementation -- cursus with ureq (or reqwest), cursus-bot with octocrab. The trait abstraction already isolates the two. This is the zero-effort option but means every GitHub API change (new endpoint, field change, error handling improvement) must be implemented twice. The six operations are functionally identical between the two implementations; only authentication differs, and that is already separated by the injected-instance pattern. Rejected because the ongoing maintenance cost of duplicated API logic outweighs the cost of adding octocrab to the library.

### Runtime-agnostic HTTP client

Use an HTTP client that does not depend on tokio (e.g. `surf` or a custom trait abstracting over HTTP). This was already considered and rejected in [ADR-037](037-async-library-with-tokio-runtime.md) -- the Rust async ecosystem has standardised on tokio, both consumers use it, and no mainstream GitHub API client is runtime-agnostic. Rejected for the same reasons as in ADR-037.

## Errata

`GitHubClient` was renamed to `CodeForgeClient` per [ADR-041](041-rename-github-client-trait-to-code-forge-client.md).
