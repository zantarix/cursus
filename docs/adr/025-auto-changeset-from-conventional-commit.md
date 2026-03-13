# ADR-025: Add `--auto` Flag to Derive Changesets from Conventional Commits

## Status

Accepted

## Context

Chronicle's `chronicle change` command records per-package semver bump levels as changeset files in `.chronicle/`. It currently operates interactively via TUI or non-interactively via explicit flags (`--change-type`, `--message`, `--project`) as described in [ADR-002](002-changeset-recording.md).

Automated dependency update bots such as Renovate and Dependabot open PRs with well-formed Conventional Commit messages. Each PR typically contains a single commit describing the dependency change. Today, a human must manually run `chronicle change` for every such PR to record a changeset -- this is friction that undermines the value of automated dependency management.

The Conventional Commits specification provides a structured commit message format (`<type>[optional scope]: <description>`) with well-defined semver mappings: `fix:` maps to PATCH, `feat:` maps to MINOR, and BREAKING CHANGE (via `!` suffix or `BREAKING CHANGE:` footer) maps to MAJOR. Other types (`chore:`, `docs:`, `ci:`, `build:`, `refactor:`, etc.) carry no semver significance.

Chronicle's config already has a `[git]` section ([ADR-006](006-git-lifecycle-hooks.md)) with an `enabled` flag that controls whether Chronicle performs git operations. The `prepare` command commits, tags, and pushes when `git.enabled = true`. The `--auto` flag should follow the same pattern: when git is enabled, the generated changeset file is committed and pushed immediately, completing the automation loop without human intervention.

A key design challenge is recursion prevention. When `--auto` commits and pushes the changeset, CI re-triggers on the branch. The next `--auto` run must detect that the branch now has more than one commit and become a no-op, preventing an infinite loop. The single-commit requirement serves double duty: it constrains the input to a well-defined case (one Conventional Commit to parse) and acts as the recursion guard.

## Decision

We will add an `--auto` flag to `chronicle change` that derives a changeset from the single Conventional Commit on the current branch.

### Flag definition

`--auto` is a boolean flag on the `change` subcommand. It is mutually exclusive with `--change-type`, `--message`, and interactive mode. When `--auto` is passed, `--no-interactive` is implied.

`--no-git` is also accepted on `chronicle change --auto`. When passed, it suppresses the git commit and push even if `git.enabled = true` in the config. This is the same escape hatch that `prepare` provides ([ADR-006](006-git-lifecycle-hooks.md)). The primary use case is self-hosted Renovate running `chronicle change --auto --no-git` as an after-update hook: Renovate manages its own git operations (committing, pushing, PR creation), so Chronicle should only write the changeset file and leave git alone. Without `--no-git`, Chronicle would commit and push independently, conflicting with Renovate's own git workflow.

### Commit counting

The command will compare the current branch against `origin/HEAD` to count how many commits the branch is ahead. This comparison uses standard git revision range mechanics (`origin/HEAD..HEAD`).

### Behaviour by commit count

**Zero commits ahead**: The branch has no commits relative to `origin/HEAD`. This is an unexpected state for a dependency PR. The command will exit with an error.

**Exactly one commit ahead**: This is the expected case. The command will parse the single commit message as a Conventional Commit and proceed to changeset generation (see below).

**More than one commit ahead**: The command will log an `info!` message explaining that `--auto` requires exactly one commit on the branch, and exit successfully with a zero exit code. No changeset is written. This is the recursion guard: after `--auto` commits and pushes the generated changeset, the branch has two commits, so subsequent CI-triggered `--auto` runs become no-ops.

### Conventional Commit parsing

The single commit message is parsed against the Conventional Commits format: `<type>[optional scope]: <description>`, with optional body and footers.

**Invalid format**: If the message does not match the Conventional Commit syntactic pattern (`<type>[optional scope]: <description>`), the command will exit with an error. This ensures that `--auto` is only used with structurally valid commit messages and produces a clear failure rather than silently generating no changeset.

**Any type is accepted**: The type field is not validated against a fixed list. Any syntactically valid Conventional Commit is accepted, including custom types like `deps:`, `perf:`, or `security:`. Only the three semver-significant signals trigger changeset creation (see below). All other types -- whether standard (`chore:`, `docs:`, `ci:`, `build:`, `refactor:`) or custom -- result in an `info!` log message indicating no changeset is needed and a successful exit.

**Semver mapping**:

- BREAKING CHANGE (indicated by `!` after type/scope, or `BREAKING CHANGE:` in a footer) creates a `major` changeset.
- `feat:` creates a `minor` changeset.
- `fix:` creates a `patch` changeset.

BREAKING CHANGE takes precedence: a `feat!:` commit produces a `major` changeset, not `minor`.

### Changeset content

The changeset message is the commit description (the text after `<type>[scope]:`). If the commit has a body, it is appended to the changeset message.

The changeset file is written using the standard format and naming conventions defined in [ADR-002](002-changeset-recording.md).

### Project scoping

The changeset is scoped to the projects whose files were modified by the commit. The command inspects the commit's changed file list (e.g., via `git diff-tree`) and matches each changed file against the known projects enumerated by the configured package managers. A file belongs to a project if it falls under that project's manifest directory or workspace root. Only projects with at least one changed file are included in the changeset.

If no changed files map to any known project, no changeset is necessary. The command will log an `info!` message and exit successfully with no changeset written. This covers commits that only touch files outside any project directory -- for example, workspace-level configuration files, root documentation, or CI pipeline definitions. These changes do not affect any package version and should not produce a version bump.

The optional scope from the Conventional Commit type is informational only and does not influence project scoping. Dependency update commits use scope for the dependency name (e.g., `fix(lodash): bump to 4.17.21`), not for the consuming Chronicle package.

### Git integration

If `git.enabled = true` in the Chronicle config and `--no-git` is not passed, the generated changeset file is immediately committed and pushed to the current branch after being written. The commit message follows a conventional format: `chore: add changeset for <original commit description>`.

This commit-and-push step is what triggers the recursion guard. After this operation, the branch has two commits relative to `origin/HEAD`, so any subsequent `--auto` invocation will detect the multi-commit state and exit as a no-op.

If `git.enabled = false` or `--no-git` is passed, the changeset file is written to disk but no git operations are performed. The calling CI workflow or tool (e.g., Renovate) is responsible for committing and pushing if desired.

### Dry-run support

`--dry-run` is respected per [ADR-008](008-dry-run-local-only-guarantee.md) and [ADR-017](017-late-guard-dry-run-pattern.md). When active, no changeset file is written and no git operations are performed. The command will log what it would have done: the derived change type, the changeset message, and whether it would have committed and pushed.

## Consequences

### Positive

- Enables fully automated changeset generation for dependency-update PRs from Renovate, Dependabot, and similar bots, eliminating human toil for routine dependency maintenance.
- The single-commit guard elegantly serves two purposes: constraining input to a well-defined case and preventing infinite recursion when git integration is enabled.
- No new configuration is required. The feature reuses the existing `[git].enabled` flag and `--no-git` escape hatch for its git operations, consistent with how `prepare` uses them.
- `--no-git` makes `--auto` composable with tools that manage their own git workflow (e.g., self-hosted Renovate after-update hooks), without requiring changes to the Chronicle config.
- Commit messages that are not syntactically valid Conventional Commits produce a clear error, catching misconfiguration early. Custom commit types beyond the standard set are accepted without error, making the feature compatible with projects that extend the Conventional Commits specification.
- Project scoping based on changed files produces more accurate changesets in monorepos, avoiding unnecessary version bumps for unaffected packages.
- The feature composes naturally with existing CI workflows: `chronicle change --auto` can be added as a step in dependency PR pipelines alongside existing changeset validation.

### Negative

- The Conventional Commits specification must be parsed, introducing either a new dependency or a non-trivial parser. Incorrect parsing of edge cases (multi-line footers, multiple BREAKING CHANGE markers, unusual scope characters) could produce wrong changeset types.
- The feature is tightly coupled to the Conventional Commits specification. Projects that do not use Conventional Commits cannot use `--auto` and will receive errors if they try.
- The `origin/HEAD` comparison assumes a specific git remote naming convention. Repositories using a different default remote name will need to ensure `origin/HEAD` is set correctly.
- Project scoping via changed file detection adds complexity: the implementation must map file paths to projects, which requires understanding workspace layouts.
- A semver-significant commit (e.g., `fix:`) that only touches files outside any known project will silently produce no changeset. This is correct behaviour -- such files do not belong to any package -- but may surprise users who expect every `fix:` commit to produce a changeset regardless of what files it touches.

### Neutral

- `--auto` is ignored in interactive mode since it implies `--no-interactive`. Users who pass both `--auto` and `--interactive` will receive an error about the mutual exclusivity.
- The feature does not introduce any new config fields. If future needs arise (e.g., configuring the base ref, mapping scopes to packages), those would be separate ADRs.
- The commit message format for the auto-generated commit (`chore: add changeset for ...`) is itself a Conventional Commit, which means `--auto` running against its own commit would classify it as "no semver significance" and exit cleanly -- a secondary recursion safety net beyond the commit count guard.
- When `--no-git` is used, the recursion guard still functions via the commit count check, but the caller is responsible for ensuring the changeset commit is pushed before the next `--auto` invocation.

## Alternatives Considered

### Parse all commits on the branch, not just one

Instead of requiring exactly one commit, parse all commits and aggregate their semver implications (taking the highest bump level). This was rejected because it complicates recursion prevention: the auto-generated changeset commit would itself be parsed in subsequent runs, requiring heuristic filtering (e.g., ignoring commits matching `chore: add changeset for`). The single-commit constraint is simpler, more predictable, and sufficient for the primary use case of single-commit dependency PRs.

### Map Conventional Commit scope to Chronicle package names

Use the scope field (e.g., `fix(my-package): ...`) to determine which packages the changeset applies to. This was rejected because the primary use case -- dependency update bots -- uses scope to identify the dependency being updated (e.g., `fix(lodash):`) not the consuming Chronicle package. Scope-to-package mapping would require configuration and would be wrong by default for the motivating use case. Changed-file detection achieves the same goal of scoping to affected projects without relying on the semantically ambiguous scope field.

### Always apply changeset to all packages

Apply the changeset to every enumerated project regardless of which files the commit modified, or fall back to all packages when no changed files match a known project. This was rejected because it produces unnecessarily broad changesets in monorepos. A dependency update that only touches one package's manifest should not trigger version bumps in unrelated packages. Commits that touch no project files at all (e.g., workspace-level config, root documentation) do not warrant a version bump for any package and should produce no changeset.

### Use a separate `chronicle auto-change` subcommand

Instead of a flag on `change`, create a dedicated subcommand. This was rejected because the operation is fundamentally a changeset recording action with an alternative input source. A flag on the existing `change` command is more discoverable and keeps all changeset-creation functionality under one subcommand.

### Silently skip non-conventional commit messages instead of erroring

When the commit message does not match the Conventional Commit format, exit successfully with no changeset instead of erroring. This was rejected because silent success on syntactically invalid input masks misconfiguration. If `--auto` is configured in a CI pipeline, the pipeline author expects Conventional Commit messages. A message that does not even match `<type>[scope]: <description>` indicates either a misconfigured bot or a pipeline applied to the wrong context, and an error makes this visible immediately. Note that this only applies to structurally invalid messages -- any valid Conventional Commit type (standard or custom) is accepted without error.

### Validate commit types against a fixed allowlist

Only accept commit types from a predefined set (e.g., `feat`, `fix`, `chore`, `docs`, `refactor`, `ci`, `build`, `perf`, `test`, `style`). This was rejected because the Conventional Commits specification explicitly allows custom types, and many projects extend the standard set with types like `deps:`, `security:`, or `release:`. A fixed allowlist would reject valid Conventional Commits and force users to conform to Chronicle's expectations rather than their project's conventions. Since only `fix`, `feat`, and BREAKING CHANGE have semver significance, all other types simply result in no changeset -- there is no need to enumerate or validate them.
