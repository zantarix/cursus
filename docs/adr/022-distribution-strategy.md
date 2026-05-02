# ADR-022: Distribution Strategy for Cursus Binaries

## Status

Accepted

## Context

Cursus is a Rust CLI tool that builds static binaries for seven OS/architecture targets: x86_64-linux, aarch64-linux, riscv64-linux, x86_64-macos, aarch64-macos, x86_64-windows, and aarch64-windows. All binaries are produced via cargo-zigbuild and are fully statically linked (musl on Linux, GNULLVM on Windows).

Cursus already manages its own releases through its own `publish` workflow. The `[github]` configuration ([ADR-005](005-github-releases.md)) builds all seven targets via `cargo make release` and attaches them as artifacts to each GitHub release. This means the binaries are already being produced and uploaded -- the question is how users discover and install them.

The primary audience for Cursus is developers working in repositories that use it for release management. These developers need a way to install Cursus that is fast, requires no build toolchain, and works across platforms. Two distinct user populations exist:

1. **Rust developers** who already have `cargo` and could use `cargo install`, but would prefer a faster option that does not require compilation.
2. **Node.js developers** who work in repositories that use Cursus for npm package releases but do not have a Rust toolchain installed.

The Node.js ecosystem is a particularly important distribution target because Cursus supports npm/yarn/pnpm workspaces as a first-class package manager ([ADR-009](009-javascript-package-manager-strategy.md)). Developers in these projects expect to install tools via `npm install` or `npx`.

The challenge is choosing distribution channels that maximize reach without taking on excessive packaging and maintenance burden. Each channel has different trade-offs around discoverability, installation speed, platform coverage, maintenance cost, and user expectations.

## Decision

We will distribute Cursus through two channels: GitHub Releases as the primary channel, and an npm package as a secondary channel optimized for Node.js ecosystem discoverability.

**GitHub Releases (primary channel).** Static binaries for all seven supported targets are attached to each GitHub release. This is already implemented via [ADR-005](005-github-releases.md) and requires no additional work. GitHub Releases serve as the canonical source of binaries for all distribution channels and for direct download by users who prefer manual installation.

**npm package (secondary channel).** Cursus will be published to the npmjs registry as `@zantarix/cursus`, a scoped package under the Zantarix organization. The npm package will not bundle any binaries. Instead, it will use a postinstall script that downloads the correct platform-specific binary from the corresponding GitHub release at install time.

The postinstall script will:

1. Detect the user's operating system and CPU architecture using Node.js `os.platform()` and `os.arch()`.
2. Map the detected platform to the corresponding GitHub release artifact name (e.g., `darwin` + `arm64` maps to `cursus-osx-aarch64`).
3. Download the binary from the GitHub release whose version tag matches the npm package version.
4. Place the binary at a known path within the package directory.
5. Fail the install with a clear error message if the platform is unsupported or the download fails.

**The npm package version will always match the Rust crate version exactly.** Cursus's own `publish` command handles both the Cargo and npm publishes in a single invocation, ensuring version synchronization.

**Publish ordering.** The intended publish order is: Cargo crate to crates.io, then npm package to npmjs, then GitHub release creation with binary uploads. This ordering means the GitHub release (and therefore the binaries) does not exist yet when the npm package is published. The postinstall script runs when a user later installs the package, not at publish time, so the binaries will be available by then. This ordering is subject to change in future ADRs.

**Hard failure on postinstall.** If the postinstall binary download fails for any reason (network error, unsupported platform, missing GitHub release), `npm install` will fail. A missing binary would make the package non-functional, and a silent degradation would produce confusing errors when the user later tries to run `cursus`. A clear, immediate failure is preferable.

**No Node.js wrapper.** The npm `bin` entry will point directly to the downloaded native binary, not to a Node.js script that spawns the binary as a child process. A wrapper would interfere with signal handling (SIGINT, SIGTERM), add startup latency, and introduce unnecessary complexity. The binary is the executable.

**All supported targets are handled uniformly.** The postinstall script will include mappings for all seven targets that Cursus builds. There is no tiered support -- riscv64-linux and aarch64-windows are handled identically to x86_64-linux. If a user's platform does not match any supported target, the script will fail with an error message listing the supported platforms and directing the user to the GitHub release page for manual download.

**Registry scope.** Day-one target is the main npmjs registry only. The `@zantarix` scope leaves the door open for publishing to other registries (GitHub Packages, private registries) in the future without name conflicts.

**The npm package is marked `private: true` during development.** Cursus's own `publish` command and the npm `PackageManagerAdapter` already understand the `private` field and will handle the transition to publishable status as part of the implementation work.

## Consequences

### Positive

- GitHub Releases provide a universal, registry-agnostic download mechanism that works for any user on any platform, regardless of their language ecosystem. No toolchain or package manager is required.
- The npm package makes Cursus discoverable and installable for the large Node.js developer population that Cursus directly serves through its npm workspace support. `npx @zantarix/cursus` works without any global installation.
- The postinstall-download pattern keeps the npm package tiny (a few kilobytes of JavaScript) regardless of how many platform targets are supported. This avoids the npm registry size limits and download overhead that come with bundling binaries.
- Version synchronization is guaranteed by Cursus's own publish workflow -- the same tool that manages other projects' releases also manages its own, ensuring the Cargo crate, npm package, and GitHub release always have matching versions.
- Hard failure on postinstall prevents users from ending up in a broken state where the package is installed but non-functional.
- Pointing the npm `bin` entry directly at the native binary avoids signal handling issues, startup overhead, and the maintenance burden of a Node.js wrapper process.

### Negative

- The npm distribution channel requires network access to GitHub at install time, not just at `npm install` resolution time. Users in air-gapped or restricted network environments cannot install via npm. They must download the binary directly from the GitHub release or use an alternative method.
- The postinstall-download pattern is a runtime dependency on GitHub's availability. If GitHub is down or rate-limiting when a user runs `npm install`, the install will fail. This is mitigable by retrying, but is inherent to the approach.
- The publish ordering (Cargo, then npm, then GitHub release) creates a window where the npm package exists on the registry but the GitHub release it depends on has not yet been created. In practice this window is seconds to minutes, but a user who installs the npm package in this interval will get a postinstall failure. Re-running `npm install` after the GitHub release is created will succeed.
- Maintaining the platform-to-artifact mapping in the postinstall script requires manual updates when targets are added or removed. This is a small but real coordination cost.
- The npm package depends on the GitHub release artifact naming convention remaining stable. Changes to artifact names in the `[github.artifacts]` config would break the postinstall script if not updated in tandem.

### Neutral

- The postinstall-download approach is a well-established pattern in the npm ecosystem, used by tools like esbuild, Playwright, and Puppeteer. Users and CI systems are accustomed to postinstall scripts that download platform-specific binaries.
- `cargo install cursus` remains available as an installation method for Rust developers who prefer it, though it requires compilation. This ADR does not add or remove that option -- it exists by virtue of Cursus being published to crates.io.
- The npm package scaffolding already exists at `pkg/` with the `@zantarix/cursus` name, `bin` entry, and postinstall hook configured. The download script is currently a placeholder.

## Alternatives Considered

### Bundle all binaries in the npm package

Instead of downloading at install time, include all seven platform binaries directly in the npm tarball and select the correct one during postinstall. This was rejected because the combined binary size would be substantial (seven static binaries, each several megabytes), inflating download times for every user regardless of platform. The npmjs registry has package size limits that could become constraining as binary sizes grow. The postinstall-download approach transfers only the single binary the user actually needs.

### Per-platform npm optional dependencies

Publish separate platform-specific npm packages (e.g., `@zantarix/cursus-linux-x64`, `@zantarix/cursus-darwin-arm64`) and declare them as optional dependencies with `os` and `cpu` fields in their `package.json`. The main `@zantarix/cursus` package would then resolve the correct binary from whichever optional dependency was installed. This is the approach used by esbuild and SWC. It was rejected because it requires publishing and versioning eight packages (one per platform plus the umbrella package) for every release, significantly increasing the publishing complexity and the surface area for version drift. The simpler postinstall-download approach achieves the same user experience with a single package and no multi-package coordination.

### Homebrew formula

Distribute Cursus via a Homebrew tap for macOS and Linux users. This was rejected as a day-one channel because Homebrew taps require maintaining a separate repository with formula definitions, and the audience overlap with GitHub Releases is high (Homebrew users are comfortable downloading binaries). Homebrew may be added as a third channel in the future if there is sufficient demand, but it does not justify the maintenance cost at this stage.

### cargo-binstall support

Add metadata to `Cargo.toml` so that `cargo binstall cursus` can download pre-built binaries from GitHub Releases instead of compiling from source. This was not rejected -- it is complementary to this ADR and may be added later. However, it serves only the Rust ecosystem and does not address the Node.js discoverability goal that motivates the npm package. It is out of scope for this decision.

### Shell installer script (curl | sh)

Provide a shell script that users pipe to their shell to download and install the correct binary. This was rejected as a primary channel because it requires trusting a remote script, does not integrate with any package manager for updates or removal, and provides no version management. It may be useful as a convenience method documented in the README, but it is not a distribution *channel* in the same sense as a registry.

## Errata

### 2026-04-26: Windows static-link mechanism corrected

The Context section describes Windows binaries as being "produced via cargo-zigbuild and ... fully statically linked (musl on Linux, GNULLVM on Windows)." This is no longer accurate for Windows. Windows artifacts are now built natively on a `windows-latest` GitHub-hosted runner using the MSVC toolchain (`x86_64-pc-windows-msvc` host-native, `aarch64-pc-windows-msvc` cross-compiled from x86_64 via MSVC's bundled aarch64 cross-toolset) with `RUSTFLAGS="-C target-feature=+crt-static"` to statically link the MSVC C runtime. The resulting `.exe` has no runtime dependency on any Microsoft redistributable.

The static-binary promise, the seven-target matrix, the artifact naming convention, the postinstall download flow, and the version-synchronization rules of this ADR are unaffected; only the Windows toolchain description in the Context section is corrected. Linux musl builds still go through `cargo-zigbuild`. See [ADR-048](048-native-windows-build-runner.md) for the full rationale.

### 2026-04-27: Postinstall now verifies a Sigstore attestation before writing the binary

The postinstall download flow described in the Decision section establishes TLS as the only trust anchor on the binary that is downloaded from the matching GitHub Release. [ADR-049](049-signed-release-artifacts.md) extends that flow with an identity-pinned Sigstore attestation check: the postinstall script now downloads the binary into memory, fetches the matching attestation bundle from the unauthenticated GitHub attestations API, verifies the bundle's certificate chain, Rekor inclusion proof, signature, and Subject Alternative Name against a platform-keyed expected workflow identity, and only then writes the binary to its final install path and makes it executable.

The hard-fail philosophy of this ADR is preserved and extended: any verification failure (missing attestation, signature failure, identity mismatch, digest mismatch, or bundle-fetch transport error) is now an additional class of error that hard-fails the install. The platform-to-artifact mapping, the version-synchronization rule, the publish ordering, and the no-Node.js-wrapper decision are all unaffected. The npm package gains a runtime dependency (`sigstore`) and a second install-time GitHub network endpoint (the attestations REST API). See [ADR-049](049-signed-release-artifacts.md) for the full rationale and identity policy.

### 2026-05-02: No-Node.js-wrapper decision superseded for the npm package

The "No Node.js wrapper" paragraph in the Decision section above is superseded for the npm distribution channel by [ADR-053](053-npm-package-node-spawner.md). The decision to point `package.json.bin` directly at the native binary turned out to be incompatible with how npm's `cmd-shim` creates Windows shims at install time, causing `./node_modules/.bin/cursus` to fail on Windows after a successful install (issue #117). [ADR-053](053-npm-package-node-spawner.md) replaces the direct-binary bin target with a small Node.js spawner script on all platforms; the spawner forwards signals and exit codes to the native binary so the user-visible behaviour remains close to the original intent.

All other aspects of this ADR — the postinstall-download model, the seven-target platform matrix, version synchronization, hard-fail on download failure, registry scope, publish ordering, and the Sigstore attestation flow added by [ADR-049](049-signed-release-artifacts.md) — are unaffected. See [ADR-053](053-npm-package-node-spawner.md) for the full rationale.

### 2026-05-02: cargo-binstall support adopted

The "cargo-binstall support" alternative in the Alternatives Considered section above was deferred at the time this ADR was written ("This was not rejected — it is complementary to this ADR and may be added later"). That deferral has now been acted on by [ADR-054](054-cargo-binstall-support.md), which adds `[package.metadata.binstall]` metadata to `packages/cursus-bin/Cargo.toml` so that `cargo binstall cursus-bin` resolves to the prebuilt binaries this ADR established as the primary GitHub Releases artifacts. The seven-target matrix, the artifact naming convention, the `cursus@<version>` tag format, and the Sigstore attestation flow added by [ADR-049](049-signed-release-artifacts.md) are all unchanged; cargo-binstall consumes the same artifacts produced by the same release pipeline. See [ADR-054](054-cargo-binstall-support.md) for the integrity model and the Linux glibc → musl override rationale.
