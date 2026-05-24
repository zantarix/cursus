# ADR-060: Push Release Tags via the Forge API in the Signed-Commit Decorators

## Status

Accepted (2026-05-24)

## Context

[ADR-050](050-verified-release-commits-via-git-data-api.md) and [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md) introduced the `GitHubSignedCommit` and `GitLabSignedCommit` decorators, which route the `ci(release): version packages` commit through each forge's web-commit API to produce Verified commits without any local signing key. Both decorators override `commit()`, `push()`, and `force_push_branch()` and delegate every other `Git` trait method — including `tag()`, `push_tag()`, and `delete_tag()` — to the inner `GitWorkdir`, which shells out to the local `git` binary.

That delegation breaks the release flow on GitLab CI. Running `cursus ci --no-interactive` against a GitLab project produced the verified release commit correctly via the GitLab commits API, but the subsequent tag push failed with a 403 (`You are not allowed to push code to this project`). The cause is that `GitLabSignedCommit::push_tag` delegated to `GitWorkdir`, which runs `git push origin <tag>` over the `origin` remote. In GitLab CI that remote is authenticated with `CI_JOB_TOKEN`, which can read the repository but cannot push code or tags. The 403 then cascaded into a 422 (`Ref is not specified`) when cursus tried to create the GitLab Release for a tag that had never reached the remote.

The verified-commit path already avoids this exact problem for the release commit: it never touches the `origin` push credential, instead asking the forge API to write the commit using the forge token cursus already holds. The release tag is the only remaining release-flow mutation that still depends on the git remote being push-capable. As long as it does, a GitLab CI environment that is otherwise fully capable of producing a verified release must additionally be granted a code-push credential — defeating the point of routing the commit through the API in the first place.

## References

- [GitLab Tags API](https://docs.gitlab.com/api/tags/)
- [GitHub Git Data API — tags](https://docs.github.com/en/rest/git/tags)
- [GitHub Git Data API — references](https://docs.github.com/en/rest/git/refs)

## Decision

We will move the release **tag push mechanism** into the forge API inside each signed-commit decorator, symmetric with the existing `commit()`/`push()`/`force_push_branch()` overrides. Only the push mechanism moves; tag objects remain annotated but unsigned, unchanged from [ADR-050](050-verified-release-commits-via-git-data-api.md) and [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md).

### Tag creation through the forge API

Both decorators will override `tag()`, `push_tag()`, and `delete_tag()` rather than delegating them to the inner `GitWorkdir`:

- `tag()` will no longer run `git tag -a`. It records the target SHA — the current HEAD, which is the release commit already on the remote — together with the annotation message into in-memory pending state. This mirrors how `commit()`/`push()` already defer their work to a later flush.
- `push_tag()` flushes that pending state through the forge API using the same forge token used for the verified commit, so the git remote needs no code-push permission:
  - `GitLabSignedCommit` creates the tag via the GitLab Tags API (`POST /projects/:id/repository/tags`).
  - `GitHubSignedCommit` creates the annotated tag object (`POST /repos/{owner}/{repo}/git/tags`) and then the ref (`POST /repos/{owner}/{repo}/git/refs`).
- `delete_tag()` becomes a no-op. There is no local tag to clean up in the API path, and API tag creation is idempotent on retry.

### Idempotency on re-runs

The release flow must tolerate re-runs over a tag that already exists. Two mechanisms cover this. First, the release workflow checks out with `fetch-depth: 0`, so any existing remote tag is present as a local ref and visible to `tag_exists`. Second, the decorators tolerate an "already exists" response from the forge API: GitLab returns a 400 whose body contains "already exists" (matched against both the string and object error-body shapes), and GitHub returns a 422 "Reference already exists". In both cases the decorator treats the tag as successfully present rather than failing the release.

### Workflow simplification

Eliminating the local `git tag -a` — alongside the already-API-routed commit — means the release flow no longer mutates the local repository through a push-capable remote at all. The release workflow therefore no longer needs a git-identity configuration step or an app-token clone; it can perform a read-only clone with `contents: read`.

### Selection

This behaviour is active only when a signed-commit decorator is installed, i.e. `[git].signed_commits` is `auto` or `force` with a forge token present (per the rules in [ADR-050](050-verified-release-commits-via-git-data-api.md) and [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md)). With `signed_commits = "off"`, the local `git push origin <tag>` path is used and the remote token must be able to push code.

## Consequences

### Positive

- The release tag is pushed using the forge token that already produces the verified commit, so GitLab CI no longer needs a code-push credential on the `origin` remote. The 403/422 cascade is eliminated.
- The release workflow can clone read-only (`contents: read`) and drops its git-identity and app-token-clone steps, shrinking the credential surface of the release job.
- The tag push is now symmetric with the commit push: both decorators route both the release commit and the release tag through the forge API, and neither depends on the git remote's push scope.

### Negative

- The CI release workflow is now coupled to the API-commit path being active. Opting out with `signed_commits = "off"` reintroduces the need for a git identity and a push-capable remote token, so the workflow simplification is conditional on signing being on.
- The GitLab `delete_tag()` no-op means any rollback or cleanup path that relied on tag deletion is now inert on GitLab. This is safe here because API tag creation is idempotent on retry and cursus's recovery flow ([Cursus ADR-055](055-end-to-end-idempotent-publish-recovery.md)) detects and skips an existing release for a tag, but it is an explicit behavioural change worth recording.

### Neutral

- No new dependency is introduced. The GitLab Tags API call uses the existing `AsyncGitlab` client; the GitHub tag/ref calls use the existing `octocrab` Git Data API surface.
- Tag objects remain annotated but unsigned. Only the push mechanism moved; tag-object signing stays out of scope, unchanged from [ADR-050](050-verified-release-commits-via-git-data-api.md) and [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md).

## Alternatives Considered

### Grant the `origin` remote a push-capable token in GitLab CI

Provision a writable token (e.g. `GITLAB_TOKEN` or a project access token with push scope) for the `origin` remote so the existing `git push origin <tag>` delegation succeeds.

Rejected because it reintroduces a long-lived, code-push-capable credential on the remote — exactly the posture the API-routed commit was designed to avoid — and is asymmetric with the commit path, which already goes through the forge API. It would also leave the release workflow needing a privileged clone purely to push the tag.

### Sign the tag object as well

Extend the change to produce a signed tag object, not just route the push through the API.

Out of scope. This change is deliberately the minimal fix for the push-credential problem; tag-object signing carries the same key-custody or separate-API-path questions deferred by [ADR-050](050-verified-release-commits-via-git-data-api.md) and [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md), and keeping tags unsigned keeps the change minimal.

### Route only GitLab through the API and leave GitHub on `git push`

The 403 only manifests on GitLab CI, so the narrowest fix would touch only `GitLabSignedCommit`.

Rejected for consistency. Both decorators already route the commit through the forge API, and leaving GitHub's tag push dependent on the git-remote token's scope would make the two decorators behave differently for no benefit. Routing both removes the dependency on the remote's push scope uniformly.
