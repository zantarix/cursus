# ADR-035: Introduce a Git Trait for Abstracting All Git Operations

## Status

Accepted

## Context

Cursus relies heavily on git operations throughout its subcommands: querying repository state, committing version bumps, tagging releases, pushing to remotes, and inspecting commit history. These operations are currently bound to the concrete `GitWorkdir` struct in `src/git/operations/mod.rs`, which shells out to the `git` binary via the `CommandRunner` trait.

The project has an established pattern for abstracting I/O boundaries behind traits stored on the `Env` struct: `CommandRunner` for subprocess execution ([ADR-011](011-command-execution-strategy.md)), `GitHubClient` for the GitHub HTTP API ([ADR-005](005-github-releases.md)), and the binary/library separation ([ADR-030](030-bin-lib-crate-separation.md)) ensures the library never reaches into the ambient environment. However, git operations remain the one major I/O surface that lacks a trait boundary.

This creates two problems. First, test isolation for git-dependent code requires either a real git repository on disk (slow, brittle) or a `RecordingCommandRunner` that fakes raw `git` output at the subprocess level (verbose, fragile). Second, future support for non-local backends -- such as remote code forges where git operations happen via HTTP API rather than a local `git` binary -- is architecturally blocked because every call site assumes `GitWorkdir`.

The current `run_with()` entry point takes `(cli, cwd, env)`, discovers the git workdir internally, constructs a `GitWorkdir`, and passes it as a `&GitWorkdir` parameter through every subcommand function. This threading pattern differs from how `CommandRunner` and `GitHubClient` are accessed (via `Env`), creating an inconsistency in the dependency injection model.

## Decision

We will introduce a `Git` trait that abstracts all git operations behind a dynamic dispatch boundary, and store it on `Env` as `Arc<dyn Git>`.

The trait will contain 20 methods spanning three categories:

**Identity:**

- `path() -> &AbsolutePath` -- the repository root

**Read-only queries:**

- `is_dirty`, `current_branch`, `tag_exists`, `remote_origin_url`, `rev_list_count`, `log_message`, `log_subject`, `log_added_commit`, `diff_tree_names`, `diff_names`

**Mutations:**

- `add`, `commit`, `tag`, `push`, `checkout`, `checkout_or_reset_branch`, `force_push_branch`, `delete_tag`, `push_tag`

`GitWorkdir` will become the concrete implementation of this trait, preserving its current behaviour of delegating to `CommandRunner` for subprocess execution. Tests use `RecordingCommandRunner` with `GitWorkdir` to assert on git operations at the subprocess level.

Git is required on `Env`, accessed via `env.git() -> &dyn Git`. `Env::new()` takes a `git` parameter alongside the other dependencies. Every subcommand that needs git accesses it through `Env`; the `init` command constructs its own `Env` with a `GitWorkdir` pointed at the discovered workdir.

Subcommand functions will drop their `git: &GitWorkdir` parameter and instead access git via `env.git()` or through the `Config` which carries the `Env`. This aligns git access with how `CommandRunner`, `GitHubClient`, and other injected dependencies are already consumed.

The `run_with()` library entry point will change its signature from `(cli, cwd, env)` to `(cli, env)`. Git discovery (finding the `.git` directory by walking up from the working directory) will move to the caller or to the `run()` convenience function, which continues to accept `cwd`. The `find_git_workdir()` function will become a public utility.

Dry-run handling for git mutations requires no changes. `GitWorkdir` uses its `CommandRunner` for all mutating operations, which is already intercepted by the `DryRunCommandRunner` decorator per [ADR-017](017-late-guard-dry-run-pattern.md). The construction order in `run_with()` ensures `GitWorkdir` is created after the dry-run wrapper is applied to `Env`, so all mutations are automatically suppressed in dry-run mode without any trait-level dry-run awareness.

`GitWorkdir` takes an `Arc<dyn CommandRunner>` and an `AbsolutePath` directly, rather than accepting a full `Env`. This narrows the dependency to exactly what `GitWorkdir` needs -- a command runner and a path -- keeping the struct decoupled from the broader `Env` type.

## Consequences

### Positive

- Tests can use `RecordingCommandRunner` with `GitWorkdir` to capture and assert on git subprocess invocations, maintaining full test isolation without needing real repositories
- The dependency injection model becomes consistent: all major I/O boundaries (`CommandRunner`, `GitHubClient`, `Git`) are traits on `Env`
- Non-local git backends (forge APIs, in-memory implementations) become possible without touching any consuming code
- Removing the `git: &GitWorkdir` threading parameter from every subcommand function simplifies their signatures
- Moving `cwd` out of `run_with()` makes the library entry point independent of filesystem discovery, improving testability of the dispatch logic

### Negative

- The `Git` trait has 20 methods, which is a large surface area; adding new git operations requires updating the trait and `GitWorkdir`
- Dynamic dispatch via `dyn Git` prevents inlining of git method calls, though the cost is negligible since every method performs subprocess I/O

### Neutral

- `GitWorkdir` continues to exist as a struct; the change is additive (new trait) rather than a rewrite
- The `find_git_workdir()` function moves from private to public but its logic is unchanged
- Existing integration tests that construct real git repositories remain valid; they now construct a `GitWorkdir` and pass it to `Env::new()` instead of relying on internal discovery

## Alternatives Considered

### Pass `&dyn Git` as a separate parameter to `run_with()`

This would avoid putting git on `Env` by threading a `&dyn Git` argument through `run_with()` and into each subcommand. This was rejected because it creates an inconsistency: `CommandRunner`, `GitHubClient`, and other dependencies are already accessed via `Env`, and adding a parallel parameter-threading pattern would fragment the dependency injection approach. It would also not simplify subcommand signatures since they would still need an extra parameter.

### Have `GitWorkdir` hold a full `Env` instead of `Arc<dyn CommandRunner>`

`GitWorkdir` could accept a full `Env` reference to access `CommandRunner` and any future dependencies. This was rejected because `GitWorkdir` only needs a command runner and a path; accepting `Env` would create a circular dependency concern (since `Env` holds `Arc<dyn Git>`, and `GitWorkdir` implements `Git`) and would over-couple `GitWorkdir` to the broader dependency injection container. Narrowing to `Arc<dyn CommandRunner>` keeps the struct focused and avoids the circularity.

### Make Git optional on `Env`

Store git as `Option<Arc<dyn Git>>` with a `.with_git()` builder method, so commands that do not need git (such as `init`) can skip constructing a `Git` implementation. This was rejected because in practice every code path that reaches `run_with()` has a discovered git workdir, and the `init` command constructs its own `Env` with a `GitWorkdir` pointed at the project root. Making git optional would add `None`-handling boilerplate at every call site for a case that does not arise in practice.

## Errata

### 2026-03-30: `GitHubClient` renamed to `CodeForgeClient`

References to the `GitHubClient` trait in this ADR are incorrect: [ADR-041](041-rename-github-client-trait-to-code-forge-client.md) renames the trait to `CodeForgeClient`. The `Git` trait abstraction itself is unchanged; only the related trait name differs.

### 2026-04-29: `Git` trait gains `head_sha()` and a `SignedCommitGit` decorator

The Decision section's enumeration of `Git` methods is incomplete and its single-impl framing is no longer accurate. [ADR-050](050-verified-release-commits-via-git-data-api.md) adds a `head_sha()` method (returning the full HEAD SHA, required so the `SignedCommitGit` decorator can determine the parent when building a commit object via the GitHub Git Data API); all `Git` implementations must provide it, with `GitWorkdir` delegating to the `git` binary. The same ADR introduces `SignedCommitGit` as a second `Git` impl that decorates an inner `Git` to route `commit()`, `push()`, and `force_push_branch()` through the GitHub API while delegating all other methods unchanged.

### 2026-05-22: `SignedCommitGit` renamed; second decorator added for GitLab

The 2026-04-29 erratum above names the decorator as `SignedCommitGit` and frames the trait as having one production impl plus one decorator. Both points are now incorrect: [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md) renames the GitHub decorator to `GitHubSignedCommit` and adds a sibling `GitLabSignedCommit` decorator that wraps any `Arc<dyn Git>` and routes `commit()`, `push()`, and `force_push_branch()` through GitLab's commits API. The trait itself is unchanged — both decorators implement the same `Git` surface — but the production picture is now one `GitWorkdir` plus two forge-specific decorators, selected at the binary-boundary based on the active forge.
