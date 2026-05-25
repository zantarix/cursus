# ADR-061: Token-Free Cross-Platform Artifact Verification via Release-Asset Sigstore Bundles

## Status

Accepted (2026-05-25)

## Context

Cursus signs every GitHub Release binary with a Sigstore-backed GitHub artifact attestation and verifies it at install time ([ADR-049](049-signed-release-artifacts.md)). Two consumers currently perform that verification: the `@zantarix/cursus` npm postinstall script (verifying against the public GitHub attestations API with a bundled `sigstore-js` verifier, hardened by [ADR-051](051-bundle-sigstore-deps-via-workspace-removal.md)), and the GitHub Actions `zantarix/actions/setup-cursus` composite action (verifying with `gh attestation verify`). Both paths reach the same trust root: an ephemeral Fulcio certificate whose Subject Alternative Name pins the exact release workflow and tag ref, with the OIDC issuer pinned to `https://token.actions.githubusercontent.com`.

This verification posture does not extend to non-GitHub platforms. `gh attestation verify` requires a GitHub token in its environment, and the public attestations endpoint is part of the GitHub REST API, which is rate-limited to 60 requests per hour per source IP when unauthenticated (5000/hr authenticated). On shared-IP CI runners — the common case on GitLab CI and other platforms — both constraints bite: there is often no GitHub token available, and even unauthenticated discovery is fragile under fleet egress.

The concrete consequence is that consumers on GitLab CI (and other non-GitHub platforms) currently download cursus release binaries such as `cursus-linux-x86_64` directly from GitHub Releases with no practical token-free way to verify them — an unverified-download supply-chain gap — precisely because token-free verification was not yet available off-platform. Closing that gap requires two things: a token-free way to verify a cursus release download anywhere, and a prepackaged GitLab consumer equivalent to the GitHub `setup-cursus` action.

The Sigstore bundle that `actions/attest` already produces is self-contained: it carries the Fulcio certificate (and therefore the pinned SAN identity), the signature, and the Rekor inclusion proof. Verification of such a bundle is anchored on the Sigstore public-good trust root, which is distributed out-of-band via TUF rather than fetched from GitHub. This means the bundle can be verified fully offline once the verifier holds both the binary and the bundle bytes — the GitHub attestations API is one transport for obtaining the bundle, not a component of the trust model. The attestations API's distinctive value is *discovery by digest* for artifacts whose producer did not co-distribute a bundle; cursus controls its own distribution and can co-locate the bundle with the binary, so discovery is not needed.

## References

- [GitHub REST API rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api)
- [GitHub artifact attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations)
- [cosign verify-blob-attestation](https://github.com/sigstore/cosign/blob/main/doc/cosign_verify-blob-attestation.md)
- [Verifying GitHub artifact attestations with cosign bundles](https://blog.sigstore.dev/cosign-verify-bundles/)
- [GitLab CI/CD Components and the CI/CD Catalog](https://docs.gitlab.com/ci/components/)

## Decision

We will make a Sigstore bundle published as a GitHub Release asset the single, token-free verification source for every cursus distribution channel, and we will publish a GitLab CI/CD Component that consumes it. This reuses the existing [ADR-049](049-signed-release-artifacts.md) signing infrastructure unchanged — no new signing path, no new keys, and no change to the trust model. Only the transport by which a verifier obtains the bundle changes.

### Publish the Sigstore bundle as a Release asset

The `actions/attest` step in `release-artifacts.yml` already produces a self-contained Sigstore bundle for each binary. We will upload that bundle to the GitHub Release alongside its binary, named to match the binary (for example `cursus-linux-x86_64.sigstore.json` beside `cursus-linux-x86_64`). `actions/attest` writes every bundle to a unique temporary directory under the fixed filename `attestation.json`, and a GitHub Release asset takes its name from the file's basename, so each bundle is copied to a `<binary>.sigstore.json` filename before upload — the same basename-renaming step the binaries themselves already require. Verifiers then download the binary and its bundle over the same un-rate-limited Release-asset CDN path the binary already uses, and verify entirely offline with zero GitHub API calls and zero token.

Serving the bundle as a Release asset is cryptographically equivalent to fetching it from the attestations API. The bundle is self-contained, and the verification fails closed on tampering through three independent checks regardless of transport: the subject digest binds the bundle to the exact binary bytes, the signature must validate against the Fulcio certificate chain and Rekor inclusion proof, and the pinned SAN and OIDC issuer must match the canonical release workflow. An attacker who substitutes either the binary or the bundle cannot satisfy all three.

### Token-free verification with cosign

The canonical token-free verifier is `cosign`, invoked as:

```
cosign verify-blob-attestation \
  --bundle <binary>.sigstore.json \
  --new-bundle-format \
  --certificate-identity <SAN> \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  <binary>
```

cosign performs offline verification anchored on the Sigstore public-good trust root distributed via TUF. cosign v2.4.0 or later is required, as that is the first release able to consume the GitHub-format `.sigstore.json` bundle via `--new-bundle-format`. The `<SAN>` is the per-release pinned identity, of the form `https://github.com/zantarix/cursus/.github/workflows/release-artifacts.yml@refs/tags/cursus@<version>` (consistent with the single-workflow identity policy established by the [ADR-049](049-signed-release-artifacts.md) errata of 2026-04-27).

### Consolidate every channel onto the Release-asset bundle

The Release-asset bundle becomes the sole verification source across channels, removing all runtime dependency on the rate-limited attestations API and on the `gh` CLI:

- The GitHub Actions `setup-cursus` action will migrate from `gh attestation verify` to downloading the bundle asset and verifying offline with cosign, dropping its requirement for a GitHub token for verification.
- The npm postinstall will migrate from the public attestations API to the bundle asset, retaining its bundled `sigstore-js` verifier ([ADR-049](049-signed-release-artifacts.md), [ADR-051](051-bundle-sigstore-deps-via-workspace-removal.md)) and the pinned SAN and OIDC-issuer checks. Only the bundle's transport changes; the cryptographic posture is unchanged.

This amends only the verification *source* decided in [ADR-049](049-signed-release-artifacts.md) and [ADR-051](051-bundle-sigstore-deps-via-workspace-removal.md) — where the bundle is fetched from — not the trust model those ADRs establish.

### Publish a GitLab CI/CD Component

A new GitLab CI/CD Component named `setup-cursus` will be published to the GitLab CI/CD Catalog from a new `zantarix/components` project on gitlab.com (the `zantarix` GitLab group already exists). Following GitLab's convention that one project is one catalog resource exposing its components as `templates/*.yml`, the component is consumed as:

```
include:
  - component: gitlab.com/zantarix/components/setup-cursus@<version>
```

The `zantarix/components` project may host further components later. The `setup-cursus` component is the GitLab-catalog equivalent of the GitHub Actions `setup-cursus` action and will:

- Accept `version` and `version-file` inputs, mirroring the GitHub action's interface.
- Download the pinned `cursus-linux-x86_64` binary and its matching `.sigstore.json` bundle from the corresponding GitHub Release.
- Run cosign offline with the SAN pinned to `https://github.com/zantarix/cursus/.github/workflows/release-artifacts.yml@refs/tags/cursus@<version>` and the OIDC issuer pinned to `https://token.actions.githubusercontent.com`.
- Install cursus onto `PATH`.
- Cache per resolved version.
- Fail closed on any verification failure, consistent with the hard-fail philosophy of [ADR-022](022-distribution-strategy.md) and [ADR-049](049-signed-release-artifacts.md).

### Non-goals

- `gh attestation verify` and the public attestations API continue to serve third-party discovery-by-digest unchanged. Cursus simply no longer depends on them at install time.
- cargo-binstall installation ([ADR-054](054-cargo-binstall-support.md)) remains TLS-only unless revisited in a separate decision.
- Tag-object or commit signing concerns are out of scope; this ADR is about artifact verification only.

## Consequences

### Positive

- Cursus releases become token-free verifiable on any platform, not just GitHub. The unverified-download gap that affects consumers on GitLab CI and other non-GitHub platforms can be closed by adopting the new component.
- All channels stop depending on the 60-request-per-hour unauthenticated attestations API at install time. The shared-IP CI rate-limit fragility that motivated the GitHub action's token requirement is removed.
- The GitHub `setup-cursus` action no longer needs a token for verification, and the npm postinstall no longer makes a GitHub API call — both fetch the bundle over the same CDN path as the binary.
- There is one verification source across npm, GitHub Actions, and GitLab CI, reducing per-forge verification divergence and the surface that must be reasoned about for the trust chain.
- No new signing infrastructure, no new keys, and no change to the Fulcio/Rekor/OIDC trust root. The change is confined to release-asset upload, consumer transport, and a new GitLab component.

### Negative

- Each release carries one additional `.sigstore.json` asset per binary, and the release workflow gains an upload step per artifact.
- A correct verification now depends on the bundle asset being present and correctly named on the Release. A missing or misnamed bundle hard-fails verification even when the binary is fine — the same hard-fail trade [ADR-049](049-signed-release-artifacts.md) already accepts, now extended to the bundle asset.
- The GitLab component is a new artifact to maintain and version in a separate `zantarix/components` project, with its own release cadence in the GitLab catalog.
- The pinned SAN and cosign minimum version (v2.4.0+) are encoded in three places — the npm postinstall, the GitHub action, and the GitLab component. A workflow rename or a bundle-format change requires a coordinated update across all three.

### Neutral

- The trust root, the Sigstore primitives, the pinned SAN and OIDC issuer, and the hard-fail philosophy are all unchanged from [ADR-049](049-signed-release-artifacts.md) and [ADR-051](051-bundle-sigstore-deps-via-workspace-removal.md). This ADR changes only where the bundle bytes come from.
- The GitLab component uses cosign rather than the npm channel's `sigstore-js` verifier. The two verifiers consume the same bundle and enforce the same identity policy; only the implementation differs, chosen per environment.
- Direct downloaders from the GitHub Releases page may now verify token-free with cosign and the published bundle, where previously they needed `gh attestation verify`. This is an incidental improvement, not a guaranteed install-path behaviour.

## Alternatives Considered

### Fetch the bundle from the public attestations API at runtime

Keep the bundle on the attestations API and have every consumer fetch it by digest at verification time. Rejected: the 60-request-per-hour unauthenticated per-IP limit on shared CI runners is exactly the fragility this ADR removes, and discovery-by-digest is unnecessary because cursus controls its own distribution and can co-locate the bundle with the binary.

### Publish a separately-signed SHA256SUMS manifest

Generate a checksum manifest and sign it separately with cosign or minisign. Rejected: this introduces a new artifact and a new signing path that duplicates trust already provided by the existing attestations, and [ADR-049](049-signed-release-artifacts.md) already rejected bare checksum manifests because an unauthenticated manifest is defeated by the same attacker who can tamper with the binary.

### Mirror releases to GitLab Releases with GitLab-native verification

Mirror cursus binaries to GitLab Releases and verify them with a GitLab-native mechanism. Rejected: this stands up a second distribution channel to maintain and reintroduces per-forge verification divergence, the opposite of the single-source consolidation this ADR achieves.

### A `cursus self-verify` subcommand

Add a subcommand to cursus that verifies a downloaded cursus binary. Rejected: it is a circular bootstrap problem — verifying cursus would require running an as-yet-unverified cursus binary.

### Reuse the npm `sigstore-js` verifier inside the GitLab component

Have the GitLab component run the same `sigstore-js` verifier the npm channel uses. Not chosen: that requires Node.js in the GitLab job, whereas cosign is a single static, language-agnostic binary that is straightforward to pin in CI. The two verifiers consume the same bundle and enforce the same policy, so the choice is purely about minimizing the GitLab job's runtime footprint.
