# ADR-023: Dependency Propagation Bumps During Prepare

## Status

Proposed

## Context

Chronicle's `prepare` command ([ADR-003](003-release-command.md)) bumps package versions based on pending changesets. In a monorepo, packages frequently depend on one another. When package `A` is bumped, any package `B` that declares a dependency on `A` may need a corresponding version bump to ensure that `B`'s published version reflects the updated dependency.

Currently, Chronicle already performs intra-workspace dependency version propagation: when a package's version is bumped, Chronicle updates the version specifiers in other packages' manifest files that reference the bumped package ([ADR-012](012-workspace-protocol-dependency-updates.md) documents the `workspace:` protocol exception to this). However, updating a dependency specifier in `B`'s manifest without also bumping `B`'s own version creates a problem: the dependency change is invisible to consumers of `B`. If `B` remains at `1.0.0` but now depends on `A@2.0.0` instead of `A@1.0.0`, consumers who have `B@1.0.0` cached or pinned will not know they need to update.

This gap is particularly consequential in two scenarios:

- **Registry publishing.** Most registries (crates.io, npmjs) treat each published version as immutable. If `B@1.0.0` was already published with `A@1.0.0` as a dependency, Chronicle cannot re-publish `B@1.0.0` with an updated dependency on `A@2.0.0`. The version must change.
- **Semantic correctness.** A dependency change is itself a change to the package's contract. Consumers relying on `B`'s transitive dependency tree need to know that `B`'s dependency set has changed, even if `B`'s own source code has not.

The appropriate semver level for a dependency-propagated bump is context-dependent. In some projects, any dependency change warrants a `patch` bump. In others, a major dependency bump (which may introduce breaking transitive changes) should trigger a `minor` or even `major` bump in the dependent. A one-size-fits-all default would not serve all use cases.

## Decision

We will add automatic dependency propagation bumps to the `prepare` pipeline. When a package is bumped during `prepare`, Chronicle will identify all packages within the workspace that declare a dependency on the bumped package and ensure those dependent packages also receive a version bump.

### Propagation mechanism

After the initial per-package version bumps have been computed from changesets (step 4 of [ADR-003](003-release-command.md)), and after any linked-version reconciliation ([ADR-024](024-linked-package-versions.md)), Chronicle will walk the intra-workspace dependency graph and mark additional packages for propagation bumps using a two-phase approach.

**Phase 1: Mark.** Starting from the set of packages that received a version bump (whether from a changeset or linked-version synchronization), traverse the dependency graph and mark all transitive dependents as needing a propagation bump:

1. **Identify dependents.** For each bumped package, find all other packages in the workspace whose manifest files declare a dependency on it.
2. **Mark for propagation.** For each dependent package that has not already been marked at an equal or greater bump level, record it as needing a propagation bump at the configured level. Packages that already have a bump from changesets or linked-version reconciliation at an equal or greater level are skipped.
3. **Propagate transitively.** Newly marked packages are themselves treated as bumped for the purposes of further traversal. Continue until no new packages are marked.

**Phase 2: Sweep.** Apply all recorded bumps (changeset-driven, linked-version, and propagation) in a single pass, computing final versions and writing manifest files.

The intra-workspace dependency graph is directed but not necessarily acyclic. Circular dependencies are valid in some ecosystems, particularly JavaScript, where two packages may depend on each other. The mark phase tolerates cycles because marking a package as "needs a bump" is idempotent: if the traversal revisits a package that has already been marked at the same or higher level, no new mark is added and no further traversal is triggered from that package. This guarantees termination regardless of graph topology.

### Configurable bump level

The semver level used for propagated bumps will be configurable in `.chronicle/config.toml`:

```toml
[prepare]
dependency_bump = "auto"  # "patch" | "minor" | "major" | "match" | "auto"
```

The available options are:

- `"patch"` -- Always bump dependents by `patch`, regardless of the dependency's bump level.
- `"minor"` -- Always bump dependents by `minor`.
- `"major"` -- Always bump dependents by `major`.
- `"match"` -- The propagated bump level matches the dependency's own bump level. If the dependency was bumped `minor`, the dependent is also bumped `minor`. If a dependent has multiple bumped dependencies, the highest level among them wins.
- `"auto"` -- Propagated bump is `patch` unless the dependency's bump was `major`, in which case the dependent is also bumped `major`. Minor bumps in the dependency produce only a `patch` bump in the dependent. This reflects the common expectation that major version changes may introduce breaking transitive changes and should be surfaced to downstream consumers, while minor and patch changes are safe to absorb with a minimal version increment.

The default is `"auto"`. This balances visibility (major breaks propagate loudly) with stability (minor and patch changes do not cascade aggressively through the dependency graph).

This setting applies uniformly to all dependency-propagated bumps. Per-dependency or per-package overrides are not supported in this initial design; they may be added in a future ADR if demand arises.

### Interaction with linked versions

Packages that have already received a version bump due to linked-version reconciliation ([ADR-024](024-linked-package-versions.md)) are exempt from dependency propagation bumps. The linked bump is sufficient: linked packages are guaranteed to share the same version within their group, and applying an additional propagation bump on top would either be redundant (if the linked version is already higher) or would break the linked-version invariant by pushing one package in the group ahead of the others.

This exemption keeps the interaction between the two mechanisms clean. Linked-version reconciliation runs first and produces a definitive version for all packages in a linked group. Dependency propagation then runs on the remaining packages, treating the linked group's version as settled.

### Interaction with explicit changesets

If a package already has pending changesets that result in a bump equal to or greater than the effective propagation level, no additional bump is applied. The changeset-driven bump subsumes the propagation bump. For example, if package `B` has a `minor` changeset and the effective propagation level is `patch`, `B` is already being bumped to a higher level, so propagation adds nothing.

If the effective propagation level is higher than the changeset-driven bump (e.g., a `major` dependency bump produces a `major` propagation under `auto` or `match` mode, but the changeset says `patch`), the propagation level takes precedence and the package receives the higher bump. The changeset's changelog entry is still recorded.

### Interaction with scoped prepare

When `prepare --package` restricts the release scope ([ADR-010](010-scoped-release-changeset-consumption.md)), dependency propagation may identify packages that need a bump but are outside the requested scope. Rather than silently skipping these packages or pulling them into the current release, Chronicle will create a new changeset file in `.chronicle/` recording the pending propagation bump for each out-of-scope dependent package.

The generated changeset will use the effective propagation bump level as the change type (accounting for `auto` and `match` semantics where the level depends on the upstream bump) and include a description indicating the reason (e.g., "Dependency update: `package-a` bumped to 2.0.0"). This ensures the bump is not lost and will be picked up by the next `prepare` run that includes those packages in its scope.

This approach maintains the contract of `--package`: only the specified packages are modified in the current run. The generated changesets integrate cleanly with Chronicle's existing changeset consumption pipeline, requiring no special handling.

### Changelog entries for propagated bumps

Packages whose version is bumped solely by dependency propagation (with no explicit user changeset) will receive a changelog entry documenting the dependency update. This entry will appear under a "Dependencies" category and will list the upstream packages whose version changes triggered the propagation.

When multiple mechanisms contribute to a single package's version bump in the same `prepare` run -- for example, a user changeset provides a `patch` bump, dependency propagation provides another `patch`-level trigger, and linked-version synchronization ([ADR-024](024-linked-package-versions.md)) pulls the version forward -- only one changelog entry per mechanism is generated, and the overall version bump reflects the highest level across all sources. The changelog will contain the user's changeset entry under its normal category and a single "Dependencies" entry summarizing all propagated dependency updates for that package, rather than one entry per upstream dependency change.

## Consequences

### Positive

- Dependent packages are automatically kept in sync with their dependencies, preventing the scenario where a published package has an updated dependency but an unchanged version number.
- The configurable bump level gives projects control over how aggressively dependency changes propagate through the version graph. The `auto` default balances visibility for breaking changes with stability for non-breaking ones, while `match`, `patch`, `minor`, and `major` cover the full spectrum of project needs.
- The scoped-prepare interaction preserves the `--package` contract while ensuring propagation bumps are never silently dropped. Generated changesets integrate with the existing pipeline.
- Changelog entries for propagated bumps provide a clear audit trail of why a package's version changed, even when no user-authored changeset exists.
- Transitive propagation ensures deeply nested dependency chains are fully resolved in a single `prepare` run.
- The linked-version exemption prevents double-bumping and keeps the interaction between linking and propagation predictable: linked groups settle first, then propagation handles everything else.

### Negative

- Dependency propagation can cause cascading version bumps across the workspace. In a densely connected monorepo, a single package change can trigger bumps in many or all packages. This is correct behavior, but may surprise users who expect only the directly changed package to be bumped.
- The generated changesets for out-of-scope packages during scoped prepare accumulate in `.chronicle/` and must be consumed by a subsequent `prepare` run. If scoped releases are used frequently, this can lead to a buildup of auto-generated changeset files that clutter the directory.
- A single global `dependency_bump` level may be too coarse for complex monorepos where different dependency relationships warrant different bump levels. Per-dependency configuration is deferred to a future ADR.
- The `auto` and `match` modes require tracking which bump level each upstream dependency received, adding bookkeeping compared to the simpler fixed-level modes. This is a modest implementation cost but increases the surface area for edge-case behavior that users must understand.

### Neutral

- Dependency propagation runs after changeset aggregation and linked-version reconciliation, and before changelog generation. It is an additive step in the `prepare` pipeline.
- The `dependency_bump` config field uses `serde(default)` and defaults to `"auto"`, so existing configurations without this field continue to work. The `deny_unknown_fields` constraint on config structs is satisfied because the field is added to the struct definition.
- Circular dependencies in the workspace graph do not cause problems. The two-phase mark-then-sweep approach handles cycles through idempotent marking, with no risk of infinite traversal.
- This feature operates on the same intra-workspace dependency information that Chronicle already uses for dependency version specifier updates. No new dependency discovery mechanism is needed.
- The `workspace:` protocol exception ([ADR-012](012-workspace-protocol-dependency-updates.md)) applies to dependency specifier updates, not to propagation bump detection. A package using `workspace:*` to depend on another package will still be identified as a dependent and receive a propagation bump; only the specifier rewrite is skipped.

## Alternatives Considered

### No automatic propagation -- rely on manual changesets

Require users to manually create changesets for dependent packages whenever they change a dependency. This preserves full user control but is error-prone in monorepos with many internal dependencies. Users must remember to create changesets for every transitive dependent, which is tedious and easy to forget. The resulting silent version staleness (a package with updated dependencies but an unchanged version) is a worse outcome than an automatic bump.

### Propagate only for major dependency bumps

Only trigger propagation bumps when the upstream dependency receives a `major` bump, on the theory that `patch` and `minor` changes are backward-compatible and do not require downstream version changes. This was rejected because it conflates semver compatibility with the need for a version bump. Even a `patch` dependency update changes the package's published artifact (its resolved dependency tree), which should be reflected in its version for registry correctness and consumer visibility.

### Use a fixed `patch` bump with no configuration

Always use `patch` as the propagation level, without making it configurable. This simplifies the design but does not accommodate projects where dependency changes are considered more significant. For example, a project where all packages form a tightly coupled framework may want dependency propagation to trigger `minor` bumps to signal to consumers that the dependency set has changed meaningfully. The configuration cost is minimal (one TOML field with a sensible default), and the flexibility it provides justifies the addition.

### Default to `patch` instead of `auto`

Use `patch` as the default propagation level, applying the smallest possible bump uniformly regardless of the upstream change type. This was rejected because it suppresses information about breaking changes. When a dependency receives a `major` bump (indicating a breaking change), consumers of the dependent package should be alerted that their transitive dependency tree has changed in a potentially breaking way. The `auto` default propagates `major` breaks loudly while keeping non-breaking changes minimal, which better matches semver expectations.

### Reject circular dependencies as an error

Treat cycles in the intra-workspace dependency graph as a configuration or project error and refuse to run propagation. This was rejected because circular dependencies are valid in some ecosystems, particularly JavaScript where two packages may legitimately depend on each other (e.g., a plugin and its host). Rejecting cycles would make Chronicle unusable for these projects. The mark-then-sweep algorithm handles cycles naturally through idempotent marking, so there is no correctness reason to prohibit them.
