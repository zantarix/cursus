# ADR-057: `cursus init` GitLab Support

## Status

Proposed (2026-05-12)

## Context

[ADR-056](056-gitlab-support-client-config-and-ci.md) delivers a working GitLab integration at the library, config, and binary-boundary layers: a `ReqwestGitLabClient`, a `[gitlab]` config section, a `gitlab/remote.rs` URL parser, and environment-variable detection for GitLab CI. That ADR was deliberately scoped to "operational against GitLab when the configuration is hand-edited" — every GitLab user must currently open `.cursus/config.toml` in an editor and fill in `group`, `project`, and (for self-managed instances) `host` before running any release command.

[ADR-019](019-improved-init-workflow.md) established that `cursus init` is the project's primary onboarding surface and should let users get to a working configuration without hand-editing the file. It does this for GitHub today: a Yes/No `EnableGitHub` prompt, an `EditGitHub` screen with the `owner/repo` field pre-populated from `GitHubRepo::parse_url`, and an opinionated default that the `branch` git strategy implies GitHub. None of this exists for GitLab. The result is that the GitLab onboarding path is materially worse than the GitHub one, despite [ADR-056](056-gitlab-support-client-config-and-ci.md) having put every piece needed to fix it in place.

[ADR-020](020-tui-screen-submodule-structure.md) requires every TUI wizard to be structured as a submodule directory with one file per screen. The init wizard currently has eight screens under that pattern. Any new screen added by this ADR must follow the same layout: `pub(super)` handler and renderer in its own file, dispatcher arms in `mod.rs`, and a `#[cfg(test)] mod tests` block colocated with the screen.

[ADR-034](034-compile-time-embedded-localisation.md) requires all user-facing strings to live in compile-time-embedded Fluent message files (`tui.ftl`, `templates.ftl`, etc.) and be referenced by key. Any new prompt copy added by this ADR must be declared as new Fluent keys in those files, never inlined as Rust string literals.

[ADR-056](056-gitlab-support-client-config-and-ci.md) also fixes the user-facing vocabulary rule for GitLab: GitLab surfaces say "project" not "repository", "group" not "owner", "group/project" not "owner/repo", and "merge request" where applicable. The same rule applies to anything `cursus init` displays while the user is configuring GitLab — Fluent key names and prompt copy must reflect GitLab vocabulary so that the wizard does not introduce a GitHub-flavoured dialect into the GitLab path.

A subtle interaction also exists with [ADR-059](059-forge-selection-runtime-rules.md), which introduces a runtime cross-validation rule rejecting configurations where both `[github].enabled` and `[gitlab].enabled` are simultaneously `true`. This ADR's TUI must therefore structurally produce only one enabled forge section at a time; the unchosen forge is written in disabled/template form so it remains discoverable and editable later. That keeps the wizard output trivially compatible with [ADR-059](059-forge-selection-runtime-rules.md) without depending on it.

## References

- [GitLab predefined CI/CD variables](https://docs.gitlab.com/ci/variables/predefined_variables/)

## Decision

`cursus init` shall be extended to first-class GitLab parity by adding a GitLab forge screen to the existing init wizard, replacing the current `EnableGitHub` Yes/No prompt with a three-way forge-choice prompt, and reusing the `gitlab/remote.rs` parser from [ADR-056](056-gitlab-support-client-config-and-ci.md) to auto-detect `group`/`project` (and host, where relevant) from the git origin remote at init time. The user-facing vocabulary at every GitLab-path prompt shall use GitLab native terms.

### Forge-choice prompt replaces the GitHub-only Yes/No

The `EnableGitHub` screen described in [ADR-019](019-improved-init-workflow.md) shall be replaced by a `ChooseForge` screen presenting three options: **GitHub**, **GitLab**, **Neither**. This screen is reached whenever the previous wizard flow would have reached `EnableGitHub` — that is, after `GitStrategy` when the Push strategy was selected, and as the implicit forge prompt when the Branch strategy was selected (Branch still implies a forge; the choice is now which one).

The default selection is **Neither**, preserving the opt-in principle established in [ADR-005](005-github-releases.md). When Branch strategy was selected, the default shifts to **GitHub** to preserve [ADR-019](019-improved-init-workflow.md)'s existing "Branch implies GitHub" behaviour for users who do not actively change it; users on Branch can still pick GitLab or fall back to Neither via this screen.

The wizard runs the chosen forge's editor screen and only that screen. The unchosen forge's config section is written to `.cursus/config.toml` in commented-out template form (matching the `[github]` template treatment for users who chose Neither in [ADR-019](019-improved-init-workflow.md)), so users can later switch forges or enable both for hand-managed configurations without re-running init. Writing only one forge as `enabled = true` keeps the wizard output structurally compatible with the cross-validation rule defined in [ADR-059](059-forge-selection-runtime-rules.md).

### New `EditGitLab` screen

A new `EditGitLab` screen shall be added to the init wizard following the [ADR-020](020-tui-screen-submodule-structure.md) submodule pattern: a single file (e.g. `packages/cursus/src/tui/init/edit_gitlab.rs`) containing a `pub(super)` `handle_edit_gitlab` function, a `pub(super)` `render_edit_gitlab` function, and a colocated `#[cfg(test)] mod tests` block. Dispatcher arms are added in `tui/init/mod.rs`.

The screen mirrors the structural shape of `EditGitHub` but uses GitLab vocabulary throughout:

- A single-line `ratatui-textarea` field labelled "GitLab project (group/project, e.g. `acme/my-app`, or leave empty)". Pre-populated from auto-detection (see below); editable by the user.
- A second, conditional `ratatui-textarea` field for the GitLab **host**, shown only when (a) the auto-detected remote host is something other than `gitlab.com`, or (b) the user explicitly toggles a "self-managed instance" affordance on the screen. Pre-populated from the detected host where applicable. The default empty state is interpreted by the runtime (per [ADR-056](056-gitlab-support-client-config-and-ci.md)) as `https://gitlab.com`.
- A per-package release-artifact mapping section, mirroring the structural pattern already used by the GitHub artifacts portion of init. This ADR does not change the per-package artifact UX shape established by [ADR-044](044-per-package-github-release-artifacts.md); it only re-uses that shape for the GitLab path under `[gitlab.artifacts.<pkg>]`.

Field validation mirrors the GitHub path: an empty `group/project` field is accepted and is written in the config as commented-out hints rather than active values, matching [ADR-019](019-improved-init-workflow.md)'s treatment of an unedited auto-detected `owner/repo`. A non-empty value must parse cleanly via `gitlab/remote.rs`'s segment-validation rules, which is the same path used at runtime — so the wizard refuses values the runtime would later reject.

### Auto-detection reuses `gitlab/remote.rs`

At init time, the wizard reads the `origin` remote URL and runs it through the same parser that `gitlab/remote.rs` exposes for runtime use. This is the unification point: `cursus init` and `cursus prepare` agree on what counts as a valid GitLab remote because they invoke the same parser.

When the parser succeeds:

- The `group/project` field is pre-populated with the parsed value.
- The host field is pre-populated with the detected hostname; the host field is hidden (collapsed into the screen as a non-rendered detail) when the detected host equals `gitlab.com`, and shown when it is anything else.

When the parser fails or the remote is absent — including the case where the user picked GitLab but `origin` actually points to a GitHub host — the wizard does not error. It falls back to the same empty-input behaviour the GitHub path uses today: an empty editable field that the user can fill in, with confirmation of an empty value being valid. This explicitly handles the case of a fresh repository that does not yet have a remote, and the case of a GitHub-mirrored GitLab project (or vice versa) where the user has selected the non-canonical forge intentionally.

The wizard does not consult `CI_API_V4_URL` at init time. That variable is only meaningful inside a GitLab CI job, which `cursus init` is not designed to be run from. `CI_API_V4_URL` remains the binary-boundary detection mechanism described in [ADR-056](056-gitlab-support-client-config-and-ci.md); it is intentionally not duplicated in the init flow.

### Template writer learns the `[gitlab]` section

The init config-template writer (the template approach established by [ADR-019](019-improved-init-workflow.md)) shall be extended to emit a `[gitlab]` section. When GitLab is the chosen forge, the section is written with `enabled = true` and the user-supplied `group`, `project`, and optional `host`; the `build_command`, `merge_request_title`, and `[gitlab.artifacts.<pkg>]` keys are written as commented-out templates with inline GitLab-flavoured help comments. When a different forge is chosen (or Neither), the entire `[gitlab]` section is written as commented-out template so that switching to GitLab later requires only uncommenting and filling in values.

The symmetric treatment also applies to `[github]`: when the user picks GitLab, the `[github]` section is written in commented-out template form. This preserves [ADR-019](019-improved-init-workflow.md)'s "every option is discoverable as a commented-out line" principle and means a user can switch forges by hand-editing without needing to know the schema by heart.

### Non-interactive init remains a hard error

[ADR-019](019-improved-init-workflow.md) made `cursus init --no-interactive` an explicit error. That behaviour is preserved. The new forge-choice prompt is interactive-only; there is no `--forge` flag. Scripts that need a config containing a `[gitlab]` section can write the TOML file directly, exactly as they do today for `[github]`.

The phrase "in `--no-interactive` init the forge-choice prompt is skipped (defaulting to Neither)" from the source brief is therefore not realised as a runtime path — `--no-interactive` init does not run any prompts at all, because it errors out at the wizard entrypoint. The structural property that the writer can emit "no forge enabled, both sections commented out" remains true and is exercised when an interactive user chooses Neither.

### Terminology rule applies to every new locale string

Per [ADR-056](056-gitlab-support-client-config-and-ci.md)'s "neutral abstraction, native vocabulary" rule, every new Fluent key added under [ADR-034](034-compile-time-embedded-localisation.md) for the GitLab init path shall use GitLab vocabulary:

- `tab-gitlab`, `enable-gitlab-question`, `edit-gitlab-question`, `edit-gitlab-invalid-question`, `edit-gitlab-help`, `edit-gitlab-host-question`, and similar keys in `tui.ftl`.
- Template comments for the `[gitlab]` section in `templates.ftl` describing each field in GitLab terms ("GitLab group", "GitLab project", "self-managed GitLab host", "merge request title").
- The new `ChooseForge` screen's prompt strings use neutral language ("Which forge do you want to use for releases?") because at that point in the flow no forge has been chosen yet; the three answer labels are the literal names "GitHub", "GitLab", "Neither".

Existing GitHub-path Fluent keys (`enable-github-question`, `edit-github-question`, etc.) are not renamed. They remain the keys used by the GitHub branch of the wizard.

### Documentation site

The docs site shall gain a `cursus init` walkthrough page for GitLab projects, parallel to the existing GitHub-init walkthrough. This page covers the forge-choice prompt, the `EditGitLab` screen, the auto-detection behaviour (including the self-managed host case), and the generated `[gitlab]` config block. The runtime semantics of the resulting config are owned by [ADR-056](056-gitlab-support-client-config-and-ci.md)'s docs page; this new page deliberately confines itself to what the wizard does.

### What this ADR does not address

- The GitLab client, config schema, environment detection, and asset upload semantics — see [ADR-056](056-gitlab-support-client-config-and-ci.md).
- Verified release commits on GitLab — see [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md).
- Cross-section runtime validation between `[github]` and `[gitlab]` (the "both enabled is an error" rule) — see [ADR-059](059-forge-selection-runtime-rules.md). This ADR's wizard structurally produces only one enabled forge section, so the cross-validation invariant is satisfied for any config init produces. [ADR-059](059-forge-selection-runtime-rules.md) is the safety net for configs that were subsequently hand-edited.

## Consequences

### Positive

- GitLab onboarding via `cursus init` reaches parity with GitHub onboarding. A user running `cursus init` on a fresh GitLab project lands on a fully working `.cursus/config.toml` without hand-editing.
- The `gitlab/remote.rs` parser introduced by [ADR-056](056-gitlab-support-client-config-and-ci.md) is now exercised by two callers (init-time auto-detection and runtime remote parsing), validating the parser against both flows and ensuring they agree on what a valid GitLab remote looks like.
- The forge-choice prompt makes the existence of GitLab support discoverable to users who would not otherwise know to read the documentation. Replacing the GitHub-only Yes/No is the smallest UX change that achieves this.
- Self-managed GitLab instances are handled by surfacing the `host` field only when needed, keeping the common (`gitlab.com`) case a single-screen confirmation while still supporting the self-managed case without forcing the user to back out of init and hand-edit the config.
- The "neutral abstraction, native vocabulary" rule from [ADR-056](056-gitlab-support-client-config-and-ci.md) is reinforced in the init layer: a GitLab user never sees "owner/repo" or "pull request" terminology, only "group/project" and "merge request".
- Writing only one `enabled = true` forge section keeps the wizard output trivially compliant with the cross-validation rule defined in [ADR-059](059-forge-selection-runtime-rules.md), regardless of which forge the user picked.

### Negative

- The init wizard's `Screen` enum grows by at least one variant (`EditGitLab`) and replaces another (`EnableGitHub` becomes `ChooseForge`), increasing the [ADR-020](020-tui-screen-submodule-structure.md) submodule count and the test surface for `handle_key()`. The cross-screen workflow tests in `tui/init/mod.rs` must cover three new paths (GitHub-chosen, GitLab-chosen, Neither-chosen) in addition to the existing flows.
- The Branch-strategy-implies-GitHub default established by [ADR-019](019-improved-init-workflow.md) is preserved by making GitHub the default when Branch was chosen, but users who pick Branch and then actively choose GitLab on the new prompt now traverse one more screen than they used to. This is a small regression in clicks for that specific path, and it is the price of making GitLab equally reachable.
- The conditional host field on the `EditGitLab` screen adds a non-trivial rendering branch (visible vs. hidden) and a tested-empty-vs-detected pre-population path. This is more complex than `EditGitHub`, which has no equivalent self-managed concept.
- Template-writer maintenance now must keep two forge sections (`[github]` and `[gitlab]`) in sync with the live config struct, doubling the surface area covered by the template-vs-struct drift hazard [ADR-019](019-improved-init-workflow.md) already accepted.

### Neutral

- The init flow's other screens (`ConfirmOverwrite`, `PackageManagers`, `ManifestPath`, `EnableGit`, `GitStrategy`, `OpenEditor`) are structurally unchanged. The only flow change is that `EnableGitHub` is replaced by `ChooseForge` and an `EditGitLab` screen joins `EditGitHub` as one of two possible terminal-forge editors.
- The `ratatui-textarea` dependency adopted in [ADR-019](019-improved-init-workflow.md) is reused for the new GitLab fields. No new TUI input library is introduced.
- All new strings are localisable from day one via [ADR-034](034-compile-time-embedded-localisation.md), matching every other init prompt.
- The wizard does not introduce any new config keys; it only writes values that [ADR-056](056-gitlab-support-client-config-and-ci.md) already defined under `[gitlab]`.

## Alternatives Considered

### Single combined forge screen with conditional fields

A single `EditForge` screen with fields whose labels and validation switch based on the chosen forge — `owner/repo` if GitHub, `group/project` if GitLab — was considered. It was rejected because the GitHub screen has stabilised under [ADR-019](019-improved-init-workflow.md) and accumulated forge-specific behaviour (e.g., its auto-detect logic, its commented-hint-on-unedited treatment), and the GitLab screen is expected to accumulate its own forge-specific behaviour over time (the self-managed `host` field is already one example, and CI-token caveats from [ADR-056](056-gitlab-support-client-config-and-ci.md) may produce more). Sharing a single screen forces every future forge-specific addition through a conditional branch in shared rendering and handler code, which compounds in cost. One screen per forge is also friendlier to translators: each forge's prompts can be translated as a self-contained unit without cross-referencing the other forge.

### Always show both forge screens regardless of choice

The wizard could run both `EditGitHub` and `EditGitLab` in sequence and let the user fill in whichever they care about. This was rejected on two grounds. First, it adds friction with no benefit when the user has just answered an unambiguous forge-choice question. Second, it produces a config where both forges are filled in and (without further plumbing) both enabled, which is exactly the invalid state [ADR-059](059-forge-selection-runtime-rules.md) rejects at runtime. Writing only the chosen forge as `enabled = true` and emitting the other as a commented-out template gives users the same later-switchability without producing an invalid intermediate state.
