# ADR-051: Bundle Sigstore Transitive Dependencies into the npm Tarball by Removing the Workspace Declaration

## Status

Accepted (2026-05-01)

## Context

[ADR-049](049-signed-release-artifacts.md) established that the `@zantarix/cursus` npm package verifies GitHub artifact attestations using the `sigstore` JavaScript library before installing the cursus binary downloaded from a GitHub Release. The Negative Consequences section of [ADR-049](049-signed-release-artifacts.md) acknowledged that this introduces a runtime JavaScript dependency on `sigstore`, but it did not address the security implications of how that dependency's *transitive* tree is resolved at consumer install time.

`packages/npm/package.json` declares `"sigstore": "4.1.0"` as an exact-pinned runtime dependency. `sigstore` itself, however, fans out into a substantial transitive tree (`@sigstore/bundle`, `@sigstore/sign`, `@sigstore/verify`, `@sigstore/tuf`, `tuf-js`, `make-fetch-happen`, and several dozen further packages), and those packages declare their own dependencies under floating `^`-ranges. The `@zantarix/cursus` package ships no `npm-shrinkwrap.json`, and `package-lock.json` is excluded from publication via `.npmignore`. Every `npm install @zantarix/cursus` therefore re-resolves the entire sigstore transitive tree against the npm registry at install time, picking up whatever versions satisfy the floating ranges at that moment.

This is a load-bearing supply-chain gap for exactly the trust chain [ADR-049](049-signed-release-artifacts.md) is responsible for. The postinstall verifier is the only piece of code in the cursus distribution that runs on the consumer's machine before the binary is installed and that authenticates the binary's provenance. If an attacker compromises any one of the dozens of transitive dependencies of `sigstore-js` -- via typosquat, maintainer account takeover, or a hostile takeover of an unmaintained package -- they can ship a release whose `postinstall` hook executes ahead of cursus's own postinstall, or whose code monkey-patches the `verify` symbol to return `true` unconditionally, or which silently rewrites `bin/download-binary.js` to skip verification entirely. The modified verifier then runs on every fresh `npm install @zantarix/cursus` and the [ADR-049](049-signed-release-artifacts.md) trust chain is bypassed without defeating any Sigstore cryptographic primitive, without compromising any GitHub workflow, without compromising any Sigstore CA, and without compromising the npm registry's publication path. Only one transitive sub-dep needs to fall.

`bundleDependencies` is the npm-native mitigation: when a package lists a dependency under `"bundleDependencies"`, `npm pack` physically embeds that dependency's resolved tree (i.e., its `node_modules/<dep>/...`) inside the published tarball. The published tarball then contains its dependencies' bytes verbatim, and the tarball as a whole is covered by the npm provenance attestation that `publishConfig.provenance: true` already produces. Consumers running `npm install @zantarix/cursus` get the bundled tree as-is, with no install-time registry round-trip for the bundled deps. A typosquatted or compromised transitive sub-dep cannot be substituted at install time because nothing is being resolved at install time.

`packages/npm/package.json` already declares `bundleDependencies: ["sigstore"]`. Inspection of the packed `packages/npm/zantarix-cursus-0.5.0.tgz` artifact, however, confirms the bundling is not effective: the tarball contains only `bin/`, `package.json`, and `CHANGELOG.md`, with no embedded `node_modules/`. The root cause is that the repository declares an npm workspace at the git root (`package.json` declares `workspaces: ["packages/npm", "docs/site"]`). Under workspace semantics, `npm install` hoists `sigstore` and its tree to the root `node_modules/`. When `npm pack` (or `npm publish`) runs from `packages/npm/`, `bundleDependencies` looks for the named packages in the package's own `node_modules/`, finds nothing there, and emits a tarball with no bundled tree -- silently. The current state of the published `@zantarix/cursus` package therefore offers no protection against the transitive-dependency substitution threat described above; the `bundleDependencies` declaration is cosmetic.

The two workspace members (`packages/npm` and `docs/site`) share zero runtime or dev dependencies. Every CI step that operates on either package already runs with an explicit `working-directory` set to the package, so workspace-root semantics are not load-bearing for CI. Renovate already tracks both `package.json` files independently. The workspace declaration exists only to allow `npm install` at the repo root to install both packages at once; it provides no other coupling between them.

## Decision

We will ship the sigstore tree physically bundled inside the published `@zantarix/cursus` tarball via `bundleDependencies`, and we will make that bundling effective by removing the npm workspace declaration from the repository root.

Concretely:

- The `"workspaces"` key in the root `package.json` will be removed. The root `package.json` (which is `"private": true`, version `0.0.0`, with no scripts and no production dependencies) exists only to declare the workspace and may be deleted entirely once the workspace key is gone.
- `packages/npm` and `docs/site` become two independent npm projects that happen to live in the same git repository. `npm install` inside either directory populates that directory's local `node_modules/` (unhoisted), and produces that directory's own `package-lock.json`.
- With the workspace removed, `npm pack` and `npm publish` run from `packages/npm/` find sigstore in `packages/npm/node_modules/sigstore/...` -- which is where `bundleDependencies` looks for it -- and the resolved sigstore tree is physically embedded in the published tarball.
- The tarball's bundled tree is covered by the existing `publishConfig.provenance: true` attestation, so consumers running `npm audit signatures` get attestation that the bundled sigstore tree was published from the canonical workflow.
- CI workflows that currently rely on workspace-root semantics will be updated so each `npm ci` invocation runs with `working-directory` scoped to the package being built. CI was already using per-package `working-directory` for build/test/lint steps, so this is a small extension of the existing pattern to install steps.
- No changes to the cursus Rust codebase are required. No changes to `packages/npm/package.json` beyond the already-declared `bundleDependencies` are required.
- The contributor workflow becomes "`cd packages/npm && npm install`" (or equivalently for `docs/site`) rather than `npm install` at the repo root. This must be documented in the contributor-facing setup docs.

The guarantee this purchases is precisely that the sigstore code that runs on a consumer's machine at install time is byte-identical to the version pinned in `packages/npm/package.json` and tested against in CI, and that nothing in the npm registry can be substituted between publish and install without invalidating the npm provenance attestation on the tarball.

## Consequences

### Positive

- `bundleDependencies: ["sigstore"]` becomes effective. Every published `@zantarix/cursus` tarball physically contains the sigstore tree it was tested against. There are no install-time registry round-trips for sigstore's transitive deps, so a compromised transitive sub-dep cannot be substituted at install time.
- The bundled tree is covered by the npm provenance attestation already produced by `publishConfig.provenance: true`. A consumer running `npm audit signatures` against `@zantarix/cursus` gets attestation that the bundled sigstore tree was published by the canonical workflow.
- The bundled sigstore source is byte-identical to the version pinned in `package.json`. A consumer can `npm install sigstore@4.1.0` separately and diff the two trees to verify this independently. This auditability is meaningful for code that sits on the [ADR-049](049-signed-release-artifacts.md) trust path.
- No changes to cursus's Rust code, no new cursus library surface, and no new package-manager-adapter behaviour. The fix is contained to the npm package directory and the workspace declaration.
- Developer experience improves locally: a contributor running `npm install` inside `packages/npm/` gets the unhoisted dependency tree that matches what is published, rather than the workspace-hoisted tree that does not.

### Negative

- The published tarball is larger. The sigstore tree (including `@sigstore/{bundle,sign,verify,tuf}`, `tuf-js`, `make-fetch-happen`, and their transitive deps) is physically embedded rather than resolved from the registry. This size cost is the security property being purchased; the trade is intentional and small.
- Two `package-lock.json` files must be maintained (`packages/npm/package-lock.json` and `docs/site/package-lock.json`) instead of a single root lockfile. Renovate already tracks both paths, so the operational impact is on contributors, not on automation.
- CI workflows need minor updates to scope `npm ci` to each package directory rather than relying on workspace-root install semantics.
- Contributors must `cd packages/npm && npm install` (or `cd docs/site && npm install`) instead of running a single `npm install` at the repo root. This changes the documented onboarding flow.

### Neutral

- The `packageManager` field in the root `package.json` (which pins the npm version) is removed along with the workspace root. Each package directory may re-declare it if desired; in practice the npm version is governed by the Nix dev shell and the field is not load-bearing.
- This ADR establishes a posture but does not encode it in tooling: any future runtime dependency added to `packages/npm/package.json` must also be added to `bundleDependencies` to maintain the same install-time guarantee. There is no automated check for this; reviewers must enforce it.
- The trust root, the Sigstore primitives, the GitHub Actions OIDC issuer, and the npm provenance mechanism are unchanged from [ADR-049](049-signed-release-artifacts.md). This ADR refines how the verifier code reaches the consumer's machine; it does not change what the verifier verifies.

## Alternatives Considered

### Built-in cursus staging via a new `[npm].bundle_dependencies` config field

Have the cursus `NpmAdapter` copy the package being published to a temporary directory, run `npm install` there, and publish from the staged copy. This would bypass the workspace-hoisting problem for any cursus user (not just the cursus repository itself) and is technically the "correct" cursus-level fix.

Rejected because it adds a substantial new code path to `package_manager/npm/mod.rs`, introduces `tempfile` as a runtime dependency (currently test-only), and is scope creep for a fix that needs to land for the cursus repo's own security posture. It remains viable as a separate future ADR if a downstream cursus user reports the same problem.

### `[npm].publish_dir` config override

Add a config field that points cursus at an alternate directory to publish from, and have the user shell out to a `[github].build_command` that stages the package there. Smaller than the built-in staging alternative, but still requires a cursus code change plus user-side shell glue, and leaves the workspace declaration in place -- which means a developer running `npm install` at the repo root locally still gets the hoisted (and therefore unbundled) tree. Rejected.

### Publish-from-tarball flow (`[npm].publish_tarball`)

Have the user's `build_command` produce a `.tgz` directly, and have cursus invoke `npm publish <tgz>`. The most flexible of the cursus-level alternatives, but also the largest cursus surface change, and shares the "future work" argument with the staging alternative. Rejected for the same reason.

### `npm-shrinkwrap.json` instead of bundling

Ship `npm-shrinkwrap.json` in the published tarball. This pins exact versions and integrity hashes for the entire transitive tree, so consumers get deterministic resolution. However, sigstore's transitive deps are still fetched from the npm registry at consumer install time -- they are not physically present in the tarball, and they are not covered by the npm provenance attestation on the tarball. The guarantee is weaker: it protects against version drift but not against a registry-level substitution against a package whose maintainer account or publication pipeline has been compromised. Generating a shrinkwrap from a workspace member is also awkward in practice. Rejected.

### Vendor the resolved sigstore tree into git

Commit `packages/npm/node_modules/sigstore/...` directly to the repository. Achieves the same physical-bundling property as `bundleDependencies` does at publish time, but causes substantial repository bloat, requires manual update workflows, and breaks Renovate's ability to track sigstore as a versioned dependency. Rejected.

### Vendor sigstore source / hand-roll a Sigstore verifier

Either copy sigstore's source into the repository under our maintenance, or implement a minimal Sigstore verifier from cryptographic primitives. The first carries an enormous ongoing maintenance burden tracking upstream sigstore changes; the second carries an enormous correctness risk for a security-critical primitive. Rejected.

### Move `packages/npm` to a separate git repository

Solves the workspace-hoisting problem by removing the workspace context entirely, but at the cost of decoupling the npm wrapper from the codebase it wraps. The colocation is operationally valuable: changes to the cursus binary's distribution surface and the npm package that fetches it land together, are tested together, and are released together. This is a heavier change with no security benefit beyond what removing the workspace declaration already achieves. Rejected.

### Bundle `sigstore` and `download-binary` together with a JS bundler

Use a JavaScript bundler (rolldown, esbuild, rollup) to compile `src/download-binary.ts` together with sigstore into a single self-contained `bin/download-binary.js`, and ship sigstore as a `devDependency` rather than a runtime dependency. Pros: zero install-time fetches, smaller tarball, tree-shaking reduces the shipped surface area.

Rejected on auditability grounds. The bundled output is a derived artifact: a consumer cannot easily verify that the sigstore code embedded in the bundle is byte-identical to what sigstore's maintainers published as `sigstore@4.1.0`. The `bundleDependencies` approach ships sigstore's published source verbatim and version-pinned, so a consumer can `npm install sigstore@4.1.0` independently and diff to confirm. For code that sits on the [ADR-049](049-signed-release-artifacts.md) trust path, that auditability is worth the extra tarball bytes. Bundler optimisations applied to a cryptographic verifier also carry non-zero correctness risk that does not exist when sigstore's source is shipped unmodified.
