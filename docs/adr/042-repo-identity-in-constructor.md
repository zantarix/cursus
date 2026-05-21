# ADR-042: Move Repo Identity into CodeForgeClient Constructor

## Status

Accepted

## Context

[ADR-041](041-rename-github-client-trait-to-code-forge-client.md) renamed `GitHubClient` to `CodeForgeClient`, making the trait name forge-agnostic. However, all six trait methods still accept `&GitHubRepo` as their first parameter. `GitHubRepo` is a GitHub-specific struct (owner + repo name, with GitHub URL parsing and GitHub-specific validation rules) defined in `github/remote.rs`. This creates a contradiction: the trait claims to be forge-agnostic, but every method signature is coupled to a GitHub identity type.

If a GitLab, Gitea, or Forgejo implementation were added, it would need a different repository identity type. GitLab identifies projects by numeric ID or a namespace path that can be nested arbitrarily deep (`group/subgroup/project`), which does not fit the `{ owner, repo }` model. Gitea uses `owner/repo` but has different validation rules. Forcing all forges to accept `&GitHubRepo` would require either shoehorning foreign identity into a GitHub-shaped struct or ignoring the parameter and resolving identity internally, both of which are incorrect.

The `CodeForgeClient` trait is used as `dyn CodeForgeClient` throughout the codebase. `Env` holds the forge client (currently `Option<Arc<dyn CodeForgeClient>>`), and callers in `cli/prepare/github.rs` and `cli/publish/github_releases.rs` receive `&dyn CodeForgeClient`. Any solution must preserve object safety.

Cursus currently only supports GitHub. Only one forge is active per repository at runtime (determined by the git remote or explicit config). There is no planned use case for interacting with multiple forges simultaneously. Because only one forge -- and therefore one repo identity -- is active per run, the repo identity is effectively a property of the client instance rather than a per-call argument.

Currently, `main.rs` constructs `OctocrabGitHubClient` from a token alone, and `GitHubRepo::resolve()` is called later inside command handlers (`cli/prepare/github.rs` and `cli/publish/github_releases.rs`). Moving repo identity into the constructor means the resolve must happen earlier. Construction can fail for many reasons: no token present, no git remote configured, unparseable remote URL, non-GitHub host, etc. All of these are simply reasons the forge client is unavailable; there is no meaningful distinction between them from the caller's perspective. Whether unavailability is fatal depends on the command being run, not on the specific reason for failure. A fresh repo with no remote (e.g. a user running `cursus init`) is a normal case and must not hard-fail at startup; instead the error is stored for later, and only commands that actually need the forge client will unwrap and propagate it.

## Decision

We will remove the `repo: &GitHubRepo` parameter from all six `CodeForgeClient` trait methods. Each implementation will store its repo identity at construction time. The trait becomes purely forge-agnostic: it carries no forge-specific types in its public API, and it remains object-safe.

The key elements of the approach:

- All six `CodeForgeClient` methods (`create_release`, `upload_asset`, `create_pull_request`, `find_open_pull_request`, `update_pull_request`, `publish_release`) will drop their `gh_repo: &GitHubRepo` parameter.
- `OctocrabGitHubClient` will be constructed with both an `octocrab::Octocrab` client and a `GitHubRepo`. Its methods will use the stored repo internally.
- `GitHubRepo` and its resolution logic (`GitHubRepo::resolve()`) become a construction concern for `OctocrabGitHubClient`, not part of the trait contract. `main.rs` will call `GitHubRepo::resolve()` during startup and pass the result into the client constructor.
- `Env` will hold `Result<Arc<dyn CodeForgeClient>>`. No wrapper types, no type erasure, no generics on `Env`. `Ok(client)` means the forge client is ready; `Err(e)` means the forge client is unavailable for any reason (no token, no remote, unparseable URL, non-GitHub host, etc.), with the specific reason preserved in the error.
- The `RecordingCodeForgeClient` test fake does not store a repo at all; it simply drops the repo parameter from its method signatures, matching the trait. The `CodeForgeInvocation` enum variants drop their `gh_repo` fields since the repo is no longer part of the trait contract.

`main.rs` will attempt to resolve the repo identity and construct the forge client eagerly at startup. `Env` will hold `Result<Arc<dyn CodeForgeClient>>` rather than `Option<Arc<dyn CodeForgeClient>>`. The two states are: `Ok(client)` means the forge client is ready, and `Err(e)` means the forge client is unavailable (no token, no remote, unparseable URL, non-GitHub host, etc.). The specific error reason is preserved so that commands which need the forge can surface a meaningful diagnostic. Commands that require the forge unwrap the `Result` and propagate the error with context; commands that do not need the forge (e.g. `change`, `init`, `verify`) ignore the field entirely.

## Consequences

### Positive

- Trait method signatures no longer reference any forge-specific type, completing the forge-agnostic abstraction started by [ADR-041](041-rename-github-client-trait-to-code-forge-client.md)
- `Arc<dyn CodeForgeClient>` remains valid with no wrappers, downcasts, or type-erasure machinery
- Callers in `cli/prepare/github.rs` and `cli/publish/github_releases.rs` become simpler: they no longer need to resolve the repo themselves or thread it through to each API call
- Each forge implementation owns its identity type internally, with appropriate validation and resolution logic; adding a new forge does not require modifying the `CodeForgeClient` trait
- Forge client construction uses a simple two-state model -- `Ok(client)` when ready, `Err(e)` when unavailable for any reason -- preserving error context so that commands which need the forge can surface a meaningful error message rather than silently proceeding without one

### Negative

- `OctocrabGitHubClient` becomes less flexible: it can no longer be constructed with a token alone and reused across different repos. This is not a real limitation for cursus (one repo per run), but it does narrow the API surface.

### Neutral

- `GitHubRepo` struct, its validation, URL parsing, and `resolve()` method remain unchanged in substance; only their call site moves from command handlers to `main.rs`
- The `github/` module directory retains its name per [ADR-041](041-rename-github-client-trait-to-code-forge-client.md)
- The number of `CodeForgeClient` methods is unchanged; only their signatures shrink
- `Env`'s `code_forge_client` field type changes from `Option<Arc<dyn CodeForgeClient>>` to `Result<Arc<dyn CodeForgeClient>>`

## Alternatives Considered

### Associated type with `AnyForgeClient` wrapper (original ADR-042 proposal)

Add `type Repo: Send + Sync + 'static` to `CodeForgeClient`, with each forge defining its own repo type. Introduce a concrete `AnyForgeClient` wrapper that type-erases both the client and repo, using `dyn Any` downcasts internally. `Env` would hold `Arc<AnyForgeClient>` instead of `Arc<dyn CodeForgeClient>`. Rejected because it solves the same problem (removing `GitHubRepo` from the trait) with considerably more machinery: an extra wrapper struct, runtime downcasts where none existed before, and loss of `dyn CodeForgeClient` compatibility. Stripping the repo from the trait entirely is simpler and avoids the object-safety problem altogether rather than working around it.

### Generic trait parameter (`CodeForgeClient<R: CodeRepo>`)

Make the trait generic: `CodeForgeClient<R: CodeRepo>` where `R` appears in all method signatures. This provides full compile-time type safety but makes the trait non-object-safe. Worse, it requires `Env` to either become generic over `R` (propagating the generic parameter through the entire application) or use `dyn CodeForgeClient<SomeConcreteR>` (which defeats the purpose since the concrete type must be named). Rejected because it creates an object-safety problem and forces generic parameter propagation with no benefit over the constructor approach.

### Opaque `Box<dyn Any>` repo parameter on trait methods

Replace `&GitHubRepo` with `&dyn Any` directly in the trait method signatures. This preserves object safety since the trait has no associated types, and `Env` can continue to hold `Arc<dyn CodeForgeClient>`. However, it scatters downcasts across every method in every implementation, loses all type information at call sites, and makes the trait's API opaque and error-prone. Rejected because it trades compile-time safety for a marginally simpler wrapper, and the poor developer experience at call sites is worse than moving the repo into the constructor.

### Separate resolution method with `dyn CodeRepo` trait object

Add a `CodeRepo` marker trait, have `CodeForgeClient` methods take `&dyn CodeRepo`, and add a `resolve_repo()` method returning `Box<dyn CodeRepo>`. Implementations must still downcast `&dyn CodeRepo` internally to access forge-specific fields, so the type safety improvement over raw `dyn Any` is minimal. The `CodeRepo` trait would need to expose a common interface, but forges have fundamentally different identity models, making a shared interface either too thin to be useful or too leaky. Rejected because it provides the appearance of type safety without the substance.

### Concrete enum (`CodeRepo { GitHub(GitHubRepo), GitLab(GitLabRepo), ... }`)

Define a `CodeRepo` enum with a variant per supported forge. This is fully object-safe and requires no wrapper. However, adding a new forge requires modifying the enum (a closed set), and every implementation must handle or ignore variants for other forges. This violates the open/closed principle and couples all forge implementations together. Rejected because it undermines the extensibility goal and creates dead branches in every implementation.

## Errata

### 2026-05-13: `GitHubRepo` location and resolution call site have moved

The Neutral bullet describing `GitHubRepo` resolution moving "from command handlers to `main.rs`" and the bullet stating that "the `github/` module directory retains its name per [ADR-041](041-rename-github-client-trait-to-code-forge-client.md)" are both incorrect after [ADR-056](056-gitlab-support-client-config-and-ci.md): the `github/` module is relocated to `forge::github`, `GitHubRepo` and `GitHubRepo::resolve()` move with it, and resolution is now performed in `cursus-bin/src/forge_resolution/github.rs` (extracted from `main.rs`). The same ADR adds a parallel `GitLabProject { host, group, project }` identity in `forge::gitlab` as a second concrete instance of the per-client identity model this ADR introduced; the construct-time identity-storage decision is unchanged.
