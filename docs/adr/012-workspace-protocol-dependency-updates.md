# ADR-012: Skip workspace: Protocol Dependencies During Intra-Workspace Version Propagation

## Status

Accepted

## Context

Chronicle's `release` command performs automatic intra-workspace dependency version propagation. When a package's version is bumped during release, Chronicle scans all other packages in the workspace and updates any dependency references to that package in their manifest files. For npm adapters, this means rewriting version strings in the `dependencies`, `devDependencies`, `peerDependencies`, and `optionaDependencies` sections of `package.json`.

The JavaScript ecosystem includes a `workspace:` protocol for referencing packages within the same monorepo. Examples include `"workspace:*"`, `"workspace:^"`, `"workspace:~"`, and `"workspace:^1.0.0"`. This protocol is not standardized across package managers:

- **npm** does not support the `workspace:` protocol at all. Using it produces an `EUNSUPPORTEDPROTOCOL` error.
- **pnpm** supports `workspace:` with auto-resolution at publish time. `workspace:*` resolves to the exact local version, `workspace:^` resolves to a caret range, and `workspace:~` resolves to a tilde range. Fixed versions like `workspace:^1.0.0` are also valid but uncommon.
- **yarn Berry (v2+)** supports `workspace:` with similar but not identical semantics to pnpm.

The most common usage pattern is `workspace:*` or `workspace:^`, both of which auto-resolve to the correct version at publish time. These entries do not contain a concrete semver string that Chronicle could meaningfully update. Attempting to parse and rewrite `workspace:` values would require understanding each package manager's resolution semantics, and the result would likely be overridden at publish time anyway.

## Decision

We will skip dependency entries that use the `workspace:` protocol during intra-workspace version propagation. When Chronicle encounters a dependency value starting with `workspace:`, it will print a warning to stderr identifying the dependency and manifest file, then continue processing without modifying that entry.

Concretely, in the npm adapter's `update_dependency_version` method, any dependency value where `current_value.starts_with("workspace:")` is true will be skipped. Regular semver ranges (`"^1.0.0"`, `"~2.3.0"`, `"1.0.0"`) continue to be updated as before.

The warning is intentionally informational, not an error. The `workspace:` protocol entries are valid and expected in pnpm/yarn Berry workspaces. The warning ensures users are aware that Chronicle did not modify those entries, without disrupting the release workflow.

Future work may add explicit `workspace:` protocol support if a clear need emerges. This could involve resolving `workspace:^1.0.0` to `workspace:^2.0.0` for fixed-version variants, or providing a configuration option to control the behavior. The current approach is safe because skipping these entries preserves the user's intent and avoids incorrect rewrites.

## Consequences

### Positive

- Avoids incorrect manifest modifications. Rewriting `workspace:*` to a concrete version would break the local resolution behavior that pnpm and yarn Berry users depend on.
- No package-manager-specific logic is needed. Chronicle does not need to understand the differences between pnpm's and yarn Berry's `workspace:` semantics.
- The warning provides visibility. Users are informed when entries are skipped, preventing confusion about why certain dependency versions appear unchanged after release.
- The most common `workspace:` usage patterns (`workspace:*`, `workspace:^`) are unaffected by the skip because they auto-resolve to the correct version at publish time without any manual intervention.

### Negative

- Users with fixed-version `workspace:` entries (e.g., `"workspace:^1.0.0"`) will not have those entries updated automatically. They must update these manually or rely on their package manager's resolution behavior. In practice, fixed-version `workspace:` entries are rare because `workspace:*` and `workspace:^` cover the typical use case without hardcoding a version.
- The warning may appear noisy in workspaces with many cross-references using the `workspace:` protocol. Each skipped entry produces a separate warning line on stderr.

### Neutral

- npm users are unaffected. npm does not support `workspace:` protocol, so their manifests will not contain such entries.
- Cargo workspaces are unaffected. The `workspace:` protocol is specific to the JavaScript ecosystem. Cargo's `workspace = true` dependency syntax is handled separately by the Cargo adapter.
- This decision does not preclude adding `workspace:` protocol support in the future. The skip-and-warn approach is a safe default that can be replaced with more sophisticated handling if demand arises.

## Alternatives Considered

### Resolve workspace: protocol values based on package manager semantics

Chronicle could detect whether the project uses pnpm or yarn Berry and apply the appropriate resolution rules when updating `workspace:` entries. For example, `workspace:^1.0.0` could be rewritten to `workspace:^2.0.0` when the referenced package is bumped to 2.0.0, while `workspace:*` could be left unchanged since it auto-resolves.

This was rejected because it introduces package-manager-specific branching into a code path that currently handles all JavaScript package managers uniformly. The resolution semantics differ between pnpm and yarn Berry, and tracking those differences across versions adds maintenance burden. The benefit is marginal since the fixed-version `workspace:` pattern is uncommon in practice, and the auto-resolving variants (`*`, `^`, `~`) do not need updating.

### Strip the workspace: prefix and update the version

Chronicle could treat `workspace:^1.0.0` as equivalent to `^1.0.0`, update the version portion, and re-add the `workspace:` prefix. This would produce `workspace:^2.0.0` without needing to understand package-manager-specific semantics.

This was rejected because it assumes that all `workspace:` values follow the `workspace:<range><version>` format. In reality, `workspace:*`, `workspace:^`, and `workspace:~` do not contain a version number at all. A prefix-stripping approach would need to distinguish between these forms and the fixed-version form, adding complexity for an edge case. Additionally, the correctness of rewriting `workspace:^1.0.0` to `workspace:^2.0.0` is not guaranteed across all package managers, since the `workspace:` protocol is not standardized.

### Treat workspace: entries as errors

Chronicle could fail the release when it encounters `workspace:` protocol dependencies, forcing users to remove them or switch to regular semver ranges before releasing.

This was rejected because `workspace:` protocol entries are a legitimate and widely-used feature of pnpm and yarn Berry workspaces. Failing on valid project configurations would make Chronicle unusable for a significant portion of the JavaScript monorepo ecosystem. The `workspace:` protocol entries do not prevent a correct release; they simply do not need Chronicle's intervention.
