# ADR-035: Introduce a Git Trait for Abstracting All Git Operations

## Status

Proposed

## Context

Cursus relies heavily on git operations throughout its subcommands: querying repository state, committing version bumps, tagging releases, pushing to remotes, and inspecting commit history. These operations are currently bound to the concrete `GitWorkdir` struct in `src/git/operations/mod.rs`, which shells out to the `git` binary via the `CommandRunner` trait.

The project has an established pattern for abstracting I/O boundaries behind traits stored on the `Env` struct: `CommandRunner` for subprocess execution ([ADR-011](011-command-execution-strategy.md)), `GitHubClient` for the GitHub HTTP API ([ADR-005](005-github-releases.md)), and the binary/library separation ([ADR-030](030-bin-lib-crate-separation.md)) ensures the library never reaches into the ambient environment. However, git operations remain the one major I/O surface that lacks a trait boundary.

This creates two problems. First, test isolation for git-dependent code requires either a real git repository on disk (slow, brittle) or a `RecordingCommandRunner` that fakes raw `git` output at the subprocess level (verbose, fragile). Second, future support for non-local backends -- such as remote code forges where git operations happen via HTTP API rather than a local `git` binary -- is architecturally blocked because every call site assumes `GitWorkdir`.

The current `run_with()` entry point takes `(cli, cwd, env)`, discovers the git workdir internally, constructs a `GitWorkdir`, and passes it as a `&GitWorkdir` parameter through every subcommand function. This threading pattern differs from how `CommandRunner` and `GitHubClient` are accessed (via `Env`), creating an inconsistency in the dependency injection model.

## Decision

We will introduce a `Git` trait that abstracts all git operations behind a dynamic dispatch boundary, and store it on `Env` as `Option<Arc<dyn Git>>`.

The trait will contain 20 methods spanning three categories:

**Identity:**

- `path() -> &AbsolutePath` -- the repository root

**Read-only queries:**

- `status_porcelain`, `current_branch`, `tag_exists`, `remote_origin_url`, `rev_list_count`, `log_message`, `log_subject`, `log_added_commit`, `diff_tree_names`, `diff_names`

**Mutations:**

- `add`, `commit`, `tag`, `push`, `checkout`, `checkout_or_reset_branch`, `force_push_branch`, `delete_tag`, `push_tag`

`GitWorkdir` will become the concrete implementation of this trait, preserving its current behaviour of delegating to `CommandRunner` via `env.run()` and `env.run_mut()`. A `RecordingGit` test double will be provided for in-memory test scenarios.

Git is optional on `Env`, set via a `.with_git()` builder method and accessed via `env.git() -> Option<&dyn Git>`. Some commands (such as `init`) need only the git workdir path for filesystem operations, not the full git operation surface. Making it optional avoids forcing callers to construct a `Git` implementation when one is not needed.

Subcommand functions will drop their `git: &GitWorkdir` parameter and instead access git via `env.git()` or through the `Config` which carries the `Env`. This aligns git access with how `CommandRunner`, `GitHubClient`, and other injected dependencies are already consumed.

The `run_with()` library entry point will change its signature from `(cli, cwd, env)` to `(cli, env)`. Git discovery (finding the `.git` directory by walking up from the working directory) will move to the caller or to the `run()` convenience function, which continues to accept `cwd`. The `find_git_workdir()` function will become a public utility.

Dry-run handling for git mutations requires no changes. `GitWorkdir` uses `env.run_mut()` for all mutating operations, which is already intercepted by the `DryRunCommandRunner` decorator per [ADR-017](017-late-guard-dry-run-pattern.md). The construction order in `run_with()` ensures `GitWorkdir` is created after the dry-run wrapper is applied to `Env`, so all mutations are automatically suppressed in dry-run mode without any trait-level dry-run awareness.

`GitWorkdir` will continue to hold `Env` internally for access to `CommandRunner`. The constructor remains `GitWorkdir::new(&env, path)`.

## Consequences

### Positive

- Tests can use `RecordingGit` to assert on git operations at a semantic level (e.g. "commit was called with this message") rather than parsing raw subprocess invocations from `RecordingCommandRunner`
- The dependency injection model becomes consistent: all major I/O boundaries (`CommandRunner`, `GitHubClient`, `Git`) are traits on `Env`
- Non-local git backends (forge APIs, in-memory implementations) become possible without touching any consuming code
- Removing the `git: &GitWorkdir` threading parameter from every subcommand function simplifies their signatures
- Moving `cwd` out of `run_with()` makes the library entry point independent of filesystem discovery, improving testability of the dispatch logic

### Negative

- The `Git` trait has 20 methods, which is a large surface area; adding new git operations requires updating the trait, `GitWorkdir`, and `RecordingGit`
- `Option<Arc<dyn Git>>` means call sites that require git must handle the `None` case, adding a small amount of boilerplate compared to the current guaranteed `&GitWorkdir` parameter
- Dynamic dispatch via `dyn Git` prevents inlining of git method calls, though the cost is negligible since every method performs subprocess I/O

### Neutral

- `GitWorkdir` continues to exist as a struct; the change is additive (new trait) rather than a rewrite
- The `find_git_workdir()` function moves from private to public but its logic is unchanged
- Existing integration tests that construct real git repositories remain valid; they now construct a `GitWorkdir` and inject it via `env.with_git()` instead of relying on internal discovery

## Alternatives Considered

### Pass `&dyn Git` as a separate parameter to `run_with()`

This would avoid putting git on `Env` by threading a `&dyn Git` argument through `run_with()` and into each subcommand. This was rejected because it creates an inconsistency: `CommandRunner`, `GitHubClient`, and other dependencies are already accessed via `Env`, and adding a parallel parameter-threading pattern would fragment the dependency injection approach. It would also not simplify subcommand signatures since they would still need an extra parameter.

### Have `GitWorkdir` take `Arc<dyn CommandRunner>` directly instead of `Env`

`GitWorkdir` currently holds a full `Env` to access `CommandRunner`. An alternative would be to narrow the dependency to just `Arc<dyn CommandRunner>`. This was rejected because `Env` may carry additional context needed by `GitWorkdir` in the future, and the `GitWorkdir::new(&env, path)` constructor is the established pattern. The `Env` clone is cheap (it contains only `Arc` pointers and small scalars).

### Make Git required (not optional) on `Env`

Instead of `Option<Arc<dyn Git>>`, always require a `Git` implementation on `Env`. This was rejected because the `init` command needs only the git workdir path for filesystem operations (writing `.cursus/config.toml`) and does not perform any git commands. Forcing callers to construct a full `Git` implementation for commands that do not use it would be wasteful and would couple `init` to the git abstraction unnecessarily.
