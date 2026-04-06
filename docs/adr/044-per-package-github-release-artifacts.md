# ADR-044: Restructure GitHub Artifact Configuration to Per-Package Sections

## Status

Accepted (2026-04-06)

## Context

[ADR-005](005-github-releases.md) established a flat `[github.artifacts]` configuration as a `BTreeMap<String, String>` mapping display names to file paths. This design assumed that every GitHub Release in a publish run would receive the same set of artifacts. The `build_command` runs once globally and all configured artifacts are attached to every release.

This works for single-package repositories and for monorepos where all packages produce identical platform binaries (as Cursus itself does today). However, it breaks down in monorepos where different packages produce different artifacts. For example, a workspace containing both a CLI tool and a library would want platform binaries attached only to the CLI's release, not the library's. ADR-005 acknowledged this limitation explicitly: "In a monorepo with multiple packages, artifacts are attached to every GitHub Release created during the publish run. If different packages need different artifacts, the user should manage this through their build command."

Managing per-package artifact selection through the build command is fragile and indirect. The build command has no awareness of which packages are being published or which releases will be created. Users would need to implement their own package-detection logic to conditionally produce artifacts, then rely on missing-file tolerance to silently skip inapplicable entries. This violates the principle of explicit configuration.

The current `artifacts` field type (`BTreeMap<String, String>`) in `GitHubConfig` is the structural constraint. Changing it to a nested map (`BTreeMap<String, BTreeMap<String, String>>`) keyed by package name enables per-package artifact declarations while retaining the existing display-name-to-path mapping within each package.

Cursus is pre-1.0 and has no stability guarantee for its configuration format. This is an appropriate time to make breaking configuration changes.

## Decision

We will replace the flat `[github.artifacts]` table with per-package `[github.artifacts.<package-name>]` sections in `.cursus/config.toml`.

### Configuration format

The new structure nests artifact maps under package names:

```toml
[github]
enabled = true
build_command = "cargo make release-cross"

[github.artifacts.cursus]
cursus-linux-x86_64          = "target/x86_64-unknown-linux-musl/release/cursus"
cursus-linux-aarch64         = "target/aarch64-unknown-linux-musl/release/cursus"
"cursus-windows-x86_64.exe"  = "target/x86_64-pc-windows-gnullvm/release/cursus.exe"

[github.artifacts.cursus-action]
"action.yml" = "packages/cursus-action/action.yml"
```

Each key under `[github.artifacts]` is a package name matching the name used in changesets and `enumerate_projects()`. Each value is a `BTreeMap<String, String>` with the same display-name-to-path semantics as the original flat map.

### Structural change

The `artifacts` field on `GitHubConfig` will change from:

```
artifacts: BTreeMap<String, String>
```

to:

```
artifacts: BTreeMap<String, BTreeMap<String, String>>
```

### Artifact selection during publish

When creating a GitHub Release for a package, Cursus will look up `config.github.artifacts.get(package_name)` to obtain that package's artifact map. Only matching artifacts are uploaded to the release. Packages without an entry in `[github.artifacts]` receive no artifacts -- their releases contain only changelog-derived release notes.

### No merge or inheritance

There is no "default" or "global" artifact set. Each package's artifacts are declared independently. If two packages need the same artifact, the path must be listed in both sections. This keeps the configuration model simple and predictable: what you see under a package name is exactly what gets attached to its release.

### `build_command` remains global

The `build_command` field continues to run once per `cursus publish` invocation, not once per package. This is consistent with [ADR-005](005-github-releases.md)'s original design and reflects that build commands typically produce all artifacts in a single invocation (e.g., `cargo make release` cross-compiles all targets).

### Dry-run output

Dry-run output will show per-package artifact lists instead of a single global list, making it clear which artifacts apply to which release.

### Single-package convenience

Single-package repositories must still use the package name as the key. There is no shorthand flat syntax. This avoids ambiguity in the configuration parser and keeps one code path for artifact resolution.

## Consequences

### Positive

- Each package's GitHub Release receives only its own artifacts, eliminating irrelevant file attachments in monorepos
- The configuration explicitly declares the artifact-to-package relationship, removing the need for build-script workarounds
- Dry-run output becomes more informative by showing per-package artifact lists
- The nested TOML table structure is idiomatic and self-documenting

### Negative

- This is a breaking change to `.cursus/config.toml`; existing configurations using the flat `[github.artifacts]` format will fail to deserialize with a clear TOML type-mismatch error (`invalid type: string "...", expected a map`), since the `artifacts` field name is unchanged but its expected type changed from `BTreeMap<String, String>` to `BTreeMap<String, BTreeMap<String, String>>`
- Single-package repositories gain a small amount of configuration verbosity (one extra nesting level)
- Packages that genuinely need identical artifacts must duplicate the entries, though this is uncommon in practice

### Neutral

- `build_command` remains global and unchanged; this ADR does not introduce per-package build commands
- The `serde(skip_serializing_if)` behavior on the artifacts field continues to omit empty maps, keeping generated configs clean
- The `cursus init` wizard does not configure artifacts (they are added manually), so no TUI changes are needed

## Alternatives Considered

### Glob-based artifact patterns

Allow glob patterns in artifact paths (e.g., `"target/*/release/cursus*"`) to automatically discover and attach files. Rejected because globs conflate file discovery with naming: the display name shown on GitHub Releases must be explicitly chosen, and glob expansion cannot reliably produce user-friendly download names. Per-package sections solve the scoping problem without sacrificing naming control.

### Global artifacts with per-package overrides

Keep the flat `[github.artifacts]` as a default and allow `[github.artifacts.<package-name>]` sections to override it. Rejected because merge semantics introduce complexity (does a per-package section replace or extend the global set?) and make it harder to reason about which artifacts end up on a given release. Explicit-only configuration is simpler.

### Per-package `build_command`

Extend this change to also make `build_command` per-package. Rejected because build commands typically produce all artifacts in one invocation (cross-compilation, CI matrix outputs), and per-package builds would require users to split their build logic unnecessarily. A future ADR can add this if demand arises.

### Maintain backward compatibility with migration

Accept both the flat and nested formats during a transition period, with a deprecation warning for the flat format. Rejected because Cursus is pre-1.0 with no stability guarantee, the user base is small, and migration is a one-line config edit. The complexity of dual-format parsing is not justified.
