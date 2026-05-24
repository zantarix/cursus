# ADR-058: Produce Verified Release Commits on GitLab via the Web Commits API

## Status

Accepted (2026-05-22)

## Context

[ADR-050](050-verified-release-commits-via-git-data-api.md) established that the `ci(release): version packages` commit produced by `cursus prepare` on GitHub should appear as **Verified**, and did so without provisioning a long-lived signing key: it routes the commit through the GitHub Git Data API (`POST /git/blobs`, `/git/trees`, `/git/commits`, `PATCH /git/refs/heads/{branch}`) with the `author` and `committer` fields omitted, so GitHub signs the resulting commit with its web-flow GPG key. This extended the keyless-trust posture already established by [ADR-028](028-npm-oidc-trusted-publishing.md), [ADR-045](045-crates-io-trusted-publishing.md), and [ADR-049](049-signed-release-artifacts.md) to the git layer.

[ADR-056](056-gitlab-support-client-config-and-ci.md) introduces a parallel GitLab integration — client, config section, and binary-boundary environment detection — and is intentionally structured so that [ADR-050](050-verified-release-commits-via-git-data-api.md)'s signed-commit story can be ported to GitLab without re-litigating the trait-shape decisions. As of [ADR-056](056-gitlab-support-client-config-and-ci.md) being implemented, however, the release commit produced when GitLab is the active forge would still be created by the local `git` binary through `GitWorkdir` and would therefore appear **Unverified** in the GitLab UI. That would leave the supply-chain story for GitLab users a strict subset of what GitHub users get.

The blocker for parity used to be technical: GitLab did not previously sign commits created through its API. That changed in GitLab 18.10. The `configure_web_based_commit_signing` feature flag was introduced in 18.3, enabled on GitLab.com in 18.9, and made generally available with the flag removed in 18.10. From that release onward, commits created through `POST /projects/:id/repository/commits` are SSH-signed by the GitLab instance's web-commits signing key (publicly retrievable via `GET /metadata/web_commits/public_key`) and appear as Verified, exactly mirroring GitHub's web-flow-signed-API-commit pattern that [ADR-050](050-verified-release-commits-via-git-data-api.md) depends on.

GitLab's API also enforces a constraint that GitHub's does not: when web-commit signing is active, commits whose `author_name` / `author_email` do not match the authenticated user are **rejected** by the API rather than merely produced unsigned. This is the equivalent of [ADR-050](050-verified-release-commits-via-git-data-api.md)'s "omit author/committer to enable signing" rule, but it is enforced strictly at the API boundary instead of being a quirk of behaviour to discover.

The two forge APIs differ enough in shape that a per-forge decorator is the right abstraction. GitHub requires a four-call sequence (blob × N, tree, commit, ref-update); GitLab accepts a single call to `POST /projects/:id/repository/commits` containing an `actions` array with per-file `create` / `update` / `delete` operations and a `branch` field. GitHub's ref-update is a separate `PATCH /git/refs/heads/{branch}` call; GitLab's commits endpoint advances the branch as part of the same call and exposes a native `force` flag (with `start_branch` / `start_sha` for the parent reference) for force-update semantics. A unified abstraction over both would have to model the union of those shapes inside the trait, which would obscure rather than illuminate the design.

This ADR adds the GitLab counterpart and, for naming symmetry, renames the existing GitHub decorator so the two are named consistently. It is the third of the four-ADR GitLab batch ([ADR-056](056-gitlab-support-client-config-and-ci.md), [ADR-057](057-cursus-init-gitlab-support.md), this ADR, [ADR-059](059-forge-selection-runtime-rules.md)). Cross-forge selection and runtime validation rules between `[github]` and `[gitlab]` remain the responsibility of [ADR-059](059-forge-selection-runtime-rules.md).

## References

- [GitLab Web Commits documentation](https://docs.gitlab.com/user/project/repository/signed_commits/web_commits/)
- [GitLab Commits API](https://docs.gitlab.com/api/commits/)
- [GitLab Web Commits API (public signing key)](https://docs.gitlab.com/api/web_commits/)

## Decision

A new `GitLabSignedCommit` decorator shall be introduced in the cursus library that wraps any `Arc<dyn Git>` (typically `GitWorkdir`) and overrides `commit()`, `push()`, and `force_push_branch()` to route the prepare commit through GitLab's `POST /projects/:id/repository/commits` endpoint, relying on GitLab 18.10's web-commit signing behaviour to produce a Verified commit. All other `Git` trait methods shall delegate unchanged to the inner implementation. The decorator shall be installed by the binary crate's environment-detection layer ([ADR-030](030-bin-lib-crate-separation.md)) when the configured `signed_commits` policy and the runtime GitLab CI environment both warrant it.

For naming symmetry with the new GitLab decorator, the existing GitHub decorator `SignedCommitGit` (introduced in [ADR-050](050-verified-release-commits-via-git-data-api.md)) shall be renamed to `GitHubSignedCommit`, and the file `packages/cursus/src/git/signed_commit.rs` shall be renamed to `packages/cursus/src/git/github_signed_commit.rs`. The module wiring in `packages/cursus/src/git/mod.rs` and the construction site in the binary crate's git-setup boundary (`packages/cursus-bin/src/git_setup.rs`) shall be updated to match.

### File and type layout

- `packages/cursus/src/git/gitlab_signed_commit.rs` — new file containing the `GitLabSignedCommit` type, structured identically to the renamed `github_signed_commit.rs`.
- `packages/cursus/src/git/github_signed_commit.rs` — the existing file at `signed_commit.rs`, renamed; the type inside it is renamed from `SignedCommitGit` to `GitHubSignedCommit`.
- `packages/cursus/src/git/mod.rs` — updated `mod` and `pub use` lines for both decorators.
- The rename does not touch any logic in the GitHub decorator; it is purely a naming change.

### Decorator semantics

`GitLabSignedCommit` shall be constructed with the inner `Git` implementation, the `Filesystem` (so the decorator can read staged file bytes per [ADR-036](036-filesystem-trait-abstraction.md)), the `AsyncGitlab` client whose token carries the necessary scope, the project identity (per [ADR-042](042-repo-identity-in-constructor.md): host + group + project), and a `dry_run` flag.

The trait method overrides shall behave as follows:

- **`add(files)`**: delegates to the inner `Git` impl so that `git add` still runs against the local index for working-tree consistency, and additionally records the staged paths in an internal list. The recorded list is what the API commit will read from disk and turn into `actions`.
- **`commit(message)`**: for each path recorded by `add`, the decorator reads the file's bytes via the `Filesystem` trait, classifies each path as `create`, `update`, or `delete` (a path that no longer exists on disk is a delete; a path that exists but has no prior commit object reachable is a create; everything else is an update), and builds a single `POST /projects/:id/repository/commits` request whose `actions` array carries all of those operations. **The request body must omit `author_email` and `author_name`** — see "Author fields must be omitted" below. The target branch and parent SHA are recorded but the API call is not yet made; that happens at `push()` time, because the `branch` field on this endpoint is what advances the remote ref. The local working tree is not touched at this point.
- **`push()`**: calls `POST /projects/:id/repository/commits` with the recorded `branch` set to the current branch, `start_sha` set to the parent commit SHA recorded at `commit()` time, and `force: false`. On success, runs `git fetch origin {branch}` and `git reset --hard FETCH_HEAD` through the inner runner to bring the verified commit object into the local object store and sync the local branch ref, index, and working tree. Equivalent semantics to a normal fast-forward push.
- **`force_push_branch(branch)`**: same as `push()` but with `force: true` and the target branch passed explicitly. GitLab's commits endpoint accepts `force: true` together with `start_sha` to overwrite the named branch with a new commit based on the chosen parent reference, replacing the branch's existing commit history. This is the closest API-level equivalent of `git push --force-with-lease` for this workflow. The same post-call `git fetch` + `git reset --hard FETCH_HEAD` then realigns the local state.

`start_sha` is chosen over `start_branch` because the parent SHA is captured at `commit()` time (after `add()`), so locking the parent to that specific SHA at `push()` time prevents any race where the branch tip on the remote moves between the user staging the commit and the API call landing it. Using `start_branch` would silently re-parent the API commit onto whatever the branch points at when the request is processed; `start_sha` makes the parent reference deterministic.

- All other methods (`tag`, `push_tag`, `delete_tag`, `checkout`, `is_dirty`, diff/log/ref operations, etc.): delegate unchanged to the inner impl.

The post-API `git fetch origin {branch}` + `git reset --hard FETCH_HEAD` sequence performed inside `push()` and `force_push_branch()` is non-optional, for exactly the reasons given in [ADR-050](050-verified-release-commits-via-git-data-api.md): no local `git commit` runs, so the local index, working tree, and branch ref are all out of sync with the API-produced commit until the local state is synced from the now-advanced remote ref. The fetch downloads the new commit object (and its tree and blobs) into the local object store and advances the `origin/{branch}` remote-tracking ref; the subsequent `git reset --hard FETCH_HEAD` moves the local branch ref, index, and working tree to the fetched SHA.

The shape divergence from [ADR-050](050-verified-release-commits-via-git-data-api.md) is that GitLab fuses commit creation and ref update into one API call, whereas GitHub splits them. Internally, that means `GitLabSignedCommit::commit()` records state and `GitLabSignedCommit::push()` performs the API call (so the branch name is known); on the GitHub side, `commit()` performs the multi-step blob/tree/commit creation and `push()` only updates the ref. Both decorators produce the same observable behaviour: a verified commit lands on the remote and the local working tree is synced.

### Author fields must be omitted

GitLab's web-commits documentation states that when commit signing is enabled, commits created through the REST API with `author_name` / `author_email` that differ from the authenticated user are **rejected**. This is stricter than GitHub's behaviour, where mismatched author fields merely produce an unsigned commit.

The decorator shall therefore omit `author_email` and `author_name` from every `POST /projects/:id/repository/commits` request body. GitLab will fill the author with the authenticated user (typically the bot identity carrying the project- or group-access token configured per [ADR-056](056-gitlab-support-client-config-and-ci.md)) and sign the resulting commit with the instance's SSH web-commits signing key.

This is the GitLab analogue of [ADR-050](050-verified-release-commits-via-git-data-api.md)'s "omit `author` and `committer` to trigger web-flow signing" rule, with the added property that violating it is loud (an API error) rather than silent (an unsigned commit).

### Reuse the existing `[git].signed_commits` config enum

The `signed_commits` field defined in `packages/cursus/src/model/config/git.rs` and introduced by [ADR-050](050-verified-release-commits-via-git-data-api.md) shall be reused unchanged. The enum values (`auto` / `force` / `off`) keep their existing meanings; what changes is how the binary boundary interprets them when GitLab is the active forge.

The binary-boundary logic in `cursus-bin/src/git_setup.rs` shall be extended as follows:

- **`auto`** (default): the API commit path is enabled when both conditions hold for the active forge. For GitHub, the existing rule is unchanged: `GITHUB_ACTIONS=true` and a GitHub token available via `GH_TOKEN`/`GITHUB_TOKEN`. For GitLab, the equivalent rule is `GITLAB_CI=true` and a GitLab token available via `GITLAB_TOKEN` (or `CI_JOB_TOKEN` as a fallback, consistent with [ADR-056](056-gitlab-support-client-config-and-ci.md)'s token-precedence rule). Outside CI or without a token, the decorator is not installed and commits go through the local `git` binary as today.
- **`force`**: the API commit path is enabled whenever a token is available for the active forge, regardless of CI environment. As with [ADR-050](050-verified-release-commits-via-git-data-api.md)'s GitHub `force` mode, this is documented as experimental and not exercised in real-world conditions by this ADR.
- **`off`**: the local `git` binary is always used, regardless of forge or CI environment.

Which forge's decorator (`GitHubSignedCommit` vs `GitLabSignedCommit`) is selected is determined by which `CodeForgeClient` is active in `Env`. [ADR-059](059-forge-selection-runtime-rules.md) defines the forge-selection precedence; this ADR's binary-boundary logic shall key off the result of that selection rather than re-implementing it.

No new config key is introduced. The split between "should we sign at all" (the existing `signed_commits` enum) and "which forge am I talking to" (decided elsewhere) keeps each concern local.

### Dry-run handling

Identical to [ADR-050](050-verified-release-commits-via-git-data-api.md): the decorator checks its `dry_run` flag at the top of each overridden mutating method and short-circuits before issuing any HTTP request, logging the intended action instead. This is a deliberate, documented exception to [ADR-017](017-late-guard-dry-run-pattern.md)'s late-guard pattern because the API call is made through the `AsyncGitlab` HTTP client rather than through `CommandRunner`, so the `DryRunCommandRunner` interceptor does not see it. The exception is local to this one decorator and mirrors the precedent established by `GitHubSignedCommit`.

### Best-effort signing on older self-managed instances

Web-commit signing is GA from GitLab 18.10. On self-managed GitLab instances running older versions, `POST /projects/:id/repository/commits` continues to succeed but the resulting commit will be unsigned. Cursus shall not interrogate the GitLab instance version before each commit: the cost of an additional API round-trip per release is real, and the value of the pre-check is marginal because the commit lands either way.

Signing shall therefore be treated as best-effort. If the instance signs, great; if it does not, the commit still lands and the release flow proceeds. The user-facing GitLab integration documentation (per [ADR-056](056-gitlab-support-client-config-and-ci.md)'s docs-site requirement) shall call out this caveat so users on older self-managed instances are not surprised by Unverified commits.

No feature-version probe, no "is your GitLab new enough" check, no gating logic. The decorator's contract is "ask GitLab to make the commit; whatever signing posture the instance has is what you get."

### Verification status fetch is out of scope

[ADR-050](050-verified-release-commits-via-git-data-api.md) does not fetch verification status after pushing, and this ADR does not either. The decorator's contract ends when the commit has landed on the remote and the local working tree has been synced. Users can observe verification state via the GitLab UI or `GET /projects/:id/repository/commits/:sha/signature` if they wish, but cursus does not surface it.

### Terminology

Per [ADR-056](056-gitlab-support-client-config-and-ci.md)'s "neutral abstraction, native vocabulary" rule, every user-visible error message produced by `GitLabSignedCommit` (API failures, parse errors, redacted-body surfacings) shall use GitLab vocabulary: "GitLab project", "merge request" where applicable, "group/project". The internal trait-level term "branch" remains forge-neutral, since both forges call it that.

Internal type names follow the symmetry rule: `GitHubSignedCommit` for the GitHub decorator (renamed from `SignedCommitGit`) and `GitLabSignedCommit` for the new GitLab decorator. Module file names match the type names.

### Scope

In scope:

- The `ci(release): version packages` commit produced by `cursus prepare` when GitLab is the active forge.
- The rename of the existing GitHub decorator and its module file, for naming symmetry.
- The reuse of the existing `[git].signed_commits` config enum, unchanged.

Out of scope (called out explicitly to forestall the equivalent open questions raised by [ADR-050](050-verified-release-commits-via-git-data-api.md)):

- **Tag object signing on GitLab.** Release tags continue to be created as annotated-but-unsigned tags via the local `git` binary. GitLab's Verified badge surfaces against commits the tag points to, which is the user-visible signal. A future ADR may revisit tag signing if downstream consumers begin to verify tag signatures.
- **`cursus change` commits.** Produced on contributor machines where local git config governs signing. Forcing them through an API path would break offline use, require every contributor to hold a GitLab token, and provides no comparable security benefit.
- **`cursus init` and other non-release commits.** Same reasoning: these run on developer machines, not under the bot identity.
- **Cross-forge runtime validation** (e.g., what happens if both `[github]` and `[gitlab]` are enabled). Handled by [ADR-059](059-forge-selection-runtime-rules.md).
- **Init wizard / locale strings for GitLab.** Handled by [ADR-057](057-cursus-init-gitlab-support.md).
- **GitLab client construction, config schema, env detection.** Handled by [ADR-056](056-gitlab-support-client-config-and-ci.md).

### Relationship to existing ADRs

- [ADR-050](050-verified-release-commits-via-git-data-api.md): this ADR is the direct GitLab parallel. An Errata entry on [ADR-050](050-verified-release-commits-via-git-data-api.md) points forward to this ADR for the rename of `SignedCommitGit` to `GitHubSignedCommit`.
- [ADR-035](035-git-trait-abstraction.md): the decorator is implemented at the `Git` trait boundary, which exists precisely to allow alternative implementations to be composed in.
- [ADR-036](036-filesystem-trait-abstraction.md): file reads inside `commit()` go through the `Filesystem` trait.
- [ADR-017](017-late-guard-dry-run-pattern.md): this ADR carves out the same exception to the late-guard pattern that [ADR-050](050-verified-release-commits-via-git-data-api.md) did, for the same reason — HTTP mutations bypass `CommandRunner`.
- [ADR-030](030-bin-lib-crate-separation.md): the decision of whether to install the decorator is made in the binary crate based on environment detection.
- [ADR-052](052-credential-redaction-in-error-messages.md): GitLab API response bodies and the post-API `git fetch` / `git reset` stderr captured inside this decorator's error paths shall pass through `redact_credentials` before being embedded in the resulting `anyhow::Error`, on the same logic that drove [ADR-050](050-verified-release-commits-via-git-data-api.md)'s post-hoc errata.
- [ADR-056](056-gitlab-support-client-config-and-ci.md): provides the GitLab client, the `[gitlab]` config, and the env-detection plumbing that this decorator consumes.

## Consequences

### Positive

- Release commits produced on GitLab show as Verified in the GitLab UI when running under a project- or group-access token on GitLab 18.10 or later, completing the supply-chain story for GitLab users to match what GitHub users have under [ADR-050](050-verified-release-commits-via-git-data-api.md).
- No long-lived signing key is introduced. The existing GitLab token already present in the GitLab CI environment is sufficient, preserving the keyless-trust posture established across [ADR-028](028-npm-oidc-trusted-publishing.md), [ADR-045](045-crates-io-trusted-publishing.md), [ADR-049](049-signed-release-artifacts.md), and [ADR-050](050-verified-release-commits-via-git-data-api.md).
- The `[git].signed_commits` config surface is identical for both forges. Users switching between or operating both forges learn one setting, not two.
- The two decorators are named symmetrically (`GitHubSignedCommit`, `GitLabSignedCommit`), so future maintainers reading the `git/` module will not have to guess that the un-prefixed `SignedCommitGit` is the GitHub one.
- GitLab's single-call commit shape is simpler than GitHub's four-call sequence, so `GitLabSignedCommit::commit()` has lower per-call overhead and fewer failure modes to recover from than its GitHub counterpart.

### Negative

- The dry-run guarantee for the GitLab API commit path is enforced manually inside the decorator rather than via the central `DryRunCommandRunner` interceptor. This is the same exception to [ADR-017](017-late-guard-dry-run-pattern.md) that [ADR-050](050-verified-release-commits-via-git-data-api.md) carved out, now extended to a second site. Future contributors must remember that the late-guard pattern does not cover HTTP mutations in *either* decorator.
- The Verified outcome depends on the user's GitLab instance running 18.10 or later. Users on older self-managed instances will see successful commits that appear Unverified. Documentation calls this out, but the decorator does not detect or warn at runtime.
- GitLab's strict author-mismatch rejection means a misconfigured token (one whose authenticated user does not match what the cursus bot expects) will fail at the API boundary with a possibly cryptic GitLab error. The decorator's error wrapping shall surface a useful message, but the root cause is upstream API behaviour.
- Renaming `SignedCommitGit` to `GitHubSignedCommit` is a breaking change for any external code (test scaffolding, downstream consumers of the library crate) that depended on the old name. The library is not currently versioned as a stable public API, so the practical blast radius is internal, but the rename is still a visible churn in commit history and grep-results for that identifier.
- The post-API `git fetch` + `git reset --hard FETCH_HEAD` realignment is required to keep the local state consistent with the API-created commit, with the same caveat as [ADR-050](050-verified-release-commits-via-git-data-api.md): it briefly diverges the working tree from any local-only state introduced between staging and pushing. In the release flow this is benign; outside that flow it would matter.

### Neutral

- The `gitlab` crate's `AsyncGitlab` is reused for the API calls inside the decorator; no new HTTP client dependency is introduced beyond what [ADR-056](056-gitlab-support-client-config-and-ci.md) already brings.
- `signed_commits = "auto"` becomes the effective default for new and existing GitLab-configured projects, just as it is for GitHub. Existing GitLab projects (if any) that did not previously have a working signed-commits path get the auto behaviour, which is a no-op outside GitLab CI and produces verified commits inside it.
- The `[git].publish_private_packages` and other `[git]` fields are unaffected by this ADR.

## Alternatives Considered

### Local GPG or SSH signing with a long-lived bot key stored as a GitLab CI variable

Configure `git config commit.gpgsign true` (or the SSH equivalent) and provision a signing key as a GitLab CI/CD variable. The local git binary on the runner would sign commits using that key. The verifying public half would be registered against the bot user's GitLab identity.

Rejected on the same grounds as the equivalent alternative in [ADR-050](050-verified-release-commits-via-git-data-api.md): reintroducing long-lived key custody for the release bot contradicts the keyless-trust posture that [ADR-028](028-npm-oidc-trusted-publishing.md), [ADR-045](045-crates-io-trusted-publishing.md), [ADR-049](049-signed-release-artifacts.md), and [ADR-050](050-verified-release-commits-via-git-data-api.md) deliberately walked towards. Rotation, revocation, secret hygiene, and onboarding/offboarding of maintainers who hold the key are operational burdens the API-mediated path avoids entirely.

### Leave GitLab release commits unsigned indefinitely

Until GitLab 18.10, this was the only available option — the API simply did not produce signed commits. Accepting the status quo would mean GitLab users permanently get a weaker supply-chain story than GitHub users, with no technical reason for the asymmetry once 18.10 is in their hands.

Rejected because [ADR-056](056-gitlab-support-client-config-and-ci.md) exists precisely to make GitLab a first-class peer of GitHub, and leaving the signed-commit gap would undermine that goal. The cost of the decorator is bounded (one new file, one rename, no new config surface, no new dependency) and is in proportion to the value of restoring parity.

### Single shared `ApiCommitGit` decorator parameterised by a forge enum

A single decorator type with an internal `enum Forge { GitHub, GitLab }` field that branches between the GitHub and GitLab API shapes inside each overridden method.

Rejected on three grounds. First, the two APIs differ substantively in shape: GitHub uses a four-call sequence (`blob` × N → `tree` → `commit` → ref-update), while GitLab uses a single multi-file `commits` call with `branch` and `force` embedded. A unified decorator would have to model the union of those shapes, which is more complex than two cleanly-separated decorators. Second, the author-override semantics differ in their failure mode (GitHub silently produces unsigned commits on mismatch; GitLab rejects the request outright), and conflating those behind a shared abstraction would hide an asymmetric contract behind a symmetric interface. Third, per-forge decorators are simpler to test in isolation: each one's test suite mocks exactly one forge's API, with no cross-forge enum-branch coverage to chase. The cost of having two small, similar files is lower than the cost of one file with a forge discriminator threaded through every method.

### Build cursus's own commit-signing layer with our own key, identical across forges

Rather than relying on either forge's web-commit signing, cursus could ship a signing key (e.g., embedded in the binary or fetched from a per-project location) and sign commits locally before pushing via either forge's normal git transport.

Rejected for the same key-custody reasons as the first alternative above, with the additional drawback that it duplicates work both forges have already done well. The forges already maintain a publicly-discoverable trust root for their own web-commits keys; building a parallel cursus-specific trust root would be a strictly inferior version of that.

## Errata

### 2026-05-24: Instance version is not the only signing prerequisite — a per-project/group setting is also required

The "Best-effort signing on older self-managed instances" section frames the GitLab instance version (18.10+, when `configure_web_based_commit_signing` went GA) as the sole gating factor for whether an API-created commit is signed. That is functionally incorrect: signing also requires the **"Sign web-based commits"** setting to be explicitly enabled at the project or group level (Settings → Repository → General), and that setting is OFF by default. Real-world testing on GitLab.com with a valid `GITLAB_TOKEN` confirmed this — the `GitLabSignedCommit` decorator behaved correctly (it omits `author_email`/`author_name` as required, and the commit was accepted rather than rejected, proving the author fields are not at fault), yet the landed commit showed as Unverified because the per-project setting was off. On a fully up-to-date GitLab.com instance the version gate is already satisfied, so this opt-in setting is in practice the more likely tripwire. The "best-effort signing" contract and the no-runtime-probe decision are unchanged; only the framing of what users must enable to actually get a Verified commit is corrected here. The user-facing GitLab integration documentation has been updated to warn about this project/group setting as a prerequisite.

### 2026-05-24: Release tag push no longer delegates to the local `git` binary

The Decision section's claim that `tag`, `push_tag`, and `delete_tag` delegate unchanged to the inner impl, and the Scope section's statement that release tags are created via the local `git` binary, are now functionally incorrect: [ADR-060](060-push-release-tags-via-forge-api.md) moves the release tag-push mechanism into the decorator itself. `tag()` now records the target SHA and message into pending state, `push_tag()` creates the tag via the GitLab Tags API (`POST /projects/:id/repository/tags`, tolerating an "already exists" 400 for idempotency), and `delete_tag()` is a no-op. Only the push mechanism moved; the still-true statement that release tags remain annotated but unsigned (tag-object signing on GitLab stays out of scope) is unchanged.
