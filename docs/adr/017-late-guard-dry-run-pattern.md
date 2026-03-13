# ADR-017: Adopt Late Guard Pattern for Dry-Run Implementation

## Status

Accepted

## Context

Cursus supports `--dry-run` across `prepare`, `publish`, and `ci` commands, governed by [ADR-008](008-dry-run-local-only-guarantee.md)'s strict local-only invariant. Dry-run must preview what an operation would do without performing any mutations or remote operations.

There are two broad approaches to implementing dry-run in a multi-step command pipeline. The first -- "early branching" -- places `if dry_run { log } else { do_it }` checks at each point where a side effect occurs, creating parallel code paths that must independently compute the same values, track the same files, and produce equivalent log output. The second -- "late guarding" -- runs all computation, logging, and decision-making unconditionally, and only guards the final mutation itself.

The early branching approach has three structural weaknesses. It duplicates logic: both the dry-run and real branches often need to compute the same values (version bumps, file paths, log messages) but do so independently, leading to repeated code. It creates prediction methods: when a function performs side effects and returns results (e.g., a lock file updater that returns the path it modified), the dry-run branch needs a separate prediction method that returns the same path without performing the side effect, duplicating knowledge that must be kept in sync. It enables silent drift: because the two branches are structurally independent, a change to the real path can silently omit the corresponding dry-run update, and no compiler or type-system mechanism enforces alignment.

Cursus already has well-defined abstraction boundaries around its side effects. `CommandRunner` is the lowest-level abstraction: all subprocess execution (git commands, `cargo publish`, `npm install`, custom lock commands) flows through it, and it already supports a decorator pattern (`VerboseCommandRunner` wraps an inner runner to add debug logging). Above that, `GitWorkdir` encapsulates git operations, `GitHubClient` (a trait) encapsulates GitHub API calls, and `PackageManagerAdapter` (a trait) encapsulates package registry and filesystem operations.

Crucially, Cursus's mutations fall into three categories: subprocess-based mutations that go through `CommandRunner` (git commands, lock file updates, registry publishes), direct filesystem writes that do not (version bumps via `write_version`, dependency propagation via `update_dependency_version`, changelog generation, changeset consumption), and HTTP API calls via `GitHubClient` (release creation, asset uploads, pull request management). A dry-run strategy must address all three categories.

## Decision

We will adopt the "late guard" pattern as the standard approach for implementing dry-run across all Cursus commands. The core principle is: **run all logic unconditionally; guard only the mutation, and guard it at the lowest abstraction boundary that owns the side effect.**

### The pattern

All computation, path resolution, logging, and decision-making runs identically regardless of `dry_run`. The dry-run path sees the same computed values as the real path because it executes the same code. Only the final side effect -- the subprocess invocation, file write, or API call -- is guarded, and that guard lives inside the abstraction that owns the side effect.

Where log wording must differ between modes (e.g., "Would push" vs "Pushed"), use conditional phrasing within a single log call rather than duplicating the surrounding logic in two separate branches.

### Layered implementation

Dry-run awareness will be pushed into Cursus's existing abstractions at two levels:

**Level 1: `CommandRunner` decorator.** A `DryRunCommandRunner` decorator, following the same pattern as `VerboseCommandRunner`, will wrap any inner runner and skip execution of mutating subprocesses. This single decorator automatically covers all subprocess-based mutations: git commands (via `GitWorkdir`), lock file updates (via `update_lock_file`), registry publishes (via `publish`), and custom shell commands. `GitWorkdir` itself needs no changes -- it remains unaware of dry-run because the runner it delegates to handles suppression transparently. This also means `PackageManagerAdapter` methods that work purely through `CommandRunner` (`update_lock_file`, `publish`) gain dry-run support without signature changes.

The decorator must distinguish read-only commands from mutating ones. Git reads like `git status`, `git rev-parse`, `git tag -l`, and `git remote get-url` must still execute during dry-run to support branch detection, tag existence checks, and remote URL resolution. The classification strategy (allowlist of read-only patterns, or a flag on the call site) is an implementation detail left to the implementer.

**Level 2: `PackageManagerAdapter` methods with direct filesystem I/O.** Methods that write files directly without going through `CommandRunner` -- `write_version`, `update_dependency_version`, and by extension changelog generation and changeset consumption in the calling code -- cannot be handled by the `CommandRunner` decorator. These methods will accept a `dry_run` parameter and return the paths they would modify without performing the write. This eliminates the need for prediction-only methods like `lock_file_path()`, which duplicates lock file resolution logic that `update_lock_file()` already contains.

**Level 3: `GitHubClient` API calls.** `GitHubClient` is a trait with its own abstraction boundary, and its calls (release creation, asset uploads, pull request management) do not flow through `CommandRunner` -- they use direct HTTP via `ureq`. Per ADR-008, all remote API calls must be skipped entirely during dry-run. The calling code will continue to gate `GitHubClient` invocations behind a dry-run check at the orchestration level, since these operations are inherently unpredictable (the dry-run path cannot know whether a release already exists or whether an upload would succeed). The `GitHubClient` trait itself remains unchanged.

### Moving `--dry-run` to a global flag

Currently `--dry-run` is defined separately on `PrepareArgs`, `PublishArgs`, and `CiArgs`. We will move it to `GlobalArgs` so that it is available uniformly across all subcommands, present and future. This makes it straightforward to inject a `DryRunCommandRunner` at application startup, before any subcommand code runs -- the same point where `VerboseCommandRunner` is already composed.

Subcommands that do not currently support dry-run (`init`, `change`) will simply ignore the flag. Future subcommands will inherit it automatically.

### Legitimate exceptions

Two categories of early `if dry_run` guards remain as intentional divergences, not duplicated logic:

1. **Pre-flight checks for resources that will not be used.** Per [ADR-008](008-dry-run-local-only-guarantee.md), dry-run must not perform remote operations. Skipping the GitHub token requirement or dirty-tree validation during dry-run is a genuine behavioral difference: these checks guard operations that will not run.

2. **Operations whose outcome cannot be predicted locally.** Registry publish operations are inherently branched because dry-run cannot invoke the registry and cannot determine whether a publish would succeed, be skipped (version already exists), or fail. The dry-run path can only report what it would attempt.

## Consequences

### Positive

- Eliminates the class of bugs where dry-run output diverges from real execution because only one branch was updated.
- Makes dry-run output more accurate by definition: the real path and the dry-run path execute the same computation code, so they always agree on values like version numbers, file paths, and tag names.
- The `CommandRunner` decorator handles the majority of mutations (all subprocess-based ones) in a single place, without any changes to `GitWorkdir` or the subprocess-based parts of `PackageManagerAdapter`. This is the same decorator pattern already proven by `VerboseCommandRunner`.
- Consolidates prediction-only methods (like `lock_file_path()`) into their corresponding mutation methods, reducing trait surface area and eliminating duplicated resolution logic.
- A global `--dry-run` flag means future subcommands inherit dry-run support without per-command boilerplate, and the `DryRunCommandRunner` is composed once at startup.

### Negative

- The `DryRunCommandRunner` must classify commands as read-only or mutating. An incorrect classification could either suppress a needed read (breaking dry-run output) or allow a mutation through (violating [ADR-008](008-dry-run-local-only-guarantee.md)). This classification logic is a new responsibility that must be maintained as new commands are added.
- `PackageManagerAdapter` methods that do direct filesystem I/O (`write_version`, `update_dependency_version`) still gain a `dry_run: bool` parameter, since the `CommandRunner` decorator cannot intercept `std::fs::write` calls. This creates a mixed model: some mutations are guarded by the decorator, others by explicit parameters.
- Subcommands that do not support dry-run (`init`, `change`) will silently accept `--dry-run` as a global flag without effect. This is mildly surprising but harmless, and consistent with how `--verbose` and `--silent` already work.

### Neutral

- [ADR-008](008-dry-run-local-only-guarantee.md)'s strict local-only invariant is unchanged. The late guard pattern still ensures no remote operations occur during dry-run; it moves the guard from the caller to lower-level abstractions but does not weaken it.
- Test coverage for dry-run paths becomes more valuable: dry-run tests now exercise the same computation logic as real tests, with only the final mutation skipped.
- The `GitHubClient` trait itself is unchanged. Its calls are already skipped at the call site during dry-run because the entire GitHub orchestration block is gated on `!dry_run`. This is the "inherently branched" exception described above.

## Alternatives Considered

### Keep the scattered early-check pattern

Continue with `if dry_run { ... } else { ... }` at each mutation site in the caller. This was rejected because the pattern has already produced duplicated prediction logic and makes it structurally easy to introduce drift between branches. The problem compounds as the number of commands and adapters grows, and there is no mechanism to enforce that both branches stay aligned.

### Push dry-run into each higher-level abstraction individually

Make `GitWorkdir` accept `dry_run` at construction and skip command execution internally, and add `dry_run` parameters to all `PackageManagerAdapter` mutation methods. This was considered but is less efficient than the `CommandRunner` decorator approach. `GitWorkdir` would need dry-run logic in every mutating method, duplicating what the decorator handles in one place. The decorator approach is strictly simpler for subprocess-based mutations and only falls back to per-method parameters for the small number of direct filesystem writes.

### Use a command/event pattern to record and replay operations

Build a list of pending operations during execution and either execute or print them at the end. This was rejected because many operations depend on the results of previous ones (e.g., the new version must be computed before the changelog can reference it). A deferred execution model would require threading intermediate results through a command queue, adding significant complexity for a problem that the late guard pattern solves more directly.
