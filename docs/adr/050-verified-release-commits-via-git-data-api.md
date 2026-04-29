# ADR-050: Produce Verified Release Commits via the GitHub Git Data API

## Status

Accepted (2026-04-29)

## Context

Cursus's CI-managed release workflow ([ADR-015](015-ci-managed-release-workflow.md)) runs `cursus ci` from `.github/workflows/release.yml` under the identity of the `zantarix-ci` GitHub App. Inside that flow, the `prepare` step produces a single `ci(release): version packages` commit that bumps versions, regenerates lockfiles, writes changelog updates, and consumes the changesets folder. That commit is created by the local `git` binary on the runner, invoked through the `Git` trait abstraction ([ADR-035](035-git-trait-abstraction.md)) -- specifically `GitWorkdir`, which dispatches `git commit` and `git push` via `CommandRunner.run_mut`.

The bot's identity is configured by setting `git config user.name` and `user.email` to the App's `bot[users.noreply]` address before the commit is made. This is sufficient to attribute the commit to `zantarix-ci` in the GitHub UI, but it is not sufficient to mark the commit as **Verified**. Because the commit object is built locally and pushed over HTTPS without a GPG or SSH signature, GitHub displays it with an **Unverified** badge.

There is a path to verified commits that does not require provisioning a long-lived signing key: when a commit is created via the GitHub REST Git Data API (`POST /repos/{owner}/{repo}/git/blobs`, `POST /git/trees`, `POST /git/commits`, `PATCH /git/refs/heads/{branch}`) and the `POST /git/commits` body **omits the `author` and `committer` fields**, GitHub fills the committer with `GitHub <noreply@github.com>` and signs the resulting commit object using GitHub's web-flow GPG key (publicly available at `https://github.com/web-flow.gpg`). The commit lands on the remote ref already verified.

This behaviour is observable in the cursus repository today. Commit `06e29bba9747a79a9043f149bf7429fd0194028f` was produced by the sibling `api` project's `GitHubBackend` using exactly this pattern; inspecting the raw commit object shows a `gpgsig` trailer signed by GitHub's web-flow key, and GitHub's UI marks it Verified.

Community evidence indicates this Verified outcome requires a GitHub App installation token rather than a personal access token (PAT). GitHub's official documentation does not state this distinction explicitly; it is asserted by multiple independent third-party implementations of the same pattern:

- <https://github.com/orgs/community/discussions/50055>
- <https://github.com/IAreKyleW00t/verified-bot-commit>
- <https://github.com/suzuki-shunsuke/commit-action>
- <https://gist.github.com/swinton/03e84635b45c78353b1f71e41007fc7c>

`release.yml` already authenticates as the `zantarix-ci` GitHub App, so the necessary token type is available to cursus inside the release flow with no additional plumbing.

This direction is consistent with the project's keyless trust posture. [ADR-049](049-signed-release-artifacts.md) (artifact attestations via Sigstore), [ADR-045](045-crates-io-trusted-publishing.md) (crates.io trusted publishing), and [ADR-028](028-npm-oidc-trusted-publishing.md) (npm trusted publishing) all establish that cursus prefers OIDC- or App-token-based identity over long-lived secrets for any credential-bearing operation. Verified commits via the Git Data API extend that posture to the git layer without introducing a new signing key to rotate, revoke, or audit.

## Decision

We will introduce a `SignedCommitGit` decorator in the cursus library that wraps any `Arc<dyn Git>` implementation and overrides `commit()`, `push()`, and `force_push_branch()` to route the commit through the GitHub Git Data API. All other `Git` trait methods will delegate unchanged to the inner implementation. The decorator will be installed by the binary crate's environment-detection layer ([ADR-030](030-bin-lib-crate-separation.md)) when the configured policy and runtime environment both warrant it.

### Decorator semantics

`SignedCommitGit` will be constructed with three collaborators captured by `Arc`: the inner `Git` implementation (typically `GitWorkdir`), the `Filesystem` (so the decorator can read staged file bytes), and the `CodeForgeClient` whose underlying `octocrab` instance carries the App installation token. It will additionally capture a `dry_run: bool` flag at construction time.

The trait method overrides behave as follows:

- **`add(files)`**: the decorator delegates to the inner `Git` impl so that `git add` still runs against the local index for working-tree consistency, and additionally records the staged paths in an internal list. The recorded list is what the API commit will read from disk and turn into blobs.
- **`commit(message)`**: for each path recorded by `add`, the decorator reads the file's bytes via the `Filesystem` trait, then performs the API sequence to create the commit object: create one blob per file, create a tree referencing those blobs against the parent tree, and create a commit object with the message and parent SHA but **without** `author` and `committer` fields (this is what triggers GitHub's web-flow signing). The returned commit SHA and the target branch name are stored in the decorator's internal state, and the staged-paths list is cleared. No local git operations happen here -- the commit object exists on GitHub but is not yet reachable from any ref (local or remote), so `git fetch` cannot see it yet.
- **`push()`**: calls `PATCH /git/refs/heads/{branch}` with `force: false`, using the SHA stored by `commit()`. Once the remote branch ref points at the new commit (and the object is therefore reachable on the remote), the decorator runs `git fetch origin {branch}` through the inner runner to download the new commit object and its tree/blobs into the local object store and advance `origin/{branch}`, followed by `git reset --hard FETCH_HEAD` to move the local branch ref, index, and working tree to the fetched SHA. Equivalent semantics to a normal fast-forward push.
- **`force_push_branch(branch)`**: same sequence as `push()`, but calls `PATCH /git/refs/heads/{branch}` with `force: true`. Equivalent semantics to `--force-with-lease`.
- All other methods (`tag`, `push_tag`, `delete_tag`, `checkout`, `is_dirty`, diff/log/ref operations, etc.): delegate to the inner impl unchanged.

The post-push `git fetch` and `git reset --hard` performed inside `push()` and `force_push_branch()` is non-optional. When `SignedCommitGit::commit()` routes the commit through the API, the resulting commit object is created on GitHub's servers and does not exist in the local git object store; no local `git commit` is run, so the local index, working tree, and branch ref are all out of sync with the API-produced commit. The fetch cannot happen inside `commit()` itself because the commit object is not yet reachable from any ref on the remote -- the branch still points at the old HEAD until the ref-update call in `push()`/`force_push_branch()` advances it, and `git fetch origin {branch}` only downloads objects reachable from that ref. Once the ref-update call has advanced the remote branch to the new commit, `git fetch origin {branch}` downloads the new commit object (and its tree and blobs) into the local object store and advances the `origin/{branch}` remote-tracking ref; the subsequent `git reset --hard FETCH_HEAD` then moves the local branch ref, index, and working tree to the fetched SHA. Together these two commands ensure the local git state (object store, branch ref, index, working tree) is consistent with the API-created commit so that subsequent git operations in the same process -- in particular `is_dirty()` checks and any operation that needs to resolve the new commit -- work correctly.

### Dry-run handling

[ADR-017](017-late-guard-dry-run-pattern.md) places the dry-run guard inside `DryRunCommandRunner`, which intercepts mutating `CommandRunner` calls. `SignedCommitGit::commit()` makes its mutations through `octocrab` (HTTP), not through `CommandRunner`, so it is not covered by that interceptor. To preserve the dry-run guarantee from [ADR-008](008-dry-run-local-only-guarantee.md), the decorator will explicitly check the `dry_run` flag captured at construction time and short-circuit before issuing any HTTP request, logging the intended commit instead.

This is a deliberate, documented exception to the late-guard pattern: it is the smallest change consistent with [ADR-008](008-dry-run-local-only-guarantee.md)'s strictly-local-only guarantee for a code path that does not flow through `CommandRunner`. The exception is local to this one decorator.

### Configuration

A new `signed_commits` field will be added to the `[git]` section of `.cursus/config.toml`:

```toml
[git]
signed_commits = "auto"   # "auto" (default) | "force" | "off"
```

The semantics are:

- **`auto`** (default): the API commit path is enabled when the binary detects that it is running under GitHub Actions (`GITHUB_ACTIONS=true`) AND a GitHub token is available via `GH_TOKEN` or `GITHUB_TOKEN`. Both conditions must hold; outside CI or without a token the decorator is not installed and commits go through the local `git` binary as today.
- **`force`**: the API commit path is enabled whenever a token is available, regardless of CI environment. This exists for users who want verified commits from contexts outside GitHub Actions. It is documented as experimental and not exercised in real-world conditions by this ADR.
- **`off`**: the local `git` binary is always used, even in CI with a token available. This is the escape hatch for users who do not want the decorator's behaviour at all.

The detection logic and the resulting wrap of `Arc<dyn Git>` will live in `packages/cursus-bin/src/main.rs`, consistent with the principle ([ADR-030](030-bin-lib-crate-separation.md)) that environment detection happens only at the binary boundary. The library crate exposes the decorator type and constructor; the library itself is unaware of `GITHUB_ACTIONS`, `GH_TOKEN`, or `GITHUB_TOKEN`.

### Token type and Verified outcome

The Verified badge depends empirically on the token used being a GitHub App installation token rather than a PAT. Cursus will not gate the API commit path on token type, because GitHub does not officially document the requirement and any check would be a heuristic on opaque token strings. With `signed_commits = "force"` and a PAT, the API commit will succeed but the resulting commit may appear Unverified -- this is an accepted limitation, documented for users.

Inside `release.yml`, where `GITHUB_TOKEN` is the App installation token, the `auto` policy will produce verified commits as designed.

### Scope

In scope:

- The `ci(release): version packages` commit produced by `cursus prepare` during the release flow.

Out of scope:

- **Tag object signing.** The release tag will continue to be created as an annotated-but-unsigned tag via the local `git` binary. GitHub's Verified badge on a release surfaces against the commit the tag points to, which is the user-visible signal; signing the tag object itself would require either a long-lived key or a separate API path with no clear additional benefit. A future ADR may revisit tag signing if the need arises.
- **`cursus change` commits.** These are produced on contributor machines, where local git config (including the contributor's own `commit.gpgsign` or SSH signing setup) governs signing behaviour. Forcing them through an API path would break offline use, require every contributor to hold a GitHub token, and provides no comparable security benefit.
- **`cursus init` and other non-release commits.** Same reasoning: these run on developer machines, not under the bot identity.

### Relationship to existing ADRs

- [ADR-015](015-ci-managed-release-workflow.md): this ADR refines the prepare-side commit producer used by the CI-managed release flow without changing the flow's overall shape.
- [ADR-017](017-late-guard-dry-run-pattern.md): this ADR introduces a local exception to the late-guard pattern, scoped to `SignedCommitGit::commit()`. The exception is documented above and does not alter the pattern for any other call site.
- [ADR-035](035-git-trait-abstraction.md): the decorator is implemented at the `Git` trait boundary, which exists precisely to allow alternative implementations to be composed in.
- [ADR-030](030-bin-lib-crate-separation.md): the decision of whether to install the decorator is made in the binary crate based on environment detection; the library crate remains environment-agnostic.

## Consequences

### Positive

- The `ci(release): version packages` commit on `main` shows as Verified in GitHub's UI, providing a stronger trust signal for the release history that complements the artifact-level signing established by [ADR-049](049-signed-release-artifacts.md).
- No long-lived signing key is introduced. The existing GitHub App installation token already present in `release.yml` is sufficient. This preserves the keyless posture established by [ADR-028](028-npm-oidc-trusted-publishing.md), [ADR-045](045-crates-io-trusted-publishing.md), and [ADR-049](049-signed-release-artifacts.md).
- The decorator is opt-out (`signed_commits = "off"`) and falls back to unmodified `GitWorkdir` behaviour when not installed, so existing users see no change unless they update their config or run under the matching CI conditions.
- No changes are required to `.github/workflows/release.yml`. The token cursus needs is the same `GITHUB_TOKEN` already present in the workflow.

### Negative

- `SignedCommitGit::commit()` performs multiple network round-trips against the GitHub API (one per file for blob creation, plus tree, commit, and ref-update calls). This is slower than a local `git commit`. Acceptable given that prepare commits typically touch fewer than 20 files, but it does extend release-flow wall time noticeably on large monorepos.
- The dry-run guarantee for the API commit path is enforced manually inside the decorator rather than via the central `DryRunCommandRunner` interceptor. This is a local exception to [ADR-017](017-late-guard-dry-run-pattern.md). It is isolated to one method, but it does mean future contributors must remember that the late-guard pattern does not cover HTTP mutations.
- The Verified outcome empirically depends on the token being a GitHub App installation token. Users running cursus with `signed_commits = "force"` and a PAT will get a successful commit that may appear Unverified. Because GitHub does not officially document the token-type requirement, the code does not guard against this case; it is documented but not validated.
- Tag objects remain unsigned. This is an accepted limitation of the initial implementation and may need revisiting if downstream consumers begin to verify tag signatures.
- The `git fetch origin {branch}` + `git reset --hard FETCH_HEAD` realignment performed inside `push()` and `force_push_branch()` is required to keep the local object store, branch ref, index, and working tree consistent with the API-created commit (since no local `git commit` runs and the new commit object initially exists only on GitHub's servers, reachable only after the ref-update call advances the branch). It briefly diverges the working tree from any local-only state the user might have introduced between staging and pushing. In the release flow this is benign (cursus controls the working tree end-to-end), but it would matter if the API commit path were ever extended outside that flow.

### Neutral

- Adds a new optional dependency on `octocrab`'s Git Data API surface. `octocrab` is already used by the project ([ADR-038](038-octocrab-github-client.md)), so no new dependency tree is introduced.
- The `signed_commits` field defaults to `"auto"` for new and existing projects. Existing projects without the field in their `.cursus/config.toml` get the auto behaviour, which is a no-op outside CI and produces verified commits inside the release workflow. This is a behavioural change for the release commit specifically; users who object can set `signed_commits = "off"`.
- The `[git].publish_private_packages` and other `[git]` fields are unaffected.

## Alternatives Considered

### GPG or SSH signing with a long-lived bot key stored as a CI secret

Configure `git config commit.gpgsign true` and provision a GPG or SSH signing key as a GitHub Actions secret. The local git binary would sign commits using that key. The verifying public half would be registered against the bot's GitHub identity.

Rejected because it reintroduces long-lived key custody. Rotation, revocation, secret hygiene, and onboarding/offboarding of maintainers who hold the key all become ongoing operational burdens. This contradicts the keyless posture that [ADR-028](028-npm-oidc-trusted-publishing.md), [ADR-045](045-crates-io-trusted-publishing.md), and [ADR-049](049-signed-release-artifacts.md) deliberately walked towards. The Git Data API path achieves the same Verified outcome with no key custody.

### GraphQL `createCommitOnBranch` mutation

GitHub offers a GraphQL `createCommitOnBranch` mutation that produces Verified commits via the same web-flow GPG signing mechanism, with native multi-file support in a single round trip.

Not chosen for this ADR because the REST Git Data API path is already proven in the sibling `api` project against the same trust root, with the same Verified outcome. The REST path is simpler to model in cursus's existing octocrab usage (no GraphQL schema, no base64-encoding of `fileAdditions`), and the per-file blob round-trips are not a meaningful cost at the file counts cursus produces. The GraphQL endpoint remains an equally valid alternative if the REST path proves insufficient in future; nothing in this decision precludes a later switch.

### Wholesale lift of the `api` project's `GitHubBackend`

The sibling `api` project (`api/src/cursus/backend/github/mod.rs`) implements both `Git` and `Filesystem` traits backed entirely by the GitHub API. Lifting that backend wholesale would give both projects a shared API-backed implementation.

Not chosen for this ADR because the `api` backend couples its `Git` and `Filesystem` impls -- file writes are accumulated in an in-memory `staged_writes` map and only flushed when the API commit is created. Cursus assumes a workspace on disk: package manager adapters call `cargo metadata`, enumerate projects from on-disk manifests, regenerate lockfiles, and render changelogs against the working tree. None of that tooling would see writes that exist only in an in-memory map. The two projects have different operational models, and the decorator pattern is the minimal change that keeps `LocalFilesystem` in place and re-routes only the commit primitive. A future ADR may revisit factoring out a shared backend crate once the proven API types in the `api` project can be extracted cleanly.

### GitHub Actions-only fix without a cursus-level change

Adjust `release.yml` to perform the verified commit directly via a third-party action (`suzuki-shunsuke/commit-action`, `IAreKyleW00t/verified-bot-commit`, or hand-rolled `gh api` calls), bypassing `cursus ci` for the git operations and reducing cursus's role to producing the file changes.

Rejected because it splits release-commit logic across the workflow file and the cursus binary, undermining the principle that cursus owns the release lifecycle end to end ([ADR-015](015-ci-managed-release-workflow.md)). It also makes verified commits invisible to anyone running cursus outside this specific GitHub Actions workflow, including users with `signed_commits = "force"` who want the same outcome on their own infrastructure.
