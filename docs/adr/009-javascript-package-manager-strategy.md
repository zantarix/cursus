# ADR-009: JavaScript Package Manager Strategy for Lockfiles and Publishing

## Status

Accepted

## Context

Cursus supports the JavaScript ecosystem through a single `NpmAdapter` that handles npm workspaces, yarn workspaces, and pnpm workspaces. Two operations within this adapter interact directly with the user's chosen package manager tooling: updating lock files after version bumps, and publishing packages to a registry.

The JavaScript ecosystem has a proliferation of package managers. The established ones -- npm, yarn (Classic v1 and Berry v2+), and pnpm -- all produce their own lockfile formats (`package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`). Newer entrants like Bun (`bun.lock`) and Deno have their own conventions as well. Each manager has subtly different CLI flags for lockfile-only updates.

However, the publishing story is simpler. The npm registry is the dominant package registry for JavaScript. Most package managers delegate to `npm publish` semantics under the hood:

- **npm**: Publishes directly via `npm publish`.
- **pnpm**: `pnpm publish` is a thin wrapper around `npm publish`.
- **yarn Classic (v1)**: `yarn publish` delegates to the npm registry using the same protocol.
- **yarn Berry (v2+)**: Introduces its own CLI syntax and plugin system for publishing, but still targets the npm registry by default.

The key question is how much Cursus should invest in detecting and supporting each package manager for each operation, versus providing a pragmatic default with an escape hatch.

## Decision

We will use a two-tier strategy for JavaScript package manager interactions:

**Lockfile updates: autodetect with configurable override.**

The `NpmAdapter::update_lock_file()` method will autodetect the package manager by checking which lockfile exists in the project root, in this order:

1. `package-lock.json` -- run `npm install --package-lock-only`
2. `pnpm-lock.yaml` -- run `pnpm install --lockfile-only`
3. `yarn.lock` -- run `yarn install --mode update-lockfile`
4. No lockfile found -- no-op (silently succeed)

Users who use a package manager that Cursus does not autodetect (e.g., Bun, Deno, or a future tool) can set the `lock_command` option in their `[npm]` configuration section to specify an arbitrary shell command:

```toml
[npm]
enabled = true
lock_command = "bun install --frozen-lockfile"
```

When `lock_command` is set, it takes unconditional precedence over autodetection. The command is split on whitespace and executed directly (not through a shell).

**Publishing: npm-only.**

The `NpmAdapter::publish()` method will always invoke `npm publish`, regardless of which package manager the user uses for development. This is a deliberate simplification based on the observation that:

- pnpm and yarn Classic pass through to npm for publishing; running `npm publish` directly produces the same result.
- yarn Berry has its own publishing syntax, but still targets the npm registry. Users of yarn Berry can publish via `npm publish` without issue since the operation reads `package.json` directly, not yarn-specific configuration.
- Supporting multiple publish commands would require detecting the package manager (duplicating the lockfile detection logic), handling divergent CLI flags, and parsing different error output formats for idempotency checks (per [ADR-004](004-publish-command.md)). The added complexity is not justified when `npm publish` works universally for the npm registry.

For scoped packages (`@scope/name`), Cursus will pass `--access <level>` using the configurable `access` field in `NpmConfig`, defaulting to `restricted` if unset.

## Consequences

### Positive

- Lockfile autodetection means zero configuration for projects using npm, pnpm, or yarn. Cursus just works.
- The `lock_command` escape hatch provides forward compatibility with any future package manager without requiring Cursus code changes.
- npm-only publishing keeps the publish path simple, testable, and predictable. There is exactly one code path to maintain and one error format to parse for idempotency.
- Users are not forced to install additional package managers beyond npm for publishing; npm is effectively always available in any Node.js environment.

### Negative

- If a future registry (e.g., JSR via Deno, or a private registry with a non-npm-compatible protocol) becomes prevalent, the npm-only publishing assumption will need to be revisited. This would likely require a `publish_command` override similar to `lock_command`, or a new adapter entirely.
- yarn Berry users who rely on yarn-specific publish plugins (e.g., for workspace versioning or custom registry authentication) cannot use those plugins through Cursus. They must ensure `npm publish` works in their environment.
- The lockfile autodetection order is fixed. If a project somehow has multiple lockfiles (e.g., during a migration), Cursus will use the first one it finds, which may not be the intended one. The `lock_command` override mitigates this.
- `lock_command` does not support shell features (pipes, redirects, environment variable expansion) since it is split on whitespace and executed directly. Users needing shell features must wrap their command in a script.

### Neutral

- There is no `publish_command` override today. If one is added in the future, it would follow the same pattern as `lock_command`: a string in the `[npm]` config section that takes precedence over the default `npm publish` invocation.
- Bun and Deno are explicitly not autodetected for lockfile updates. Users of these tools are expected to configure `lock_command`. This is a pragmatic choice given their smaller adoption footprint and the availability of the override.

## Alternatives Considered

### Full package manager detection for both lockfiles and publishing

Cursus could detect the active package manager for all operations and invoke the appropriate tool-specific commands for both lockfile updates and publishing. This was rejected because the publishing side provides no practical benefit -- `npm publish` works for all npm-registry-compatible managers -- and would significantly increase the surface area for bugs and maintenance. Each package manager has different error message formats, different CLI flags, and different edge cases around scoped packages and authentication.

### No autodetection; always require explicit configuration

Cursus could require users to specify their package manager or lock command explicitly in configuration, rather than autodetecting from lockfiles. This was rejected because lockfile presence is a reliable and well-established signal. Requiring explicit configuration would add friction to the common case (npm, pnpm, or yarn) without meaningful benefit. The override exists for uncommon cases.

### Provide a `publish_command` override alongside `lock_command`

A `publish_command` field in `NpmConfig` would allow users to specify a custom publish command, mirroring `lock_command`. This was deferred rather than rejected. The current npm-only approach covers all known use cases today. If alternative registries (e.g., JSR) gain traction, adding `publish_command` would be a backward-compatible configuration change. Premature abstraction here would add complexity without a concrete use case to validate the design.

### Honor corepack's `packageManager` field from package.json

Instead of autodetecting from lockfiles, Cursus could read the `packageManager` field in `package.json` (e.g., `"packageManager": "pnpm@8.6.0"`). This field was introduced by corepack to provide explicit, version-pinned package manager selection at the project level. Using it would give Cursus a definitive signal without requiring Cursus-specific configuration, and it would respect the project's declared intent rather than inferring it from a side effect (the lockfile).

This was discounted because Node.js is removing corepack from its base distribution starting with Node 25. With corepack no longer bundled by default, the `packageManager` field occupies an uncertain position: it may be present in `package.json` but the tooling that gives it meaning (corepack's automatic package manager installation and routing) may not be available. Relying on this field would create an inconsistent experience where Cursus's behavior depends on whether the user has independently installed corepack.

This approach may be revisited if the `packageManager` field achieves de facto standard status independent of corepack -- for example, if other tools in the ecosystem begin honoring it, or if Cursus could parse the field purely as a detection hint (extracting the package manager name) without depending on corepack being installed. In that scenario, it would serve as a more explicit alternative to lockfile-based detection, sitting between full autodetection and the `lock_command` override in terms of user effort.

## Errata

### 2026-03-01: `lock_command` is no longer whitespace-split

The Decision section's claim that `lock_command` "is split on whitespace and executed directly (not through a shell)", and the matching Negative consequence that users wanting shell features must "wrap their command in a script", are both incorrect. [ADR-011](011-command-execution-strategy.md) establishes a project-wide command-execution standard under which `lock_command` is executed via `/bin/sh -c "<command>"`, so pipes, redirects, environment-variable expansion, and compound commands all work directly. The Neutral note that any future `publish_command` "would follow the same pattern as `lock_command`" remains correct in spirit — it now means the [ADR-011](011-command-execution-strategy.md) standard.
