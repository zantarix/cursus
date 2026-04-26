# ADR-048: Build Windows Artifacts Natively with MSVC and Static CRT

## Status

Proposed

## Context

Cursus distributes statically linked binaries for seven OS/architecture targets ([ADR-000](000-founding-constraints.md), [ADR-022](022-distribution-strategy.md)). Static linkage is a founding constraint and is non-negotiable; the toolchain used to achieve it is incidental.

Until recently, all seven targets have been produced via `cargo-zigbuild` from a single Linux runner, including the two Windows targets `x86_64-pc-windows-gnullvm` and `aarch64-pc-windows-gnullvm`. macOS targets were already migrated to a native `macos-latest` GitHub-hosted runner in `release-artifacts.yml` after a similar class of cross-compilation failure; Windows now needs the same treatment.

### The break

Windows cross-compilation via `cargo-zigbuild` has stopped working. The linker (`lld-link`) reports undefined symbols (`___chkstk_ms`, `__floattidf`, `__umodti3`, `__truncsfhf2`, `__extendhfsf2`, `__fixdfti`, `__fixunsdfti`, `sincos`, and others) even though `nm` confirms those symbols are present in the `compiler_builtins` rlib that Rust ships. The most probable root cause is that `cargo-zigbuild`'s `zig cc` driver does not translate Rust's `--whole-archive` linker directive (used to force-load `compiler_builtins`) into the `/WHOLEARCHIVE:` form that `lld-link` expects, so the relevant archive members are never pulled in.

### The bug is structural, not environmental

Bisection ruled out version-pinning fixes:

- Pinning zig back to 0.15.2 (from 0.16.0) did not fix the build.
- Pinning the Rust nightly back to 2026-04-21 (from 2026-04-24) produced a different but equivalently-shaped set of undefined-symbol errors.
- `cargo-zigbuild` itself was 0.22.1 in both nixpkgs revisions tested.

The most likely trigger that surfaced the latent bug was `fluent-templates 0.14.0` introducing `f64↔i128/u128` conversions in `fluent_bundle`, which exercised compiler builtins that prior versions of the dependency tree did not touch. The underlying defect is in how `cargo-zigbuild` drives the Windows linker, not in any specific zig, Rust, or `fluent-templates` version.

### Windows is not a supported `cargo-zigbuild` target

`cargo-zigbuild`'s own README states explicitly that "Currently only Linux and macOS targets are supported." Windows cross-compilation through `cargo-zigbuild` was never officially supported and worked only by chance. There is no commitment to fix the linker driver, and "pull requests are welcome" is not a credible recovery path for an active release pipeline.

Linux cross-compilation via `cargo-zigbuild` (the musl targets `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, and `riscv64gc-unknown-linux-musl`) is officially supported and continues to work; this ADR does not affect those builds.

### The macOS precedent

The same class of failure previously broke macOS cross-compilation. The fix was to build macOS artifacts natively on a `macos-latest` GitHub-hosted runner instead of cross-compiling. That pattern has been stable since adoption and is the template this ADR follows for Windows.

### Distribution constraint

[ADR-022](022-distribution-strategy.md) currently describes the static-link mechanism on Windows as "GNULLVM." That mechanism is failing. The static-binary promise itself ([ADR-000](000-founding-constraints.md), [ADR-022](022-distribution-strategy.md)) is what must be preserved; the toolchain that produces the binary is an implementation detail.

## Decision

We will build Windows release artifacts natively on `windows-latest` GitHub-hosted runners using the MSVC toolchain with a statically linked CRT, dropping the `*-pc-windows-gnullvm` targets from the build matrix entirely.

### Targets

The two Windows targets become:

- `x86_64-pc-windows-msvc` -- host-native build on `windows-latest`.
- `aarch64-pc-windows-msvc` -- cross-compiled from the same `windows-latest` runner using MSVC's bundled aarch64 cross-toolset, which Microsoft officially supports and which `windows-latest` ships with.

### Static linkage

Builds will set `RUSTFLAGS="-C target-feature=+crt-static"`, statically linking the MSVC C runtime into the produced `.exe`. The resulting binary has no runtime DLL dependency on `MSVCP*.dll`, `VCRUNTIME*.dll`, or any other Microsoft redistributable. This satisfies the static-binary promise of [ADR-000](000-founding-constraints.md) and [ADR-022](022-distribution-strategy.md).

### Toolchain provisioning

The `windows-latest` job will install Rust via `dtolnay/rust-toolchain@nightly` directly. The Nix dev shell is not involved on Windows (the project's flake only supports `x86_64-linux`, `aarch64-linux`, and `aarch64-darwin` -- it never claimed Windows support).

### Removal of GNULLVM targets

The `x86_64-pc-windows-gnullvm` and `aarch64-pc-windows-gnullvm` targets are removed from:

- the Nix flake's target list,
- the `cargo make` task graph (`release-windows-x86_64`, `release-windows-aarch64`),
- the cursus artifact configuration,
- any CI workflow references.

The `release-windows-x86_64` and `release-windows-aarch64` Makefile tasks are updated to invoke `cargo build --release` against the MSVC targets with `+crt-static`, so they remain runnable on a Windows development machine. They are documented as Windows-only tasks; running them from the Nix dev shell on Linux or macOS is not supported and not attempted.

### CI workflow changes

- `release-artifacts.yml` gains a new `windows` job that mirrors the existing `macos` job's structure: a `windows-latest` runner builds both Windows targets and uploads their artifacts. Linux artifacts continue to be produced by the existing zigbuild-based job.
- `smoke-test.yml` gains a new `release-windows` job that exercises the same Windows build path used in releases.
- The existing `smoke-test` job in `smoke-test.yml` gains a per-platform static-linkage assertion step: Linux runs `ldd` against the binary and requires it to fail (indicating no dynamic dependencies); macOS runs `otool -L` and requires every listed library to live under `/usr/lib/` or `/System/Library/`; Windows runs `dumpbin /dependents` and requires no CRT DLLs (`MSVCP*.dll`, `VCRUNTIME*.dll`, `api-ms-win-crt-*.dll`) to appear. Static linkage thereby becomes a CI-enforced invariant rather than an assumed property of the toolchain.

**Linux is unchanged.** `cargo-zigbuild` continues to produce the three musl Linux artifacts on a Linux runner; this is officially supported and not affected by this decision.

### Relationship to ADR-022

This ADR amends [ADR-022](022-distribution-strategy.md)'s description of the Windows static-link mechanism from GNULLVM to MSVC + `+crt-static`. The distribution channels, artifact naming, postinstall download flow, and version-synchronization rules of [ADR-022](022-distribution-strategy.md) are unaffected. An erratum will be added to [ADR-022](022-distribution-strategy.md) once this ADR is accepted, pointing forward to it.

## Consequences

### Positive

- Windows builds run on a structurally supported toolchain (MSVC native) with no reliance on undocumented `cargo-zigbuild` behaviour.
- MSVC is the idiomatic Windows ABI; combined with `+crt-static`, the resulting `.exe` is genuinely self-contained and requires no Visual C++ Redistributable on the target machine.
- The fix mirrors the proven macOS pattern (use a native runner for the platform that cannot be cross-compiled), keeping the CI architecture coherent across platforms.
- Static linkage becomes a CI-enforced invariant via the new smoke-test assertion step, rather than an assumed property that silently regresses if the toolchain changes.
- Removing the GNULLVM targets simplifies the Nix flake and `cargo make` task graph.

### Negative

- Adds `windows-latest` GitHub-hosted runner minutes to both `smoke-test.yml` and `release-artifacts.yml`. Windows runners are billed at a higher multiplier than Linux runners for private repositories.
- The `cargo make release-windows-*` tasks are no longer runnable from the project's Nix dev shell. They were never officially supported there (the flake's supported systems do not include Windows), but the previous zigbuild-based path made it incidentally possible to invoke them on Linux or macOS. After this change, those tasks are documented as Windows-only.
- The Windows binary ABI changes from GNULLVM to MSVC. End users running the static `.exe` see no functional difference, but anything that linked against or interoperated with the previous GNULLVM build at the binary level would need to switch. For a distributed CLI binary this is a non-issue; it is noted only for completeness.

### Neutral

- [ADR-022](022-distribution-strategy.md)'s artifact naming, distribution channels, and postinstall-download contract are unaffected; only the static-link mechanism description changes (handled via erratum on [ADR-022](022-distribution-strategy.md)).
- Linux musl builds via `cargo-zigbuild` are unaffected; `cargo-zigbuild`'s officially supported scope still serves the project where it works.
- The aarch64 Windows artifact is now produced via cross-compilation from an x86_64 Windows runner using Microsoft's bundled aarch64 cross-toolset. This is officially supported by Microsoft and avoids depending on the public-beta `windows-11-arm` runner.

## Alternatives Considered

### Pin zig to 0.15.2

Verified empirically: rebuilding with zig 0.15.2 (via the prior nixpkgs pin) did not fix the linker errors. The same class of undefined-symbol failures recurred. Rejected because the bug is not version-specific.

### Pin an older Rust nightly

Verified empirically: pinning the Rust nightly back to 2026-04-21 produced a different specific set of errors but the same structural failure mode (undefined symbols from `compiler_builtins`). Rejected because the defect is in `cargo-zigbuild`'s linker driver, not in any particular Rust version.

### Wait for `cargo-zigbuild` to officially support Windows

`cargo-zigbuild`'s README says "pull requests are welcome" with no timeline commitment, and Windows is currently outside its supported scope. Rejected because there is no credible delivery path for an actively-released project to wait on a third-party fix to an unsupported target.

### Use the `windows-11-arm` runner for the aarch64 build

GitHub offers a `windows-11-arm` runner that would let aarch64 build natively rather than via cross-compile. Rejected because it adds extra runner cost, depends on a runner currently in public beta, and offers no advantage over MSVC's officially-supported aarch64 cross-toolset, which `windows-latest` ships with.

### Use MinGW (`x86_64-pc-windows-gnu`)

The MinGW-based GNU toolchain is the same ABI family as the failing GNULLVM target. Rejected because it would not address the root cause (Windows linker driver bugs in cross-compile setups), it produces a less idiomatic Windows binary, and it requires separate libgcc handling.

### Drop Windows from supported targets

Removing Windows support entirely would sidestep the build problem. Rejected because it directly violates the distribution promise in [ADR-022](022-distribution-strategy.md) and the multi-platform reach goal in [ADR-000](000-founding-constraints.md).
