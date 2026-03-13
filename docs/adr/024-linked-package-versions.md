# ADR-024: Linked Package Versions in Monorepos

## Status

Proposed

## Context

Chronicle's `prepare` command ([ADR-003](003-release-command.md)) bumps each package independently based on its pending changesets. In a monorepo with packages `A`, `B`, and `C`, a changeset touching only `A` results in only `A` receiving a version bump. Packages `B` and `C` remain at their prior versions.

This independent versioning model is correct for many monorepos, but a significant class of projects requires that some or all packages move in lockstep. Common scenarios include:

- **Framework ecosystems** where a set of packages form a cohesive product and consumers expect all packages at the same version. Deploying `@acme/core@3.1.0` alongside `@acme/utils@3.0.4` creates confusion even if the packages are technically compatible.
- **Platform libraries** where packages are always installed together and version mismatches are either unsupported or a source of subtle bugs.
- **Gradual adoption** where a monorepo contains both tightly coupled packages (which should be linked) and loosely coupled utilities (which should version independently). A global "all packages share one version" policy is too coarse; the user needs to define groups.

Tools like Changesets and Lerna support version linking, and users migrating from those tools expect comparable functionality. Without linked versioning, Chronicle cannot serve the lockstep-versioning use case at all -- there is no workaround short of manually editing versions after every `prepare` run.

A further practical concern is recovery from desync. When a project first enables linked versions, or when manual edits or merge conflicts cause versions to diverge within a linked group, the linking algorithm needs to converge the group to a single version without requiring manual intervention.

## Decision

We will add a `[linked-versions]` configuration section to `.chronicle/config.toml` that allows users to declare groups of packages whose versions must stay in sync.

### Configuration

The configuration supports two modes, both using the same underlying mechanism.

**Global linking** (all packages share one version):

```toml
[linked-versions]
enabled = true
```

When `enabled = true` and no `groups` are defined, all packages enumerated by the configured package managers are treated as a single linked group.

**Group-based linking** (user-defined subsets):

```toml
[[linked-versions.groups]]
packages = ["@org/prefix-*", "@org/other"]

[[linked-versions.groups]]
packages = ["@org/another-group-*"]
```

Each group is an array of package name patterns. Patterns support glob-style matching (e.g., `@org/prefix-*` matches `@org/prefix-a` and `@org/prefix-b`). A package may appear in at most one linked group. Packages that do not match any group pattern remain independently versioned.

When `groups` are defined, the `enabled` field is not required and defaults to `true`. Setting `enabled = false` disables all linked versioning regardless of whether groups are defined, providing a quick toggle for experimentation or debugging without removing the group configuration.

Global linking (`enabled = true` without `groups`) is a convenience shorthand equivalent to defining a single group whose pattern matches all packages. The implementation will treat it identically.

### Version calculation algorithm

During `prepare`, after changesets have been aggregated and independent version bumps have been computed per [ADR-003](003-release-command.md), a linked-version reconciliation step runs for each linked group:

1. **Bump normally.** For every package in the group that has pending changesets, compute its next version using the standard semver bump rules (highest change type wins across all changesets for that package).
2. **Find the maximum.** Across all packages in the group -- both those that were bumped in step 1 and those with no pending changesets -- find the highest version. This comparison uses standard semver ordering.
3. **Apply the maximum.** Set every package in the group to that highest version. Packages that already had this version (or were bumped to it in step 1) are unchanged. Packages that had a lower version are raised to the maximum.

This "max version wins" algorithm has two important properties:

- **Convergence.** If versions within a group have drifted apart (due to initial migration, manual edits, or partial failures), a single `prepare` run brings them all to the same version. No iterative correction or manual alignment is needed.
- **Monotonicity.** No package's version ever decreases. A package without changesets may be pulled forward to match the group maximum, but it will never be pushed backward. This satisfies the semver requirement that published versions are immutable and monotonically increasing.

Packages raised to the group maximum without having their own changesets will still receive a version bump in their manifest file and a lock file update if applicable. These packages will receive a changelog entry noting the version synchronization, ensuring the version change is documented. When multiple internal mechanisms (linking and dependency propagation per [ADR-023](023-dependency-propagation-bumps.md)) both contribute to a package's version bump, only one changelog entry is generated per package, summarizing the combined effect.

### Interaction with scoped prepare

When `prepare --package` is used to scope a release ([ADR-010](010-scoped-release-changeset-consumption.md)), Chronicle will refuse to run if the `--package` scope partially overlaps with a linked group. If any package in a linked group is included in the scope but other packages in the same group are excluded, `prepare` will exit with an error identifying the linked group and the missing packages. The user must either include all packages in the linked group in their `--package` scope, or exclude all of them.

This strict enforcement prevents a scoped prepare from producing a desynced linked group, which would violate the invariant that linked packages always share the same version. Allowing partial overlap would silently break the linking guarantee and confuse users who configured linked versions precisely to avoid version divergence.

Packages in a linked group that have no pending changesets but are included in the `--package` scope (because the full group is in scope) will still participate in the max-version reconciliation as described above.

### Validation

Chronicle will validate the linked-versions configuration during `prepare` and `ci` commands. Validation does not run during `chronicle change`, which is a lightweight command unaffected by linked-version semantics.

- A package matching more than one group pattern is a configuration error.
- A pattern that matches no enumerated packages produces a warning (not an error), since packages may be added later.
- An empty `packages` array in a group is a configuration error.

Running validation during `ci` ensures that users on the branch strategy ([ADR-015](015-ci-managed-release-workflow.md)) receive fast feedback on every push to main, since `chronicle ci` runs automatically in their CI pipeline. Users on the push strategy who run `prepare` directly receive the error immediately before their next release.

## Consequences

### Positive

- Monorepos that require lockstep versioning can use Chronicle without post-hoc version manipulation. This closes a significant functionality gap compared to Changesets and Lerna.
- The glob pattern support allows groups to be defined by naming convention (e.g., `@org/sdk-*`), which scales naturally as packages are added or removed without requiring config updates.
- The max-version-wins algorithm is self-healing: it converges diverged versions in a single run, making migration to linked versions and recovery from desync straightforward.
- Global linking provides a zero-configuration experience for the common case where all packages should share one version.
- The `enabled` toggle allows users to temporarily disable linking without removing their group definitions.

### Negative

- Packages without pending changesets can receive version bumps purely due to linking. This may be surprising to users who expect a version bump to always correspond to a code change.
- The strict scoped-prepare validation means users cannot release a subset of a linked group independently. A `--package` scope that partially overlaps a linked group is rejected outright. Users who need to release individual packages from a linked group must either remove the linking configuration or include the full group in their scope.
- Glob pattern matching adds a dependency on pattern-matching logic that must be consistent across platforms. Edge cases in glob semantics (e.g., whether `*` matches path separators in scoped npm package names) require careful specification and testing.
- The `deny_unknown_fields` constraint on config structs means that adding `[linked-versions]` is backward-compatible (old configs without it work fine via `serde(default)`), but users on older Chronicle versions who encounter a config with this section will get a parse error.

### Neutral

- The linked-version reconciliation step runs after the existing per-package bump logic and before changelog generation. It is an additive step in the `prepare` pipeline, not a replacement for existing logic.
- Changeset files themselves are unaffected by this feature. Changesets continue to record per-package change types. The linking is applied at `prepare` time, not at `change` time.
- This decision does not affect `publish` ordering or behavior. Publishing remains per-package and respects dependency order as before.
- `chronicle change` does not validate linked-version configuration. Invalid group patterns or overlapping memberships are only surfaced when `prepare` or `ci` runs. This keeps `change` lightweight and avoids requiring package enumeration during changeset recording.

## Alternatives Considered

### Synchronized change types instead of max version

Instead of finding the maximum version across the group, apply the highest change type from any package's changesets to all packages in the group. For example, if `A` has a `minor` changeset and `B` has no changesets, bump both `A` and `B` by `minor` from their respective current versions.

This was rejected because it does not converge diverged versions. If `A` is at `2.1.0` and `B` is at `2.0.0` (due to a prior desync), a `minor` bump produces `A@2.2.0` and `B@2.1.0` -- still desynced. The max-version approach produces both at `2.2.0`. Additionally, the change-type propagation approach requires deciding what change type to assign to packages with no changesets, which is semantically awkward: a `minor` bump to `B` when nothing changed in `B` misrepresents the nature of the version increment.

### Lockfile-style fixed versions

Maintain a separate file (e.g., `.chronicle/linked-versions.toml`) that records the current canonical version for each group. The `prepare` command would read this file, bump the canonical version, and apply it to all packages in the group.

This was rejected because it introduces a new source of truth for versions that can conflict with the actual versions in manifest files. Chronicle's philosophy is to read versions from their native locations (`Cargo.toml`, `package.json`) rather than maintaining shadow state. A canonical version file would need reconciliation logic for cases where manifest versions and the canonical version disagree, adding complexity without clear benefit over the max-version approach.

### Implicit linking via workspace dependencies

Infer linked groups from dependency relationships: packages that depend on each other within the workspace are automatically linked. This was rejected because dependency relationships and version-linking requirements are orthogonal concerns. A utility package depended on by every other package does not necessarily need to share their version. Conversely, packages in a linked group may have no direct dependency relationship. Explicit configuration is clearer and more predictable than inference from dependency graphs.

### Allow partial-overlap scoped prepare with intersection semantics

When `prepare --package` partially overlaps a linked group, apply linking reconciliation only within the intersection of the scope and the group, leaving out-of-scope packages in the group untouched. This was rejected because it silently breaks the core invariant of linked versioning: that all packages in a group share the same version. A scoped prepare that bumps `@org/prefix-a` to `2.1.0` while leaving `@org/prefix-b` at `2.0.0` produces exactly the desync that linked versions are designed to prevent. Users who configured linked versions would reasonably expect the guarantee to hold after every `prepare` run, and a partial update would violate that expectation without any warning. An explicit error with guidance to include the full group is clearer and safer.
