# ADR-059: Forge Selection Runtime Rules

## Status

Accepted (2026-05-13)

## Context

[ADR-056](056-gitlab-support-client-config-and-ci.md) added a `[gitlab]` config section that sits alongside the existing `[github]` section in `.cursus/config.toml`. Each forge section carries an `enabled: bool` field. [ADR-056](056-gitlab-support-client-config-and-ci.md) deliberately deferred the question of what happens when more than one forge section is enabled in a single config: its scope was "the section parses correctly", and it explicitly named this ADR as the home for cross-section validation.

[ADR-057](057-cursus-init-gitlab-support.md) extended `cursus init` with a three-way forge-choice prompt (GitHub / GitLab / Neither). The wizard structurally produces config files with at most one forge marked `enabled = true`; the unchosen forge is written as a commented-out template. This means the wizard *cannot* generate an ambiguous "more than one enabled" state. However, `.cursus/config.toml` is hand-editable, the schema permits writing `enabled = true` on more than one forge section (today, `[github]` and `[gitlab]`), and users can plausibly land in that state during a forge migration or by copy-pasting examples from documentation. The runtime needs a defined behaviour for that case.

A related question is whether forge selection should ever consult signals other than the explicit `enabled` flag — most plausibly, the hostname of the `origin` git remote. [ADR-056](056-gitlab-support-client-config-and-ci.md)'s Alternatives Considered section already flagged remote-URL auto-detection as a tempting but ambiguous primary signal (forks live on different hosts than canonical projects, mirrors are common in both directions, self-managed GitLab on arbitrary hostnames defeats allowlisting) and noted that this ADR would close out the question.

The runtime question is therefore simple: given a config containing multiple forge sections, which `CodeForgeClient` does the binary construct and inject into `Env`? The `CodeForgeClient` abstraction established by [ADR-041](041-rename-github-client-trait-to-code-forge-client.md) is singular — `Env` holds one `Option<Arc<dyn CodeForgeClient>>` slot, not a collection — so the answer must resolve to at most one client. The CI-managed release workflow defined by [ADR-015](015-ci-managed-release-workflow.md) is the downstream consumer of whichever client gets chosen: `prepare` opens an MR/PR against that forge, `publish` cuts releases on it, and `ci` dispatches accordingly.

This is the final ADR in the four-ADR GitLab batch ([ADR-056](056-gitlab-support-client-config-and-ci.md), [ADR-057](057-cursus-init-gitlab-support.md), [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md), and this ADR). Its scope is intentionally narrow: a single configuration invariant, the error raised when the invariant is violated, and the explicit rejection of remote-URL sniffing as a runtime selection mechanism. It is the safety net for hand-edited configs that [ADR-057](057-cursus-init-gitlab-support.md)'s wizard cannot produce.

## References

- [GitLab predefined CI/CD variables](https://docs.gitlab.com/ci/variables/predefined_variables/)

## Decision

Cursus shall select its active forge exclusively via the `enabled` flag on each forge config section (today: `[github]` and `[gitlab]`). **At most one** forge section may have `enabled = true`. Enabling more than one is a hard configuration error raised at `Config::load`. Enabling none is permitted and causes cursus to skip all forge-mediated operations, matching today's `[github].enabled = false` behaviour. No automatic remote-URL detection or other heuristic shall participate in forge selection at runtime.

### Cross-section validation at config load

`Config::load` shall, after parsing the TOML and populating each forge section's struct, check how many forge sections have `enabled = true`. If more than one is enabled, it shall return a hard error whose user-visible message:

- Names the currently enabled forge sections explicitly so the user can locate them in the file.
- States the rule unambiguously: at most one forge section may have `enabled = true`.
- States the resolution: set `enabled = true` on no more than one forge section, and set the others to `false` (or remove them).

Note the distinction between the *rule statement* and the *runtime error message*. The rule itself is N-agnostic ("at most one forge section may have `enabled = true`") and does not enumerate the specific forges. The error message, on the other hand, dynamically lists whichever sections it actually found enabled — today that will be some subset of `[github]` and `[gitlab]`, but the message construction is driven by the discovered state rather than by a hard-coded pair.

This validation runs as part of the load path. It surfaces before any forge-mediated subcommand attempts to run; it does not wait for command-construction time, and it does not fire lazily on first forge use. The forge invariant is a property of the configuration, not of the active command, and is therefore enforced where configuration validation already lives.

The exact wording and error type are implementation details left to the implementing change; the requirement is that the message names the offending flags and gives an actionable fix.

### Zero forges enabled is a valid configuration

If no forge section has `enabled = true`, cursus shall not construct any `CodeForgeClient`. The `Env`'s `Option<Arc<dyn CodeForgeClient>>` slot remains `None`. Subcommands that require a forge — release creation, MR/PR opening, asset upload — shall be skipped exactly as they are today when `[github].enabled = false` is the only forge-related setting present.

This preserves the established behaviour for projects that want changeset-driven version management and changelog generation without any forge integration at all. It is also the natural intermediate state during a forge migration where a user has disabled the old forge but has not yet enabled the new one.

### No remote-URL auto-detection at runtime

Cursus shall not consult the `origin` remote URL, any other git remote, or any other implicit signal when deciding which forge client to construct. The explicit `enabled` flag is the only input.

This rejection covers all flavours of remote-URL inspection: hostname matching against an allowlist, host-derived parsing à la `gitlab/remote.rs` used as a selection signal, and any "if hostname contains `gitlab` then GitLab else GitHub" shortcut. The `gitlab/remote.rs` and `github/remote.rs` parsers established by [ADR-056](056-gitlab-support-client-config-and-ci.md) and the equivalent older GitHub parser continue to exist and continue to be used for their narrow purposes (init-time auto-detection of `owner/repo` or `group/project`, and runtime parsing of the remote to derive identity once a forge is already chosen). What they do not do, after this ADR, is decide which forge is active.

The rationale is straightforward and worth recording so future readers do not relitigate it:

- **Forks may live on a different host than the canonical project.** A contributor's GitHub fork of a GitLab-canonical repository, or vice versa, would be silently misclassified by hostname inspection.
- **Mirrors are common in both directions.** Projects that mirror a GitHub canonical to a GitLab read-only mirror (or the reverse) would have an `origin` whose host disagrees with the intended publishing target.
- **Self-managed GitLab runs on arbitrary hostnames.** No fixed allowlist can recognise `gitlab.acme-corp.internal` or `code.example.org` as a GitLab instance, and a regex on the hostname would catch unrelated hosts that happen to contain the substring.
- **The cost of being explicit is one line in `.cursus/config.toml`.** That cost is paid once per project, by the user who already knows which forge they intend to use.

The explicit-flag rule survives refactors of CI environments, fork chains, and mirror topology without surprises.

### Consumers consult whichever forge was selected

[ADR-015](015-ci-managed-release-workflow.md)'s CI-managed workflow is the principal consumer of the active forge. Its decision logic — `verify` checks for at least one new changeset against a base ref; `ci` dispatches between `prepare` and `publish` based on repo state — is forge-agnostic in shape and is not altered by this ADR. `prepare` opens its MR or PR against whichever forge is active; `publish` cuts releases against whichever forge is active. The forge-mediated commands derive their target forge from the resolved `Option<Arc<dyn CodeForgeClient>>` in `Env`, which is constructed exactly once at the binary boundary according to the rule above.

[ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md)'s decorator selection (`GitHubSignedCommit` vs `GitLabSignedCommit`) keys off the same resolved choice. There is no second selection mechanism for the signing path.

### Relationship to the init wizard

[ADR-057](057-cursus-init-gitlab-support.md)'s wizard is the primary mechanism that prevents users from reaching a "more than one enabled" state in the first place. The wizard's three-way `ChooseForge` prompt writes only one forge as `enabled = true` and emits the alternative as a commented-out template, so any config produced by `cursus init` is trivially compliant with the invariant defined here.

This ADR is the safety net for the cases the wizard cannot cover: users who hand-edit `.cursus/config.toml`, users mid-forge-migration who enabled the new forge but forgot to disable the old one, and users who copy-paste examples from documentation. The wizard prevents the state; this ADR detects and reports it.

### Per-forge error vocabulary is unchanged

[ADR-056](056-gitlab-support-client-config-and-ci.md) established the "neutral abstraction, native vocabulary" rule: error messages produced *while* a forge is active speak that forge's terminology. This ADR introduces exactly one new error message — the "more than one enabled" one — and that message is forge-neutral by necessity: at the point it fires, no forge has been selected, so no forge-specific vocabulary applies. The message names the offending TOML keys (which today will be drawn from `[github].enabled` and `[gitlab].enabled`) and refers to them as "forge sections" or equivalent neutral phrasing.

### Out of scope

- The GitLab client, config schema, and binary-boundary env detection — owned by [ADR-056](056-gitlab-support-client-config-and-ci.md).
- The init-wizard UX — owned by [ADR-057](057-cursus-init-gitlab-support.md).
- Verified release commits on GitLab — owned by [ADR-058](058-verified-release-commits-on-gitlab-via-web-commits-api.md).
- Multi-forge publish (cutting releases on more than one forge simultaneously). The current decision is firmly "one forge at a time". A future ADR may revisit this if a concrete use case emerges; until then, multi-forge publish is not a supported configuration.
- Migration tooling to move a project from a GitHub-only config to a GitLab-only config (or vice versa). [ADR-057](057-cursus-init-gitlab-support.md)'s commented-out template sections provide the structural support for hand-editing; no automated migration is planned.

## Consequences

### Positive

- The forge-selection rule is explicit, simple to state, and matches how the `CodeForgeClient` abstraction is already shaped (one slot in `Env`). Future contributors can read the rule and the rationale without having to reverse-engineer it from selection-code behaviour.
- Hand-edited configurations that accidentally enable more than one forge fail loudly at load time, before any forge-mediated work begins. The user sees a message that names the offending flags and tells them what to do, rather than getting an MR opened on the wrong forge or a confusing mid-flight error during `publish`.
- Remote-URL auto-detection is closed out as a runtime mechanism, preventing the well-known failure modes around forks, mirrors, and self-managed instances from ever materialising. The decision is recorded so future contributors do not reintroduce it under a different name.
- The zero-forges-enabled-is-fine rule preserves the established behaviour for projects that use cursus purely for changeset-driven version management without any forge integration. No regression for existing `[github].enabled = false` users.
- Forge migrations have a defined intermediate state (no forge enabled, or exactly one enabled) that the configuration loader will accept without complaint. Users are not forced to flip every flag in a single edit.
- The validation runs at every `Config::load` — including read-only ones like `change` that have no forge involvement. This makes it hard for a misconfiguration to slip past review.

### Negative

- A user who genuinely wants cursus to publish to more than one forge from a single run is told no, with the only mitigation being a future ADR that they will need to advocate for. The "one forge at a time" rule is conservative on purpose, but it is a real ceiling.
- Configurations that drift between the wizard and the runtime (someone hand-edits the wizard's output to enable a commented-out forge section *in addition to* the wizard's enabled one) will now fail at load. This is the intended behaviour, but it is a surface where users who expected multiple forges to mean something coherent will be surprised.
- The validation runs at every `Config::load`, which is paid by every subcommand that reads the config — including read-only ones like `change` that have no forge involvement. The cost is a single count of `enabled = true` flags so the runtime impact is negligible, but conceptually the load path is doing forge-shaped work even when no forge is touched.

### Neutral

- No new config keys are introduced. The decision is expressed purely in terms of fields that already exist (`github.enabled`, `gitlab.enabled` from [ADR-056](056-gitlab-support-client-config-and-ci.md)).
- No changes to the `CodeForgeClient` trait or to `Env`'s forge slot. The selection rule is implemented entirely in the binary-boundary construction code and the load-time validation.
- The init wizard's behaviour is unchanged. [ADR-057](057-cursus-init-gitlab-support.md)'s output continues to satisfy this ADR's invariant by construction.
- The `gitlab/remote.rs` and `github/remote.rs` parsers continue to exist and continue to be used for identity parsing; this ADR only constrains what they are *not* used for at runtime (forge selection).

## Alternatives Considered

### Remote-URL auto-detection as the primary selection mechanism

Cursus could inspect the `origin` remote URL, parse the hostname, and use it as the primary signal for which forge client to construct — with the explicit `enabled` flag acting only as an override.

Rejected. The hostname signal is structurally ambiguous in three common real-world configurations: forks that live on a different host than the canonical project, mirrors in either direction (GitHub canonical with GitLab mirror, or vice versa), and self-managed GitLab on arbitrary hostnames that no allowlist can recognise. Any of these would silently misroute a release to the wrong forge — releases cut against a fork rather than the canonical, or against a mirror that is read-only, or against `gitlab.com` when the project actually lives on a self-managed instance. The cost of being explicit is one line in `.cursus/config.toml`, paid once per project; the cost of being wrong is a misrouted release with no obvious error signal. [ADR-056](056-gitlab-support-client-config-and-ci.md) already flagged this alternative as deferred to this ADR; this is where it is closed out.

### Allow multiple forges enabled simultaneously (multi-forge publish)

Permit `enabled = true` on more than one forge section at the same time (today, that would be `github.enabled = true` and `gitlab.enabled = true` together), with the runtime interpreting that as a request to cut parallel releases on every enabled forge: dual release-notes, dual asset uploads, dual MRs/PRs.

Rejected for this ADR. The use case is real in principle (mirrored projects whose maintainers want every mirror to look authoritative), but the implementation surface is substantial: every forge-mediated operation in `prepare` and `publish` would need a "for each active forge" loop with its own per-forge error handling, partial-failure semantics, and idempotency story. None of that exists today, and [ADR-055](055-end-to-end-idempotent-publish-recovery.md)'s idempotency contract is currently single-forge by construction. A future ADR may revisit multi-forge publish if anyone has the concrete use case; the current decision is firmly one forge at a time.

### Precedence rule when more than one forge is enabled (e.g., "if multiple, prefer GitLab")

Define a precedence order so that a multi-enabled state resolves silently to one specific forge rather than erroring.

Rejected. A precedence rule papers over a configuration mistake and produces a hard-to-debug experience: a release goes out on an unexpected forge, the user wonders why, and the silent precedence rule is the only thing standing between them and the right answer. Erroring out is louder, forces the user to make the choice explicit, and removes the question of which forge "wins" from the configuration semantics entirely. There is no precedence rule that is obviously correct (alphabetical? "newer ADR wins"? configuration order?), and any choice would have to be justified in this ADR forever after.
