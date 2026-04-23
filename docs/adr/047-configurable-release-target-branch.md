# ADR-047: Configurable Release Target Branch

## Status

Proposed

## Context

[ADR-015](015-ci-managed-release-workflow.md) introduced the `branch` git strategy for `cursus prepare`, under which the command creates a release branch (named `{git.release_branch_prefix}{current_branch}`) and, when `[github].enabled = true`, opens a pull request from that release branch back into the branch that was checked out when `prepare` ran. The PR base is hard-wired to the original branch: the release PR always targets the same branch the developer or CI job was on.

This hard-wiring conflates two concerns that are independent in practice: the *source* of a release (the branch whose changesets and commits are being released) and the *target* of a release PR (the branch that accumulates released versions). Two common, legitimate workflows cannot be expressed under the current design:

1. **Trunk-based development with a dedicated release branch.** A repository with `main` as its trunk may want to advance a separate `release` branch only when a version actually ships. Today, running `cursus ci` on `main` produces a release branch `cursus-release/main` that targets `main` — so every merged release PR moves `main`, defeating the purpose of isolating released state on a separate branch.
2. **Git-flow-style workflows.** A repository where active development happens on `develop` and released state lives on `main` wants releases prepared from `develop` to PR into `main`. Today, the release PR would target `develop`, requiring operators to manually re-target the PR (or perform an additional merge) after creation.

Neither workflow can be served by reconfiguring the existing `release_branch_prefix` field, which controls the *name* of the release branch, not the PR base. Neither can be served by the existing `--branch` CLI override, which likewise names the release branch.

The decision to make the PR base configurable is specifically scoped to `prepare`. `cursus verify` ([ADR-032](032-verify-changeset-on-branch.md)) operates on feature branches *before* merge and uses `--base` to identify the diff ancestor for changeset detection; its base-ref semantics are unrelated to release-PR targeting and remain CLI-only. `cursus publish` inspects HEAD and tag/registry state and never reads branch names. `cursus ci` dispatch logic keys off changeset presence and tag absence ([ADR-015](015-ci-managed-release-workflow.md)) and is likewise branch-name-agnostic.

Finally, backwards compatibility is a hard constraint: existing repositories relying on the current "PR into self" behaviour must continue to work with no configuration changes on upgrade.

## Decision

Cursus shall add a per-source-branch mapping to the `[git]` configuration section that specifies the merge target (PR base) for releases produced from each source branch.

### Configuration schema

A new field `release_targets` shall be added to the `[git]` section of `.cursus/config.toml`:

```toml
[git.release_targets]
main = "release"
develop = "main"
```

- Type: `BTreeMap<String, String>` mapping source branch name to target branch name. `BTreeMap` is used for deterministic ordering, consistent with the existing `github.artifacts` field convention.
- Default value: an empty map. An absent `[git.release_targets]` table is semantically identical to an empty map; no configuration migration is required for existing repositories.

### Target resolution

Target resolution shall occur alongside release-branch computation (the existing `compute_release_branch()` path in the `prepare` git lifecycle). The resolved target branch shall be stored on the branch-state value that is already threaded through preflight checks and passed to the PR-creation step, replacing today's use of the original branch as the PR base.

Resolution precedence (highest to lowest):

1. The `--target <branch>` CLI flag on `cursus prepare` (new, added by this ADR), mirroring the existing `--branch` override.
2. A matching entry in `[git.release_targets]` for the current source branch.
3. Fallback: the current source branch itself (the current behaviour, preserved for unmapped branches).

An explicit identity mapping (e.g. `main = "main"`) is valid and is functionally a no-op relative to the fallback, but may be used to document intent in the configuration file.

### Release branch naming stays source-derived

The release branch name shall continue to be derived from the source branch as `{release_branch_prefix}{source_branch}`, regardless of the resolved target. This ensures that parallel release flows targeting the same branch (for example, `cursus-release/main` and `cursus-release/develop` both targeting `main` in a git-flow repository) remain distinct and retain provenance in their branch name.

### Detached HEAD

Detached HEAD shall continue to hard-error during `prepare` when git is enabled. Mapping lookup requires a branch name; there is no special-case behaviour for detached HEAD.

### Remote target validation

Cursus shall not pre-validate the existence of the configured target branch on the remote. If the target does not exist, the forge will reject the pull-request creation request and surface the error; no additional round-trip is warranted.

### Non-goals

- Back-merging (e.g. git-flow's `main → develop` sync after release) is a separate workflow concern and is not addressed here.
- `cursus publish` behaviour is unchanged; it reads HEAD, packages, and tag state, and never inspects branch names.
- `cursus ci` dispatch logic is unchanged; the prepare-vs-publish switch remains driven by changeset presence and tag absence per [ADR-015](015-ci-managed-release-workflow.md), not by branch names.
- `release_branch_prefix` is not renamed; its semantics are orthogonal to the new target mapping.

## Consequences

### Positive

- Trunk-based workflows that isolate released state on a dedicated branch become expressible without manual PR re-targeting.
- Git-flow workflows can produce release PRs that target the production branch directly from `develop`.
- Existing repositories require no configuration change; the empty-map default preserves today's "PR into self" behaviour exactly.
- The `--target` CLI flag gives CI jobs deterministic target selection without committing configuration, symmetric with the existing `--branch` override.
- The release branch name stays keyed to the source branch, so parallel release flows targeting a common branch do not collide.
- Validation failure modes are delegated to the forge, avoiding an additional network round-trip on every `prepare` invocation.

### Negative

- A misconfiguration (e.g. a typo in the target branch name) surfaces only at PR-creation time as a forge-side error, not as a preflight failure.
- The `[git]` configuration surface grows. Operators must understand the interaction between `release_branch_prefix` (names the release branch), `release_targets` (chooses its PR base), and the `--branch`/`--target` CLI overrides.
- Explicit identity mappings (`main = "main"`) are valid but redundant with the fallback; operators who include them for documentation may later wonder whether they change behaviour.

### Neutral

- The CLI surface of `prepare` gains a `--target` flag alongside `--branch`; neither flag has any effect when `strategy = "push"`.
- Target resolution folds into the existing branch-state computation in the `prepare` git lifecycle; no new orchestration layer is introduced.
- Detached HEAD continues to error out; this ADR does not add or remove any detached-HEAD behaviour.

## Alternatives Considered

### Single scalar `release_target`

A single top-level `[git].release_target = "main"` string, applied uniformly regardless of source branch.

Rejected because it cannot express git-flow, where `develop` and `main` may both serve as source branches in the same repository and require different targets simultaneously. A scalar collapses the mapping to one entry and forces operators back to CLI overrides or out-of-band scripting for the second source branch.

### Scalar shorthand alongside the per-source table

Support both a scalar default and a per-source map, with the scalar used as a fallback when no map entry matches.

Rejected because the empty-map default already makes the zero-configuration case zero-ceremony: unmapped branches fall back to the current behaviour ("PR into self"), which is the right default. Adding a scalar shorthand introduces two ways to express the same intent for the single-branch case and expands the precedence rules without adding expressive power.

### Error when the source branch is unmapped

Require every source branch that runs `prepare` to be explicitly listed in `release_targets`, erroring otherwise.

Rejected because it would break every existing repository on upgrade. The fallback-to-self behaviour is backwards-compatible and preserves the principle that configuration additions should not invalidate existing configs.

### Derive the release branch name from the target instead of the source

Name the release branch using the target branch (e.g. `cursus-release/release` when targeting `release` from `main`).

Rejected for two reasons. First, it collides when two distinct source branches target the same branch: a git-flow repository preparing from `develop` and a hotfix from `main`, both targeting `main`, would produce identical release branch names. Second, the release branch name loses its provenance — an operator reading `cursus-release/release` cannot tell which source branch the changesets came from.
