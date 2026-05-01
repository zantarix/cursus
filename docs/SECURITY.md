# Security

## Reporting a vulnerability

Please report security issues **privately** using
[GitHub's private vulnerability reporting](https://github.com/zantarix/cursus/security/advisories/new)
rather than filing a public issue. This gives us a chance to review and prepare
a fix before any details are public.

**Supported versions:** only the latest release on `main` is supported. Older
tags do not receive backports.

**Response time:** this is a small project maintained on a best-effort basis.
We will acknowledge reports as quickly as we can, but we cannot guarantee a
specific SLA.

## Verifying release artifacts

Every binary on the [GitHub Releases page](https://github.com/zantarix/cursus/releases)
is signed with a Sigstore-backed attestation produced by GitHub Actions. The
`@zantarix/cursus` npm package verifies the attestation in-memory during
`postinstall` before the binary is written to disk or made executable.

For a complete description of the verification chain, the Subject Alternative
Name pin, and manual audit steps (including `gh attestation verify` and
`npm audit signatures`), see:

- [`docs/adr/049-signed-release-artifacts.md`](adr/049-signed-release-artifacts.md) —
  the full verification sequence and trust roots.
- [`docs/adr/051-bundle-sigstore-deps-via-workspace-removal.md`](adr/051-bundle-sigstore-deps-via-workspace-removal.md) —
  how the npm-side sigstore dependency is bundled and how to confirm its
  integrity independently.

A user-facing summary is in the
[installation docs](https://zantarix.github.io/cursus/getting-started/installation/).

## Scope

**In scope:** code under `packages/cursus/`, `packages/cursus-bin/`,
`packages/npm/`, and the release workflows under `.github/workflows/`. This
includes, but is not limited to:

- Command injection or argument smuggling via changeset frontmatter, branch
  names, commit messages, or `Cargo.toml`/`package.json` fields used in
  subprocess invocations.
- Path traversal via changeset filenames or untrusted config values.
- Token leakage (GitHub PAT, `CARGO_REGISTRY_TOKEN`, `NPM_TOKEN`) through
  logs, error messages, PR bodies, or subprocess arguments.
- Bypassing the postinstall attestation verification in `packages/npm/`.
- Bypassing the dry-run guard or the signed-commit path.

**Out of scope:** vulnerabilities that are purely in the upstream behaviour of
third-party dependencies — please report those upstream. However, if cursus's
own dependency pinning, bundling (see ADR-051), or invocation of a dependency
creates an exploitable condition that the upstream project would not own, please
report that here.
