---
title: GitLab CI
description: Using Cursus with GitLab (gitlab.com and self-managed instances)
---

Cursus supports GitLab as a first-class forge alongside GitHub. The same `prepare`, `publish`, and `ci` subcommands work against GitLab projects with no behaviour changes — only the configuration section and the CI environment differ.

## Enabling GitLab

Add a `[gitlab]` section to your `.cursus/config.toml`:

```toml
[gitlab]
enabled = true
build_command = "cargo make release"
merge_request_title = "chore: release updates"

[gitlab.artifacts.cursus]
"cursus-linux-x86_64" = "target/x86_64-unknown-linux-musl/release/cursus"
"cursus-macos-aarch64" = "target/aarch64-apple-darwin/release/cursus"
```

`group` and `project` are auto-detected from your `origin` git remote. Set them explicitly if your remote differs from the canonical project (mirrors, forks, etc.). See the [`[gitlab]` configuration reference](/cursus/reference/configuration/#gitlab) for the full schema.

## Self-managed GitLab

For self-managed instances, set `host` to your instance's base URL:

```toml
[gitlab]
enabled = true
host = "https://gitlab.example.com"
group = "platform"
project = "release-tooling"
```

Non-standard ports are supported — include the port as part of the host:

```toml
[gitlab]
host = "https://gitlab.example.com:8443"
```

When running under GitLab CI, `CI_API_V4_URL` is set automatically and takes precedence over `host` — this is the most reliable indicator of the correct API base on self-managed instances, especially behind reverse proxies.

## Authentication

GitLab CI exposes two kinds of tokens, and they have **different scopes**:

| Token | Source | Can create releases? | Can create/update merge requests? |
|-------|--------|---------------------|-----------------------------------|
| `GITLAB_TOKEN` | Project- or group-access token, provisioned by you | Yes | **Yes** |
| `CI_JOB_TOKEN` | Predefined; available on every CI job | Yes | **No** |

Cursus prefers `GITLAB_TOKEN` and falls back to `CI_JOB_TOKEN`. Pick the token that matches the operations your CI runs:

- **`publish`** (tags, releases, asset uploads): `CI_JOB_TOKEN` is enough.
- **`prepare`** (creates/updates a release merge request): `GITLAB_TOKEN` is required. Cursus fails fast with a clear error when `prepare` runs with only `CI_JOB_TOKEN` available, before any branch push.
- **`ci`** is a dispatcher: when changesets are pending it routes to `prepare` (requires `GITLAB_TOKEN`); when packages have un-tagged versions it routes to `publish` (`CI_JOB_TOKEN` is enough). The fail-fast fires only when `ci` dispatches to `prepare`.

Provision `GITLAB_TOKEN` as a [project- or group-access token](https://docs.gitlab.com/user/project/settings/project_access_tokens/) with at least the `api` scope, then expose it to CI as a masked, protected variable named `GITLAB_TOKEN`.

## Release artifacts and the Generic Package Registry

GitLab has no equivalent to GitHub's "upload binary directly to a release" endpoint. To attach an artifact to a release, Cursus performs a two-step flow:

1. Upload the file to the project's [Generic Package Registry](https://docs.gitlab.com/user/packages/generic_packages/) (`PUT /projects/:id/packages/generic/release-assets/<version>/<file>`).
2. Attach the resulting URL to the release as a release asset link.

This is transparent — the `[gitlab].artifacts` schema is identical to `[github].artifacts`. Two things to know:

- **Generic Package Registry must be enabled** at the instance and project level. If it is disabled (some self-managed deployments turn it off), asset attachment will fail.
- **Storage cost.** Uploaded artifacts consume Generic Package Registry storage on your GitLab project. On self-managed instances with quotas, this is a real cost the GitHub flow does not impose.

## Verified commits

By default, commits made by the release workflow appear as **Unverified** in the GitLab UI because they are produced by the local `git` binary on the runner. To get the green **Verified** badge on your release commits, Cursus can route the prepare commit through GitLab's commits API; commits created this way are SSH-signed by the instance's web-commits key.

When Cursus detects it is running on GitLab CI (`GITLAB_CI=true`) with a token available, it automatically routes the prepare commit through the GitLab commits API. The default is `signed_commits = "auto"` — no `.cursus/config.toml` change is required. Either `GITLAB_TOKEN` (a project- or group-access PAT with `api` scope) or `CI_JOB_TOKEN` (the default CI token) satisfies the auth requirement.

Requirements:

- **GitLab 18.10 or later.** Older versions accept the API call but do not produce a Verified signature.
- A project- or group-access token, or `CI_JOB_TOKEN`. No long-lived signing key custody is required.
- The token's user identity is what GitLab records as the author and committer; `author_email` / `author_name` are deliberately omitted from the request so GitLab can sign the commit.

To opt out, set `[git].signed_commits = "off"` in `.cursus/config.toml`. To force the API path outside CI (e.g. for local testing against a dev instance), set `[git].signed_commits = "force"`.

## Releases vs draft releases

GitLab has no concept of a "draft" release — releases are created in their final state and become visible immediately. This differs from GitHub, where Cursus creates a draft release, uploads artifacts, then transitions it to published. On the GitLab path:

- The release is visible the moment `create_release` succeeds.
- The internal `publish_release` step is a no-op.
- Cursus's idempotent-recovery behaviour ([ADR-055](https://github.com/zantarix/cursus/blob/main/docs/adr/055-end-to-end-idempotent-publish-recovery.md)) still works — an existing release for a tag is detected and skipped, regardless of forge.

## Example GitLab CI pipeline

```yaml
stages:
  - verify
  - release

verify-changesets:
  stage: verify
  image: rust:latest
  rules:
    - if: $CI_MERGE_REQUEST_IID
  script:
    - cursus verify --no-interactive

release:
  stage: release
  image: rust:latest
  rules:
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
  variables:
    # GITLAB_TOKEN must be provisioned as a masked/protected variable
    # for `prepare` to be able to open the release merge request.
    GITLAB_TOKEN: $RELEASE_GITLAB_TOKEN
  script:
    - cursus ci --no-interactive
```

In this pipeline, the `release` job runs `cursus ci` on every push to the default branch. Cursus auto-detects whether to run `prepare` (when changesets are pending) or `publish` (when packages have versions without matching tags).
