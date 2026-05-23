# ADR-049: Sign GitHub Release Artifacts and Verify in npm Postinstall

## Status

Accepted (2026-04-27)

## Context

Cursus distributes its release binaries through two channels established in [ADR-022](022-distribution-strategy.md): GitHub Releases (primary) and the `@zantarix/cursus` npm package (secondary). The npm package ships no binaries -- its postinstall script downloads the platform-specific binary from the matching GitHub Release at install time, using a hard-fail policy on any download error.

Today, the only trust anchor for that download is TLS. There is no content-level authentication that proves a downloaded binary was actually produced by one of the canonical release workflows in `zantarix/cursus`. Two concrete attack surfaces exist:

1. **A tampered GitHub Release.** A maintainer account compromise (or an attacker with sufficient permissions on the repository) could replace the binary asset on an existing Release with a malicious payload. The postinstall script would download and install it silently because TLS only authenticates the transport, not the content's provenance.
2. **A binary uploaded from a rogue workflow.** A pull request from a fork that successfully modifies the release workflow, or an unrelated workflow with `contents: write` permission, could attach an artifact to a Release. Without identity-pinned signatures, every asset on a Release is treated as equally trustworthy by the postinstall script.

GitHub's artifact attestations feature -- built on top of Sigstore (Fulcio for keyless certificate issuance, Rekor for the public transparency log, OIDC tokens as the identity claim) -- directly addresses both surfaces. It produces a SLSA build provenance attestation per artifact, signed by an ephemeral certificate whose Subject Alternative Name encodes the exact workflow file and ref that produced the artifact. With identity pinning, only the legitimate release workflows on the canonical tag ref can produce a verifying attestation.

This direction is also consistent with cursus's existing OIDC posture. [ADR-028](028-npm-oidc-trusted-publishing.md) (npm trusted publishing) and [ADR-045](045-crates-io-trusted-publishing.md) (crates.io trusted publishing) already establish that the project prefers GitHub Actions OIDC identity over long-lived secrets for any credential-bearing operation. Signing artifacts via OIDC keyless extends that posture to the binary-distribution layer without introducing new key custody, rotation, or revocation responsibilities.

The npm package is the natural verification choke point. It is the only distribution channel where cursus controls install-time client code: every `npm install @zantarix/cursus` runs the postinstall script, and the script already owns the network fetch and file write that need to be authenticated. Verifying there means every npm-channel install gets identity-pinned signature verification automatically, with no opt-in step on the user's part.

The npm tarball itself is already covered by npm's own provenance feature via `publishConfig.provenance: true` in `packages/npm/package.json`. That handles the "is this tarball really from this repo" question for the npm registry side. The gap this ADR closes is the binary that the postinstall script downloads from GitHub Releases, which lives outside that provenance boundary.

## Decision

We will sign every GitHub Release binary artifact with a Sigstore-backed GitHub artifact attestation, and we will verify that attestation in the npm postinstall script before the downloaded binary is written to disk or made executable.

### Signing in the release workflows

Two distinct workflows produce and upload release artifacts, and each must produce attestations for the artifacts it owns using `actions/attest`. All seven artifact targets enumerated in [ADR-022](022-distribution-strategy.md) and amended by [ADR-048](048-native-windows-build-runner.md) are in scope, partitioned as follows:

- **`release.yml`** runs on `push: branches: [main]` and invokes `cursus ci` -> `cursus publish`, whose `build_command = "cargo make release-linux"` produces and uploads the three Linux artifacts (`cursus-linux-x86_64`, `cursus-linux-aarch64`, `cursus-linux-riscv64gc`). Attestations for the Linux artifacts are produced in this workflow.
- **`release-artifacts.yml`** is triggered by `release: published` and builds the macOS (`cursus-osx-x86_64`, `cursus-osx-aarch64`) and Windows (`cursus-windows-x86_64.exe`, `cursus-windows-aarch64.exe`) artifacts natively. Attestations for the macOS and Windows artifacts are produced in this workflow.

In both workflows the attestation step runs in the same job that produces each artifact and is gated on the artifact's existence.

Signing is rooted in each workflow's GitHub Actions OIDC token. Fulcio issues an ephemeral signing certificate whose Subject Alternative Name encodes the workflow file path, repository, and ref -- so the SAN differs between the two workflows even though the issuer, repository, and tag ref are identical. The signature and certificate are recorded in the public Rekor transparency log. No long-lived signing keys are introduced and no manual key rotation is required.

### Verification in the npm postinstall

`packages/npm/src/download-binary.ts` shall verify the attestation before any verified bytes touch disk. The verification sequence is:

1. **Compute digest.** After downloading the binary into an in-memory buffer (or a quarantined temporary path that is not yet executable and not yet at the final install path), compute its SHA-256.
2. **Fetch attestation bundle.** Issue an unauthenticated `GET https://api.github.com/repos/zantarix/cursus/attestations/sha256:<digest>` request. The endpoint serves attestations for public repositories without requiring a token.
3. **Verify the bundle.** Use a Sigstore JS verifier (`sigstore`'s `verify` function) to validate the certificate chain (Fulcio root), the Rekor inclusion proof, and the bundle signature against the binary's digest. The verifier is invoked with the platform-appropriate expected SAN derived in step 4.
4. **Enforce identity policy.** The expected Subject Alternative Name is keyed to the platform/artifact being installed, because Linux artifacts are attested in `release.yml` (the `cursus publish` flow) while macOS and Windows artifacts are attested in `release-artifacts.yml`. The postinstall script already knows the platform at runtime via `const platform = process.platform` and the artifact-name mapping it uses to choose the download URL, so it can derive the expected workflow filename without any additional input. Concretely:
   - For `linux/x64`, `linux/arm64`, and `linux/riscv64`, the SAN must equal `https://github.com/zantarix/cursus/.github/workflows/release.yml@refs/tags/cursus@<version>`.
   - For `darwin/x64`, `darwin/arm64`, `win32/x64`, and `win32/arm64`, the SAN must equal `https://github.com/zantarix/cursus/.github/workflows/release-artifacts.yml@refs/tags/cursus@<version>`.

   In both cases `<version>` is the version of the npm package being installed, the OIDC issuer must be `https://token.actions.githubusercontent.com`, and the ref component must be `refs/tags/cursus@<version>`. Only the workflow filename varies. This pin is non-negotiable: without it, any GitHub Actions run anywhere could produce a syntactically valid attestation.
5. **Confirm subject digest.** The attestation's subject digest must match the SHA-256 computed in step 1.

Only after all five steps succeed will the binary be written to its final install path and made executable.

On any verification failure -- missing attestation, signature failure, identity mismatch, digest mismatch, or transport error fetching the bundle -- the postinstall script will delete any partial download, log a clear error explaining the failure, and exit non-zero. This matches the hard-fail philosophy established in [ADR-022](022-distribution-strategy.md): a non-functional install with a clear error is preferable to a silently degraded install.

### Cutover and backward compatibility

The npm package version is locked to a specific cursus release version, so a postinstall script that requires attestations only ever fetches binaries from the release that ships alongside it. Once an npm package version that ships with verification is released, the corresponding GitHub Release is required to carry attestations -- there is no version skew window where an attestation-aware postinstall could legitimately fetch from a release without attestations.

Older npm package versions (e.g., `cursus@0.2.2`) will continue to work as they always have, downloading without verification. They are not retroactively secured. Soft-failing on missing attestations in newer versions is rejected because it would silently restore the pre-decision threat model whenever an attacker could simply omit the attestation.

### Scope

In scope:

- Producing attestations in `release.yml` for the three Linux artifact targets and in `release-artifacts.yml` for the four macOS and Windows artifact targets.
- Verifying attestations in the npm postinstall script before installing the binary.
- Hard-failing the install on any verification failure.

Out of scope:

- Apple notarization and macOS Gatekeeper trust for the macOS binaries. These require Apple Developer Program enrolment, key custody, and a separate certificate procurement workflow. They will be addressed in a future ADR if and when the user demand justifies the operational cost.
- Windows Authenticode signing of the `.exe` artifacts. Same reasons as Apple notarization -- this requires a code-signing certificate with key custody.
- Signing the npm tarball itself. This is already covered by npm provenance via `publishConfig.provenance: true` in `packages/npm/package.json`, which uses the same OIDC trust root as this ADR.

## Consequences

### Positive

- No long-lived signing keys are introduced. Trust is fully OIDC-derived, consistent with the posture established by [ADR-028](028-npm-oidc-trusted-publishing.md) and [ADR-045](045-crates-io-trusted-publishing.md). Cursus continues to never own credentials.
- Verification is automatic for every `npm install @zantarix/cursus`. There is no opt-in step or separate command for users to run.
- The identity-pinned attestation closes the "rogue workflow on the same repository" gap: even an attacker who can run `actions/attest` from a different workflow file or a different ref produces an attestation that fails the pinned-identity check.
- SLSA build provenance is a useful side-product. Downstream supply-chain audit tooling (e.g., consumers running `gh attestation verify` or scanning Rekor) gets a verifiable record of how each binary was built, for free.
- No changes to cursus Rust code are required. The change is confined to the npm package and the release workflow.

### Negative

- The npm package gains a runtime JavaScript dependency (`sigstore`). It was previously a near-zero-runtime-dependency package. Two dev dependencies (`@sigstore/rekor-types`, `@types/make-fetch-happen`) are also added to bridge missing type declarations from `sigstore`'s transitive dependencies. This expands the attack surface of the install path itself, though the alternative -- shipping no signing at all -- is strictly worse.
- The postinstall script now requires two GitHub network endpoints (Releases asset download plus attestations API) instead of one. Either being unavailable hard-fails the install.
- The unauthenticated GitHub REST API has a 60-request-per-hour rate limit per IP. Heavy install scenarios (e.g., large CI fleets sharing an egress IP) can hit it, causing transient install failures even when nothing is actually wrong. There is no mitigation inside cursus's control short of asking users to authenticate the API call, which is friction the postinstall script is explicitly designed to avoid.
- The identity pin hard-codes two workflow paths (`release.yml` for Linux artifacts and `release-artifacts.yml` for macOS and Windows artifacts). If either file is ever moved or renamed, every previously-released npm package version with the new pin would still work for its own binaries, but coordinated updates between the workflow rename and any in-flight npm package version are required. This is a minor coordination cost that needs to be acknowledged on workflow refactors, and it now applies to two workflows rather than one.
- Hard-fail on missing attestations means that if the attestation service or the GitHub attestations API is broken, no installs succeed even though the binary itself is fine. This is the deliberate consequence of choosing security over availability for the install path.
- A new npm package version cannot install binaries from any release that does not carry attestations, including any future scenario where the release workflow temporarily fails to attest. This is a small operational hazard for releases.

### Neutral

- The trust root is the GitHub Actions OIDC issuer. This is the same trust anchor cursus already relies on for npm trusted publishing ([ADR-028](028-npm-oidc-trusted-publishing.md)) and crates.io trusted publishing ([ADR-045](045-crates-io-trusted-publishing.md)). The blast radius of a compromise of `https://token.actions.githubusercontent.com` is unchanged by this decision -- it would already be catastrophic for the project.
- Users who download the binary directly from the GitHub Releases page without going through npm continue to get no automatic verification. They can opt in via `gh attestation verify`, but this is unchanged by this ADR.
- The Sigstore project is comparatively young infrastructure. Its public-good services (Fulcio, Rekor) are operated by the OpenSSF and are widely depended on by GitHub itself, npm provenance, PyPI provenance, and others. This ADR's exposure to Sigstore outages is therefore broadly aligned with the rest of the modern OIDC supply-chain ecosystem.

## Alternatives Considered

### Sigstore/cosign keyless directly, distributing `.sig` and `.pem` as Release assets

Use the lower-level Sigstore primitives without GitHub's attestations wrapper: produce a detached signature and certificate per artifact, attach them as additional assets on the GitHub Release, and verify with a cosign-compatible JS library on the postinstall side. The trust root is identical. This was rejected because it requires distributing two extra assets per binary (so 14 extra assets per release), maintaining a custom retrieval convention, and exposing more surface for asset-naming drift. GitHub artifact attestations is the GitHub-native wrapper around these same primitives with a simpler, single-endpoint retrieval path and no extra release assets to coordinate.

### SHA-256 checksum manifest only (`SHA256SUMS` alongside the binaries)

Generate a `SHA256SUMS` file in the release workflow and have the postinstall script verify the downloaded binary's digest against that manifest. This was rejected because the manifest itself is unauthenticated. An attacker who can tamper with the binary on the Release can also tamper with the manifest. A checksum manifest only protects against accidental corruption in transit (which TLS already handles) and does nothing against the threat model that motivates this ADR.

### Minisign or GPG with long-lived keys

Generate a Minisign or GPG signing key, store its private half in a GitHub Actions secret, sign artifacts with it during the release workflow, and verify with the corresponding public key embedded in the npm package. This was rejected on principle: it reintroduces the long-lived credential management problem that [ADR-028](028-npm-oidc-trusted-publishing.md) and [ADR-045](045-crates-io-trusted-publishing.md) deliberately walked away from. Cursus would have to take responsibility for key rotation, revocation, secret hygiene, and onboarding/offboarding maintainers who can rotate the key. OIDC keyless eliminates all of those concerns and offers the same security properties.

### User-side opt-in verification only (`gh attestation verify`)

Produce attestations on the release side but rely on users to verify them by running `gh attestation verify` against the binary they download. This was rejected because the default install path -- `npm install @zantarix/cursus` -- would remain unauthenticated. Users who do not know about the verification command, or who automate installs in CI, would be unprotected. This is structurally inconsistent with the [ADR-022](022-distribution-strategy.md) principle of hard-failing rather than silently degrading: it pushes a security-critical step onto users who do not know they need to take it.

## Errata

### 2026-04-27: Two-workflow attestation split was unworkable

The Decision section's two-workflow split — Linux attestations in `release.yml` (push-triggered), macOS/Windows attestations in `release-artifacts.yml` (release-triggered) — and the matching platform-keyed identity policy in the npm postinstall are both incorrect. The split was unworkable because `release.yml` runs on a branch push, so the GitHub Actions OIDC token carries `refs/heads/main` as the ref claim; Fulcio embeds that ref verbatim in the certificate SAN, so the resulting attestation can never satisfy the `refs/tags/cursus@<version>` identity pin and Linux verification hard-fails with a certificate identity mismatch. The corrected design moves the Linux build, artifact upload, and attestation step into `release-artifacts.yml` alongside the macOS and Windows jobs so all seven artifacts are attested under the tag ref by a single workflow; the npm postinstall's identity policy collapses to one expected workflow path (`https://github.com/zantarix/cursus/.github/workflows/release-artifacts.yml@refs/tags/cursus@<version>`), the platform-keyed branching in step 4 of the verification sequence is removed, `release.yml` loses its `attestations: write` permission, and `.cursus/config.toml` no longer carries `build_command` or `[github.artifacts.cursus]`. The trust root, Sigstore primitives, hard-fail philosophy, scope, and rejected alternatives are unchanged.

### 2026-04-30: Sigstore transitive-dependency pinning gap

The Negative Consequences section acknowledges the new runtime `sigstore` dependency but does not address how its transitive tree is resolved at consumer install time, which left a real gap in the trust chain. As originally shipped, every `npm install @zantarix/cursus` re-resolved sigstore's transitive tree from the npm registry under floating `^`-ranges, so compromise of any one of dozens of transitive sub-deps could bypass the [ADR-049](049-signed-release-artifacts.md) chain without defeating any Sigstore primitive. [ADR-051](051-bundle-sigstore-deps-via-workspace-removal.md) closes this gap by physically embedding the sigstore tree in the published tarball via `bundleDependencies` and removing the npm workspace declaration that was preventing the bundling from taking effect.
