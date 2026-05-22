# ADR-056: GitLab Support — Client, Config, and CI Integration

## Status

Accepted (2026-05-13)

## Context

Cursus today integrates with exactly one code forge: GitHub. Several earlier decisions deliberately set the stage for a second forge:

- [ADR-041](041-rename-github-client-trait-to-code-forge-client.md) renamed `GitHubClient` to `CodeForgeClient` to make the abstraction forge-neutral.
- [ADR-042](042-repo-identity-in-constructor.md) moved repo identity into the client constructor so each implementation carries its own forge-specific identity (owner/repo for GitHub, group/project for GitLab) instead of threading naming conventions through trait method signatures.
- [ADR-038](038-octocrab-github-client.md) established the per-forge client implementation pattern (`OctocrabGitHubClient`) sitting behind the trait, and [ADR-055](055-end-to-end-idempotent-publish-recovery.md) extended the trait with `find_release_by_tag` for idempotent recovery.
- [ADR-028](028-npm-oidc-trusted-publishing.md) and [ADR-045](045-crates-io-trusted-publishing.md) already inspect GitLab CI predefined variables (`CI_JOB_JWT_V2`) for OIDC trusted publishing — the binary boundary therefore already knows how to recognise a GitLab CI environment, even though no actual GitLab API calls are made anywhere in the codebase.

What is still missing is the operational glue to actually talk to GitLab: a client implementation, a config section, environment detection wired into the binary boundary as required by [ADR-030](030-bin-lib-crate-separation.md), and explicit handling of the places where the GitLab API genuinely differs from the GitHub API (release-asset upload, draft semantics, MR-vs-PR token scoping).

GitLab adoption inside Zantarix and from external users hosting on self-managed GitLab instances has been requested repeatedly, and Cursus's CI-managed release workflow ([ADR-015](015-ci-managed-release-workflow.md)) is otherwise platform-agnostic (it relies on environment-variable detection, not a hard-coded `GITHUB_ACTIONS`-only check). The remaining barrier is the forge-API integration itself.

This ADR is the first of a four-ADR batch covering GitLab support. Its scope is deliberately narrow: it makes Cursus operational against GitLab when the configuration is hand-edited. Three follow-up ADRs cover the user-facing surfaces:

- [ADR-057](057-cursus-init-gitlab-support.md) — `cursus init` walkthrough for GitLab projects.
- [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md) — Verified release commits on GitLab (parallel to [ADR-050](050-verified-release-commits-via-git-data-api.md)).
- [ADR-059](059-forge-selection-runtime-rules.md) — Runtime cross-validation rules between `[github]` and `[gitlab]` sections, and forge-selection precedence.

A "config drop" delivery boundary was chosen so that the integration can be exercised end-to-end (and the trait-shape assumptions inherited from the GitHub client can be validated against GitLab's real API) before the UX, signing, and forge-selection layers are designed. Splitting these concerns also keeps each ADR's surface area reviewable.

A note on the upstream Rust crate. The Kitware-maintained `gitlab` crate (`gitlab` on crates.io, `https://docs.rs/gitlab`) is the only mature, actively-maintained, async-capable GitLab client for Rust. It exposes `AsyncGitlab` for tokio-based use and follows a typed-builder pattern similar to octocrab. Its versioning tracks GitLab itself rather than semver — the version observed at the time this ADR is written is `0.1811.0`, released 2026-04-23, with monthly cadence — which has consequences for pinning that are addressed in the Decision section.

## References

- [`gitlab` crate (docs.rs)](https://docs.rs/gitlab/latest/gitlab/)
- [`AsyncGitlab` (docs.rs)](https://docs.rs/gitlab/latest/gitlab/struct.AsyncGitlab.html)
- [GitLab Releases API](https://docs.gitlab.com/api/releases/)
- [GitLab Release Links API](https://docs.gitlab.com/api/releases/links/)
- [GitLab Merge Requests API](https://docs.gitlab.com/api/merge_requests/)
- [GitLab Generic Package Registry](https://docs.gitlab.com/user/packages/generic_packages/)
- [GitLab predefined CI/CD variables](https://docs.gitlab.com/ci/variables/predefined_variables/)
- [GitLab CI_JOB_TOKEN scopes](https://docs.gitlab.com/ci/jobs/ci_job_token/)
- [GitLab REST authentication](https://docs.gitlab.com/api/rest/authentication/)

## Decision

Cursus shall gain a first-class GitLab integration alongside the existing GitHub one. Concretely, the library will add a `ReqwestGitLabClient` (built on the Kitware `gitlab` crate's `AsyncGitlab` HTTP client) that implements the existing `CodeForgeClient` trait without modification, plus a parallel `[gitlab]` configuration section, plus environment detection wired in at the binary boundary. The user-visible vocabulary at every config and error-message surface will use GitLab's native terms; the internal trait, parameter names, and shared types will remain forge-neutral.

### Trait shape stays forge-neutral; user-facing vocabulary is forge-native

The `CodeForgeClient` trait, its parameter names (`head`, `base`, `pull_request`, etc.), and shared types (`ExistingRelease`, etc.) shall not be renamed. The trait abstracts the forge concept; renaming `create_pull_request` to `create_change_request` or similar would force every consumer to learn invented terminology that matches no real forge.

To let orchestration code compose forge-aware log lines without inline `unwrap_or` fallback, the trait gains a single new method, `forge_name(&self) -> &'static str`, that returns the active forge's user-facing label (`"GitHub"`, `"GitLab"`). `Env` exposes a paired accessor, `code_forge_name()`, that auto-captures the active client's name and returns `"forge"` as a neutral fallback when no client is configured. This is the smallest trait-shape change required to keep orchestration log lines vocabulary-correct without re-introducing forge-specific branches into shared code.

In exchange, every user-visible surface shall use the active forge's vocabulary:

- Config keys under `[gitlab]` use `group` (not `owner`), `project` (not `repo`), `merge_request_title` (not `pull_request_title`).
- Error messages, log lines, and (in the follow-up [ADR-057](057-cursus-init-gitlab-support.md)) TUI strings rendered while the GitLab client is active say "merge request", "GitLab project", "group", "group/project".
- The GitLab client implementation is responsible for translating trait parameter names to GitLab API field names (e.g. trait `head` → API `source_branch`, trait `base` → API `target_branch`) at the API boundary. This translation is internal to `ReqwestGitLabClient` and does not leak outward.

This split — neutral at every code abstraction, native at every user boundary — is the rule for any future forge addition.

### New `forge/` module containing `github/` and `gitlab/` siblings

The existing `github/` module shall be relocated under a new `forge/` parent module, and a new `gitlab/` module shall be added alongside it. The final library layout is:

- `forge/mod.rs` — module wiring and public re-exports (`pub use client::CodeForgeClient`, etc.).
- `forge/client.rs` — the `CodeForgeClient` trait definition and shared types (`ExistingRelease`, `PullRequest`).
- `forge/github/` — the relocated GitHub implementation (`mod.rs`, `octocrab_client.rs`, `remote.rs`).
- `forge/gitlab/mod.rs` — module wiring and public re-exports.
- `forge/gitlab/client.rs` — the `ReqwestGitLabClient` implementation of `CodeForgeClient`, constructed with a `GitLabProject { host, group, project }` identity per the constraint set by [ADR-042](042-repo-identity-in-constructor.md). The struct holds an `AsyncGitlab` instance and the project identity for the lifetime of the client.
- `forge/gitlab/remote.rs` — git remote URL parser, mirroring `forge/github/remote.rs` in structure but with GitLab-aware behaviour.

This is the module reorganisation foreshadowed by [ADR-041](041-rename-github-client-trait-to-code-forge-client.md)'s closing note ("a future ADR may reorganise the module structure when a second forge is added"). The flat `github/` directory was the right shape when only one forge existed; once a second forge needs to share trait, types, and a directory neighbourhood, a `forge::{github, gitlab}` parent is the natural home. Existing references to `crate::github::*` paths shall be updated to `crate::forge::github::*` at the same time.

The `Env` struct keeps its single `Result<Arc<dyn CodeForgeClient>, String>` slot; the choice of which concrete client to construct is made at the binary boundary in `cursus-bin/src/main.rs`. [ADR-059](059-forge-selection-runtime-rules.md) will define the precise selection logic. For this ADR, the addition is purely "the GitLab client now exists and can be constructed."

### Remote URL parsing must be host-derived, not host-matched

`forge/gitlab/remote.rs` shall parse three forms in line with `forge/github/remote.rs`:

- HTTPS: `https://gitlab.example.com/group/project.git`
- SCP-style SSH: `git@gitlab.example.com:group/project.git`
- `ssh://` SSH: `ssh://git@gitlab.example.com/group/project.git`

Unlike the GitHub parser, GitLab's parser must not hard-code `gitlab.com` as a sentinel. The hostname must be extracted from the URL itself so that self-managed instances (`gitlab.mycompany.internal`, etc.) are supported without configuration heroics. Explicit-port hosts (e.g. `gitlab.example.com:8443`) shall be preserved verbatim in the parsed host so that downstream API calls hit the correct endpoint on instances that expose GitLab on a non-default port. The parsed segments are exposed as `group` and `project`; subgroup paths (`group/subgroup/project`) shall be supported by treating everything up to the final `/` as the group path. The same unsafe-character validation pattern used in `forge/github/remote.rs` shall be applied to each parsed segment before it leaves the parser.

### New `model/config/gitlab.rs` config section

A new file at `packages/cursus/src/model/config/gitlab.rs` shall define a `GitLab` struct, persisted as `[gitlab]` in `.cursus/config.toml`. It mirrors `github.rs` structurally but uses GitLab vocabulary in every key name:

```toml
[gitlab]
enabled = true
group = "zantarix"
project = "cursus"
host = "https://gitlab.com"          # optional; default empty → gitlab.com
build_command = "cargo make release"
merge_request_title = "Release {date}"

[gitlab.artifacts.cursus]
"cursus-x86_64-linux"   = "target/release/cursus"
"cursus-aarch64-linux"  = "target/release/cursus"
```

Field types:

- `enabled: bool`
- `group: Option<String>` (not `owner`)
- `project: Option<String>` (not `repo`)
- `host: Option<String>` — empty string and absent both resolve to `https://gitlab.com`; any other value is treated as a self-managed base URL.
- `build_command: Option<String>`
- `merge_request_title: Option<String>` (not `pull_request_title`)
- `artifacts: BTreeMap<String, BTreeMap<String, String>>` exposed as `[gitlab.artifacts.<pkg>]` tables, mirroring the per-package layout introduced by [ADR-044](044-per-package-github-release-artifacts.md).

The two config sections (`[github]` and `[gitlab]`) coexist in the schema. Whether having both `enabled = true` simultaneously is an error, or whether one takes precedence, is decided in [ADR-059](059-forge-selection-runtime-rules.md) — this ADR makes no validation rule beyond "the section parses correctly."

To keep callers forge-agnostic, `Config` gains four helpers that resolve a single value from whichever forge is active: `forge_enabled()` (any forge enabled), `release_request_title()` (PR / MR title template), `build_command()` (the active forge's build command), and `forge_artifacts()` (per-package artifact maps). When both forges are configured with `enabled = true`, these helpers prefer the GitHub value — this is the precedence rule for the helpers themselves and is independent of the runtime forge-selection rules being defined in [ADR-059](059-forge-selection-runtime-rules.md).

### Orchestrator rename: `github_releases.rs` → `forge_releases.rs`

The `cli/publish/github_releases.rs` orchestrator and its public symbols (e.g. `orchestrate_github_releases` → `orchestrate_forge_releases`) shall be renamed to drop the `github_` prefix. The orchestrator drives any `CodeForgeClient` impl and contains no GitHub-specific logic; the old name was a vestige of the single-forge era. This rename is internal to the binary and library crates and does not change any user-visible surface.

### Environment detection wired at the binary boundary

Per [ADR-030](030-bin-lib-crate-separation.md), environment detection lives only in the binary crate. As part of this work the binary crate's `main.rs` shall be decomposed into focused submodules so the GitLab-specific resolution path does not balloon a single file:

- `cursus-bin/src/main.rs` — argument parsing, top-level orchestration, exit-code handling.
- `cursus-bin/src/logging.rs` — `CliLogger` setup and verbosity wiring (previously inline in `main.rs`).
- `cursus-bin/src/env_helpers.rs` — small helpers for reading and normalising environment variables.
- `cursus-bin/src/git_setup.rs` — git workdir discovery and the `SignedCommitGit` decorator wrap decision.
- `cursus-bin/src/forge_resolution/{mod,github,gitlab}.rs` — per-forge token + base-URL + identity resolution and `CodeForgeClient` construction. The `mod.rs` selects between forges, the `github.rs` and `gitlab.rs` submodules each own their own environment-variable contracts.

The GitLab-specific resolution shall:

- Detect a GitLab CI environment by reading `GITLAB_CI` and checking that it equals `"true"`.
- Read the auth token from `GITLAB_TOKEN` in preference, falling back to `CI_JOB_TOKEN` if `GITLAB_TOKEN` is absent. This precedence is required because `CI_JOB_TOKEN` has narrower scope than a user-provisioned project- or group-access token (see the authentication caveat below).
- Prefer `CI_API_V4_URL` for the base URL when present; otherwise fall back to the `host` value from the `[gitlab]` config section; otherwise default to `https://gitlab.com`. `CI_API_V4_URL` is provided by GitLab CI on every job and is the most reliable indicator of the correct API base, especially on self-managed instances.

These values flow into the `ReqwestGitLabClient` constructor as part of the `GitLabProject` identity and authenticated client setup. The library itself reads none of these environment variables directly.

### rustls crypto provider unified on `aws-lc-rs`

Octocrab and the Kitware `gitlab` crate both pull in `rustls` transitively. With two HTTP stacks active in the same binary, the previously-implicit choice of `rustls` crypto provider becomes ambiguous: `rustls` refuses to pick a default when more than one provider is compiled in, and panics at first use. To avoid this, the binary shall pin the crypto provider to `aws-lc-rs` (the FIPS-friendly default that already ships with octocrab) and call `aws_lc_rs::default_provider().install_default()` at startup before any HTTP client is constructed. Octocrab's feature flags shall be trimmed so that the `ring` provider is not also compiled in.

### Release-asset upload is a two-step flow, but the trait signature does not change

GitLab does not have GitHub's "upload binary directly to a release" endpoint. To attach an artifact to a GitLab release, two API calls are required:

1. Upload the file to the project's Generic Package Registry: `PUT /projects/:id/packages/generic/<package_name>/<version>/<file_name>`. This returns the package file URL.
2. Attach the resulting URL to the release as a release link: `POST /projects/:id/releases/:tag/assets/links` with the URL, name, and link type.

The `CodeForgeClient::upload_asset` trait signature shall not be changed. The two-step flow is fully internal to `ReqwestGitLabClient::upload_asset`, which performs the generic-package PUT followed by the release-link POST, returning the same `Result` shape that `OctocrabGitHubClient::upload_asset` returns. Callers see a single conceptual "upload" regardless of which forge is active.

### `publish_release` is a no-op on GitLab; `find_release_by_tag` reports `is_draft: false`

GitHub's API distinguishes draft releases from published releases, and Cursus's idempotency story ([ADR-055](055-end-to-end-idempotent-publish-recovery.md)) relies on the `is_draft` flag to decide whether to skip, finalise, or abort. GitLab has no equivalent: releases are created in their final state and there is no "publish a draft" step.

The `CodeForgeClient` trait shall keep both `create_release` and `publish_release`. The GitLab implementation handles them as follows:

- `create_release` calls the Releases POST endpoint and the release is immediately visible.
- `publish_release` is a no-op that returns `Ok(())`. It exists to keep the trait shape uniform.
- `find_release_by_tag` calls the GET-by-tag endpoint and returns `Ok(None)` on 404 (per the `find_release_by_tag` contract from [ADR-055](055-end-to-end-idempotent-publish-recovery.md)) and `Ok(Some(ExistingRelease { is_draft: false, .. }))` on hit. The `is_draft` field is always `false` for GitLab because the concept does not exist there.

The draft-release recovery branch in `orchestrate_forge_releases` (renamed from `orchestrate_github_releases` per the section above) will therefore never trigger when GitLab is the active forge — the "no release exists" and "published release exists" paths cover every GitLab case.

### Authentication caveat — `CI_JOB_TOKEN` cannot create merge requests

`CI_JOB_TOKEN` has read-only access to the Merge Requests API on GitLab as documented in [CI_JOB_TOKEN scopes](https://docs.gitlab.com/ci/jobs/ci_job_token/). It can create releases (the Releases API accepts it), but it cannot create or update MRs.

The implication for Cursus's CI-managed workflow ([ADR-015](015-ci-managed-release-workflow.md)) is:

- The `prepare` flow, which opens a release MR, requires a project- or group-access token with `api` scope provisioned by the project owner and exposed to the CI job as `GITLAB_TOKEN`. `CI_JOB_TOKEN` is insufficient.
- The `publish` flow, which creates tags, releases, and uploads assets, can run with `CI_JOB_TOKEN` alone.

This split is documented in the user-facing GitLab CI integration page (see docs-site requirement below). Cursus shall fail with a clear warning when `prepare` runs with only `CI_JOB_TOKEN` available; the precise wording and detection mechanism is an implementation detail not pinned by this ADR.

### Error-message vocabulary

Every error message surfaced from `ReqwestGitLabClient`, and every log line written while it is the active client, shall use GitLab vocabulary: "merge request", "GitLab project", "group", "group/project". The trait parameter names (`pull_request`, etc.) shall not appear in user-visible output regardless of which client is active. The GitHub client's existing error strings remain unchanged.

### Crate pinning

The `gitlab` crate does not follow semver — its versions track GitLab releases (`0.1811.0` corresponds to GitLab 18.11). The Cargo dependency shall be pinned to a tilde requirement (`~0.1811`) rather than a caret range, because a minor-number bump in this crate is a GitLab-version bump and may contain breaking API changes. Upgrades shall go through Renovate like any other dependency, with a manual review of the GitLab changelog for each bump.

### Documentation site

The docs site's CI-integration section shall be restructured into a nested sidebar group with three pages — Overview, GitHub Actions, and GitLab — so each forge's CI guidance can grow independently. The new GitLab page covers:

- `[gitlab]` config schema with examples.
- Required CI variables (`GITLAB_TOKEN`, fallback to `CI_JOB_TOKEN`, the MR-creation caveat).
- Self-managed instance setup (the `host` key, `CI_API_V4_URL` interplay).
- A note that release artifacts on GitLab go through the Generic Package Registry, so projects with that registry disabled at the instance or project level will not be able to attach assets.

The configuration reference page also gains a `[gitlab]` section mirroring the existing `[github]` documentation.

The `cursus init` walkthrough for GitLab projects is **out of scope** for this ADR and lands in [ADR-057](057-cursus-init-gitlab-support.md).

### What this ADR does not address

- The `cursus init` TUI / interactive wizard for GitLab — see [ADR-057](057-cursus-init-gitlab-support.md).
- Cross-section runtime validation between `[github]` and `[gitlab]` (e.g. "both enabled" rules, autodetection from the git remote) — see [ADR-059](059-forge-selection-runtime-rules.md).
- Verified release commits on GitLab (parallel to [ADR-050](050-verified-release-commits-via-git-data-api.md)) — see [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md).

## Consequences

### Positive

- Cursus becomes operational against both GitHub and GitLab — including self-managed GitLab — once the user populates the `[gitlab]` section. The blast radius of "we support one forge" is removed.
- The forge-neutrality of `CodeForgeClient` is validated by a second real implementation. Any leakage of GitHub-specific assumptions through the trait (parameter names, error shapes, the implicit existence of drafts) is surfaced and dealt with at the implementation boundary rather than infecting the trait itself.
- The "neutral abstraction, native vocabulary" rule is established as a precedent for future forge additions. A hypothetical Gitea or Forgejo client follows the same pattern without further ADR-level debate.
- The two-step GitLab asset-upload flow is hidden behind the same `upload_asset` trait method users already know, so the publish stage's orchestration code is unchanged.
- The new `forge_enabled` / `release_request_title` / `build_command` / `forge_artifacts` helpers on `Config` let orchestration code stay forge-agnostic without per-call-site branching, and the `forge_name()` trait method removes the previous need for inline `unwrap_or("forge")` fallbacks in log lines.

### Negative

- A non-semver dependency enters the crate graph. The `gitlab` crate's versioning policy means we must review the GitLab changelog on every Renovate bump rather than trusting a caret range. This is operational overhead.
- Cursus now ships GitLab API surface that is exercised only when a user activates the `[gitlab]` section. Cargo-feature-gating was considered (see Alternatives) and declined; the dependency weight (a tokio-async HTTP client) is paid by every build.
- The MR-creation token caveat is a sharp edge. Users running `prepare` from GitLab CI for the first time will hit it. The mitigation is documentation plus a clear error message, but neither replaces the experience of a forge where the default CI token can do everything.
- Asset uploads consume Generic Package Registry storage on the user's GitLab project. On self-managed instances with quotas, this is a real cost the GitHub flow does not impose. The docs must call this out.
- Two `rustls` crypto providers are now reachable transitively, so the binary must explicitly `install_default()` on `aws-lc-rs` at startup. Forgetting this would cause a runtime panic on first HTTPS call; it is a one-line invariant but a new one to maintain.

### Neutral

- The `[gitlab]` config section is purely additive. Existing `[github]`-configured projects are unaffected by parsing changes.
- The `CodeForgeClient` trait gains exactly one new method (`forge_name()`); all other methods, parameter names, and shared types are unchanged. Existing test doubles, the `OctocrabGitHubClient`, and orchestration code continue to compile against the trait with a single one-line addition per implementation.
- `publish_release` becoming a no-op on the GitLab path is a contract-level surprise but a behaviour-level non-event: the GitHub path is unchanged and the GitLab path needs no second call.
- Token precedence (`GITLAB_TOKEN` → `CI_JOB_TOKEN`) is consistent with GitLab community convention; users coming from other GitLab-aware Rust tools will recognise it.
- The `cli/publish/github_releases.rs` orchestrator is renamed to `cli/publish/forge_releases.rs` (and `orchestrate_github_releases` to `orchestrate_forge_releases`). The rename is internal; no user-facing surface changes.
- The `github/` module is relocated to `forge::github`. All in-tree `crate::github::*` references shift to `crate::forge::github::*`; downstream callers (the `cursus-bot` project) update their import paths once.

## Alternatives Considered

### Unified `[forge]` config section with a `kind` discriminator

A single `[forge]` table with `kind = "github" | "gitlab"` and shared keys (`host`, `owner_or_group`, `repo_or_project`, etc.) was considered. It was rejected for three reasons. First, it breaks every existing `.cursus/config.toml`, which would force a migration ADR and a tool to run before any GitLab user could benefit. Second, sharing keys requires either neutral key names (`owner_or_group`) which read awkwardly in both contexts, or a renaming step at parse time which trades clarity for cleverness. Third, the existing `[github]` section is precedent — a parallel `[gitlab]` section matches the established schema pattern, including [ADR-044](044-per-package-github-release-artifacts.md)'s per-package artifact layout.

### Auto-detect the forge from the git remote URL

Cursus could read the origin remote, parse the hostname, and decide which forge to use without any config flag. This was tempting because it removes a config step, but it was rejected as the *primary* selection mechanism. The hostname heuristic is ambiguous in real environments: forks may live on different hosts than the canonical project, mirrors are common (a GitHub-canonical repo with a GitLab mirror, or vice versa), and self-managed GitLab instances on arbitrary hostnames defeat any allowlist-style detection. Explicit config under `[gitlab]` or `[github]` is unambiguous and survives refactors of CI environments. ([ADR-059](059-forge-selection-runtime-rules.md) defines the final selection logic and closes out remote-URL inspection as a runtime selection mechanism in any form — flagging this here so future readers understand the placement.)

### Hand-rolled `reqwest`-based GitLab client instead of the `gitlab` crate

A bespoke client built directly on `reqwest` was considered. It would have zero external Rust dependencies beyond what we already use for HTTP, and would give us full control over types. It was rejected because the Kitware `gitlab` crate is actively maintained (monthly releases tracking GitLab versions, most recently `0.1811.0` on 2026-04-23), provides `AsyncGitlab` tokio-compatible out of the box, uses a typed-builder pattern consistent with octocrab, and removes the burden of keeping handcrafted GitLab API types in sync with a moving target. The non-semver versioning is real friction (see Pinning above), but it is strictly less friction than maintaining the types ourselves.

### Cargo feature-flag the GitLab integration

A `gitlab` Cargo feature could gate compilation of the `forge::gitlab` module and its dependencies so that users who only use GitHub do not pay the compile-time cost. This was rejected for now because the binary is shipped as prebuilt static artifacts ([ADR-022](022-distribution-strategy.md), [ADR-054](054-cargo-binstall-support.md)) — end users do not compile Cursus locally, so the compile-cost argument applies only to the project's own CI and Renovate runs. The added complexity of conditional compilation across `mod.rs`, the `Env` constructor, the binary's main, and the test-support feature was judged not worth that marginal saving. If binary size becomes a concern (the `gitlab` crate's transitive dependencies are not trivial), this decision can be revisited in a follow-up ADR.

### Reuse `octocrab` against a GitLab-compatible shim

A handful of GitLab-compatibility shims for octocrab-shaped clients exist. They were rejected on inspection: none is maintained, all target a subset of the GitHub REST surface that does not include the endpoints Cursus needs (Releases, Merge Requests, Generic Packages), and the impedance mismatch between GitHub's and GitLab's actual API shapes (drafts, two-step asset upload, MR vs PR semantics) is large enough that a shim would hide more bugs than it solves.

## Errata

### 2026-05-13: Both-enabled state is now rejected at load time

The Decision and Out of Scope sections defer the cross-section "both forges enabled" question, and the GitHub-first precedence described for the four forge-resolving helpers (`forge_enabled`, `release_request_title`, `build_command`, `forge_artifacts`) is presented as a runtime-reachable behaviour. Both framings are now incorrect: [ADR-059](059-forge-selection-runtime-rules.md) closes the deferred question by requiring that at most one forge section have `enabled = true`, enforced at `Config::load`. The helpers still contain the precedence branches as a defensive fallback, but the GitHub-first arm cannot be exercised under a valid config.

### 2026-05-22: `SignedCommitGit` renamed and a GitLab decorator now lives alongside it

The binary-crate decomposition bullet for `cursus-bin/src/git_setup.rs` describes it as owning "the `SignedCommitGit` decorator wrap decision." That name is now incorrect: [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md) renames the GitHub decorator to `GitHubSignedCommit` and adds a sibling `GitLabSignedCommit` decorator. `cursus-bin/src/git_setup.rs` now owns the wrap decision for *both* decorators, selecting between them based on which forge section is active in config, with the `[git].signed_commits` enum from [ADR-050](050-verified-release-commits-via-git-data-api.md) controlling whether either is installed. The decomposition itself is unchanged in shape; only the named decorator (and the fact that there are now two) is.
