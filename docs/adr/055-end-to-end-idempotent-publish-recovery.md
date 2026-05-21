# ADR-055: End-to-End Idempotent Publish Recovery

## Status

Accepted (2026-05-03)

## Context

`cursus publish` orchestrates three sequential stages for each releasable package: registry publish (cargo / npm), git tag creation and push, and GitHub Release creation with artifact upload. Each stage can fail independently. Today, the recovery story across these stages is incomplete: if a package is successfully published to its registry in run N but the subsequent tag or GitHub Release stage fails, re-running `cursus publish` does not recover. The user must perform the remaining steps manually.

Two interacting problems cause this gap.

**Problem 1 — `Skipped` packages bypass the tag and release stages entirely.**

When the registry reports "version already published" (cargo: "is already uploaded"; npm: `EPUBLISHCONFLICT`), the publish adapter returns `PublishResult::Skipped` to honour the idempotency contract from [ADR-004](004-publish-command.md) §Idempotency. The publish state machine, however, only adds packages with a `Published` outcome to its internal `state.published` list. `Skipped` packages are accounted for in `skipped_count` and dropped.

The git release stage (`run_git_release_operations`) iterates `state.published` and only `state.published`. Tag creation is already idempotent — it checks `git.tag_exists()` before creating — but for a skipped package the loop never reaches that check. The same applies to `orchestrate_github_releases`. As a result, once a package has been published in any prior run, the tag and release stages become permanently unreachable for that package on subsequent re-runs unless the version is bumped again.

**Problem 2 — GitHub Release creation has no idempotency.**

`orchestrate_github_releases` calls `code_forge_client.create_release(&tag, ...)` unconditionally. Octocrab's `create_release` always opens a new draft. When a release for that tag already exists on GitHub, the API returns HTTP 422 — the call fails, and there is no fallback. The user has no way to recover via `cursus publish`; they must either delete the existing release manually or skip the GitHub stage entirely.

[ADR-004](004-publish-command.md) §Idempotency documents the registry-only idempotency model ("treat 'already exists' as success"). [ADR-005](005-github-releases.md) §Error handling documents that "if a GitHub Release fails, we report and exit non-zero but do not roll back the registry publish." Neither ADR addresses recovery of the tag and release stages across re-runs after a partial failure.

## References

- [ADR-004: Publish Command](004-publish-command.md) — defines the registry-only idempotency contract this ADR extends.
- [ADR-005: GitHub Releases Integration](005-github-releases.md) — defines the GitHub Release stage of the publish pipeline.
- [ADR-008: Dry-Run Must Be Strictly Local-Only](008-dry-run-local-only-guarantee.md) — constrains the behaviour of the new pre-check API call under dry-run.
- [ADR-015: CI-Managed Release Workflow](015-ci-managed-release-workflow.md) — establishes that tag creation and GitHub Releases are part of `cursus publish`.
- [ADR-038: Octocrab GitHub Client](038-octocrab-github-client.md) — the underlying client whose `create_release` lacks idempotency.
- [ADR-041: Rename `GitHubClient` Trait to `CodeForgeClient`](041-rename-github-client-trait-to-code-forge-client.md) — the trait this ADR extends with `find_release_by_tag`.

## Decision

Extend `cursus publish` to be idempotent end-to-end across all three stages: registry publish, git tag creation, and GitHub Release creation.

### Change 1: `Skipped` registry results flow into the tag and release stages

The semantics of `state.published` (the input to `run_git_release_operations`) shall be redefined to mean "packages whose target version is on the registry, whether published in this run or in a previous run." Concretely:

- `PublishResult::Skipped` shall add the package to `state.published` in addition to incrementing `skipped_count`.
- `state.published` is no longer a record of work performed in the current invocation; it is a record of packages eligible for downstream stages.
- Summary accounting shall use `skipped_count` to disambiguate "newly published" from "already on registry" so the summary line continues to read "X published, Y skipped" correctly. The total `X + Y` equals the number of packages that flow into the tag and release stages.
- Dry-run is unaffected: `Skipped` outcomes do not occur in dry-run because no registry calls are made.

The tag stage already short-circuits on `git.tag_exists()`, so this change immediately gives skipped packages a path to tag creation when their tag is missing.

### Change 2: Pre-check for an existing GitHub Release before creating one

A new method shall be added to the `CodeForgeClient` trait:

```rust
async fn find_release_by_tag(&self, tag: &str) -> anyhow::Result<Option<ExistingRelease>>;
```

`ExistingRelease` carries the fields required to make a recovery decision: `id: String` (release identifier) and `is_draft: bool`. Implementations may carry additional fields if needed by future logic, but the public surface starts minimal.

The `OctocrabGitHubClient` implementation delegates to `repos().releases().get_by_tag(tag)`, mapping HTTP 404 responses to `Ok(None)`. All other errors propagate. The tag string is percent-encoded before being placed in the URL path so that tags containing reserved characters (space, `?`, `#`, `%`, `/`) are transmitted to the GitHub API exactly as stored, avoiding ambiguity at the URL boundary. Test doubles in the `test-support` feature flag implement the same contract.

`orchestrate_github_releases` shall call `find_release_by_tag` before any mutating release API. The decision tree is:

- **No release exists** → continue with the existing path: create draft, upload artifacts, publish draft.
- **A published (non-draft) release exists** → log an informational skip, count the package as "release already present," and do not call any mutating API. The publish run continues for remaining packages.
- **A draft release exists** → log a clear, actionable error message instructing the user to either finalise or delete the draft manually (e.g., via the GitHub UI or `gh release delete <tag>`) and re-run. Set `github_failed = true` for this package. Do not modify the draft in any way.

`find_release_by_tag` lookup failures other than 404 are handled per-package: the failure is logged, the package is marked as failed, and orchestration continues with remaining packages. This matches the existing per-package error-handling contract used by `create_release` and ensures that one transient API hiccup does not abort the entire publish run.

Under dry-run, `find_release_by_tag` is a read-only API call and is permitted under [ADR-008](008-dry-run-local-only-guarantee.md). However, no mutating call (create, upload, publish) is made regardless of the pre-check outcome — dry-run continues to print only what would happen.

### Why we deliberately decline to auto-recover draft releases

Resuming a draft would require listing existing assets to avoid re-uploading, and even then a partially uploaded asset occupies a name slot permanently — overwriting it would require a delete-and-re-upload sequence not currently in the API surface. More importantly, a draft release may be the result of an in-progress manual review by a human reviewer who has not yet hit "Publish." Modifying it without consent is surprising and dangerous. The safe default is to surface the state and let the user decide how to proceed.

### `cursus ci` impact

`cli/ci.rs` dispatches directly to `cmd_publish` and inherits this behaviour without changes. No new configuration flag is introduced; the new behaviour is the only behaviour.

## Consequences

### Positive

- Re-running `cursus publish` after a partial failure now completes the tag and GitHub Release stages for any package whose registry publish succeeded in a previous run, eliminating a manual intervention gap that previously required users to construct git tags and GitHub Releases by hand.
- The full publish pipeline becomes idempotent at every stage where automation is safe. Tag creation was already idempotent; this change brings registry-skip handling and GitHub Release creation up to the same standard.
- The explicit "draft release blocks recovery" error message tells users exactly what to do without guessing, and it surfaces the existence of the draft (which may have been opened by a human collaborator) rather than silently overwriting it.
- The registry-only idempotency contract from [ADR-004](004-publish-command.md) §Idempotency is preserved and strengthened: the principle of "treat already-published as success" now extends through the entire pipeline rather than stopping at the registry boundary.

### Negative

- Each package incurs one additional GitHub API GET (`find_release_by_tag`) per release run. In practice this is negligible (one call per package, not per artifact), but it does increase API usage for users near rate limits.
- The draft-release error path requires a deliberate manual step from the user. Users who expected `cursus publish` to "just resolve" any state will be surprised. This is intentional — see the rationale in the Decision section.
- `state.published` semantics widen. Code that read `state.published` as "packages we just uploaded" will now see entries that were uploaded in a previous run. Summary math must explicitly subtract `skipped_count` to avoid double-counting.

### Neutral

- The new `find_release_by_tag` method is additive on `CodeForgeClient`; no existing call sites change. Test doubles must implement it, but the trait extension is backward-compatible at the consumer level.
- Behaviour under `[github].enabled = false` is unchanged: the GitHub stage is skipped entirely, so neither the pre-check nor the draft-handling path runs.

## Alternatives Considered

### Auto-recover draft releases by inspecting and reconciling assets

`orchestrate_github_releases` could list existing assets on a draft release, skip uploads for assets already present at the expected name, upload missing ones, and then publish the draft. This was rejected because (1) GitHub asset slots are name-keyed and partially uploaded assets cannot be safely overwritten without a delete-then-upload sequence that risks user-visible inconsistency mid-run; (2) a draft release may represent in-flight human work and silently mutating it violates the principle of least surprise; (3) the additional logic significantly expands the surface area of `orchestrate_github_releases` for a marginal recovery case that the manual workflow handles cleanly.

### Add a `--force` or `--resume-drafts` flag to opt into draft mutation

Introducing a flag would let advanced users opt into the auto-recovery path. This was rejected because the default behaviour (manual recovery) is already the right answer for the majority case, and adding an opt-in flag obligates the project to maintain two divergent code paths through the GitHub stage. If users repeatedly request this, it can be added in a follow-up ADR with concrete evidence of demand.

### Track publish progress in a local state file (e.g., `.cursus/publish-state.json`)

A local state file could record which packages reached which stage, allowing `cursus publish` to resume from the precise point of failure without querying registries or GitHub. This was rejected because it duplicates information that the registry and GitHub already authoritatively hold, introduces a new file lifecycle to manage (creation, cleanup, conflict with concurrent runs), and could become stale or contradict reality if anything changes outside Cursus's view (e.g., a human deletes a draft). Querying the source of truth is simpler and more robust.

### Treat HTTP 422 from `create_release` as success

Instead of pre-checking, `orchestrate_github_releases` could catch the HTTP 422 response from a duplicate-tag `create_release` and treat it as "release already exists, skip." This was rejected because HTTP 422 is also returned for legitimate validation failures (malformed body, invalid tag name, etc.), so the catch would have to inspect the response payload to disambiguate. Pre-checking with `get_by_tag` is unambiguous and gives the code direct access to the `is_draft` flag, which is required to differentiate the safe-skip case from the user-action-required case.

## Errata

### 2026-05-13: `orchestrate_github_releases` renamed; draft branch unreachable on GitLab

References to `orchestrate_github_releases` in this ADR are incorrect: [ADR-056](056-gitlab-support-client-config-and-ci.md) renames it to `orchestrate_forge_releases` and adds a second `CodeForgeClient` implementation (`ReqwestGitLabClient`) whose `find_release_by_tag` returns `Ok(None)` on 404 and `Ok(Some(ExistingRelease { is_draft: false, .. }))` on hit — `is_draft` is always `false` on GitLab because the concept does not exist there. The recovery decision tree this ADR defines is unchanged, but the draft-release recovery branch is unreachable under the GitLab forge; the GitLab path exercises only the "no release exists" and "published release exists" arms.
