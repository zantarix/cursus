# ADR-053: Use a Node.js Spawner Script for the npm Package Binary Entry Point

## Status

Accepted (2026-05-02)

## Context

[ADR-022](022-distribution-strategy.md) established the npm distribution channel for cursus as a postinstall-download model: the published `@zantarix/cursus` tarball contains a placeholder script at `bin/cursus` and a `package.json` whose `bin` field points at that placeholder. At install time, the postinstall script downloads the matching native binary from the corresponding GitHub release (verified against a Sigstore attestation per [ADR-049](049-signed-release-artifacts.md)) and writes it to disk. ADR-022's "No Node.js wrapper" decision called for the `package.json` `bin` entry to point directly at the native binary so that `node_modules/.bin/cursus` invokes the binary without a wrapper process — the binary *is* the executable.

On Unix this works correctly: npm creates a symlink at `node_modules/.bin/cursus` pointing at `bin/cursus`, postinstall overwrites `bin/cursus` with the native binary, and the symlink resolves to the native binary at runtime. Signal handling, exit codes, and stdio all behave as if the user invoked the binary directly.

Issue #117 surfaced that this model is broken on Windows. After `npm install @zantarix/cursus` succeeds, running `./node_modules/.bin/cursus` fails with `cursus: native binary is not installed.` — the placeholder's error message — even though the native binary has been downloaded successfully and exists on disk.

The root cause is in how npm creates Windows shims. Windows has no symlinks for executables in this context; npm uses the `cmd-shim` package to generate `.cmd`, `.ps1`, and bash shims at `node_modules/.bin/cursus.cmd` (etc.) that proxy invocations to the target. Reading the `cmd-shim` source confirms that it inspects the target file's **shebang line at install time**, not the target's file extension, to decide what kind of shim to write:

- If the target has a `#!/usr/bin/env node` shebang, `cmd-shim` writes shims that invoke `node <target>`.
- If the target has no shebang, `cmd-shim` writes shims that invoke the target directly as a native executable.

The placeholder script shipped in the npm tarball begins with `#!/usr/bin/env node` because it is a Node.js script that prints the "native binary is not installed" message. At install time — before postinstall runs — npm sees the shebang and writes node-invoking shims. Postinstall then overwrites `bin/cursus` with the native binary contents and (in the prior implementation) mutated `package.json.bin` to `bin/cursus.exe`, but **npm does not re-run shim creation after postinstall**. The `.cmd` shim continues to invoke `node bin/cursus`, which now points at a binary file that no longer parses as JavaScript — except that in some configurations the shim still resolves the original placeholder path, prints the error, and exits.

Either way, the underlying defect is the same: the shim shape is locked in at install time based on the placeholder's shebang, and there is no way to "upgrade" a node-invoking shim into a direct-call shim from a postinstall script without re-running the shim creation step that npm itself controls.

The fix space has three shapes:

1. Make the placeholder shim-compatible as a native executable (rename to `bin/cursus.exe`, drop the shebang) so `cmd-shim` writes direct-call shims at install time, and have postinstall overwrite the file in place.
2. Lean into the node-invoking shim that `cmd-shim` already creates and ship a real Node.js spawner script as the bin target on Windows.
3. Lean into the node-invoking shim on **all** platforms and ship a single Node.js spawner regardless of OS.

`@apollo/rover`, the closest precedent for shipping a Rust binary through an npm postinstall-download model, uses approach (3): a small Node.js spawner script always, on every platform, that `spawn`s the native binary and forwards signals.

Cursus's primary distribution target is CI environments. The npm channel exists to make cursus discoverable for Node.js workspace users; it is not the performance-sensitive path. Users who care about startup overhead have direct alternatives: the `zantarix/actions/setup-cursus` GitHub Action, manual download from GitHub Releases, and the cargo-binstall path established by [ADR-054](054-cargo-binstall-support.md) for the Rust ecosystem. The ~50ms node-startup overhead per invocation is acceptable on the npm channel.

## Decision

We will replace the "binary as bin target" model from [ADR-022](022-distribution-strategy.md) with a Node.js spawner script as the `package.json.bin` target on **all platforms**, including Unix.

The published npm tarball will retain the existing placeholder at `bin/cursus.js` and will additionally bundle a pre-written, source-controlled Node.js spawner script at `bin/cursus.shim.js`. Both files are static, known at publish time, and committed to the repository — the spawner is **not** generated dynamically at install time. The placeholder will continue to print the "native binary is not installed" error message and exit with a non-zero status; it serves as a graceful fallback if postinstall fails before the spawner has been copied into place.

The `package.json.bin` field will point at `bin/cursus.js` (renamed from `bin/cursus` for clarity, since the file is always a JavaScript ESM module — either the placeholder or, after postinstall, the spawner).

Postinstall will be modified as follows. The native binary will be downloaded and verified per the existing flow ([ADR-049](049-signed-release-artifacts.md)) and written to a sibling path next to the bin entry: `bin/cursus.exe` on Windows, `bin/cursus-bin` on Unix. Postinstall will then **copy `bin/cursus.shim.js` over `bin/cursus.js`**, replacing the placeholder with the pre-written spawner, and **chmod the resulting `bin/cursus.js` to mode `0755`**. The chmod step is required because the TypeScript compiler emits files with mode `0644` and `cp` preserves source permissions; on Unix, npm's `node_modules/.bin/cursus` symlink inherits the target's permissions, so without the explicit chmod the symlinked invocation fails with "permission denied". The `package.json.bin` field will continue to point at `bin/cursus.js` and will not be mutated by postinstall.

The spawner script (`bin/cursus.shim.js`) shall:

- Begin with `#!/usr/bin/env node` so that `cmd-shim` produces node-invoking shims on Windows (the desired behaviour given that the spawner *is* a Node.js script). Because the placeholder at `bin/cursus.js` shares this shebang, the shim shape that `cmd-shim` locks in at install time matches what the spawner needs at runtime.
- Resolve the path to the sibling native binary relative to its own location (`__dirname`-style resolution) so it works regardless of where the package is installed in the dependency tree.
- Use runtime `process.platform` detection to choose between the `bin/cursus.exe` (Windows) and `bin/cursus-bin` (Unix) sibling paths, so a single static spawner file works on all platforms without needing per-platform variants.
- Use the asynchronous `child_process.spawn` API (not `spawnSync`) with `stdio: "inherit"` so stdin, stdout, and stderr are passed through transparently.
- Forward `SIGTERM`, `SIGINT`, and `SIGHUP` from the spawner process to the child native binary so that container orchestrators (which use `SIGTERM` for graceful shutdown) and interactive users (Ctrl+C) get the expected behaviour.
- Exit with the child process's exit code; if the child was terminated by a signal, the spawner shall exit with the conventional `128 + signal_number` code.

The spawner is also responsible for surfacing the "native binary is not installed" error message at runtime if the sibling binary is missing or not executable, replacing the placeholder's role for that case.

This decision supersedes the "No Node.js wrapper" portion of [ADR-022](022-distribution-strategy.md) for the npm distribution channel. All other aspects of [ADR-022](022-distribution-strategy.md) — version synchronization, hard-fail on download failure, the seven-target platform matrix, registry scope, and publish ordering — are unchanged. The Sigstore attestation verification flow established by [ADR-049](049-signed-release-artifacts.md) is unchanged: the spawner is written only after the attestation check has succeeded.

## Consequences

### Positive

- The Windows install path is fixed. `cmd-shim` writes node-invoking shims based on the placeholder's shebang at install time, and after postinstall the bin target *is* a Node.js script, so the shim's invocation is correct.
- A single code path in the postinstall logic and a single shape for `bin/cursus.js` across all platforms. The `download-binary.ts` postinstall script does not need to branch on `process.platform` for the bin layout.
- Signal handling is uniform across platforms. The spawner explicitly forwards `SIGTERM`, `SIGINT`, and `SIGHUP`, which is the correct behaviour for container and CI environments where `SIGTERM` is the standard early-termination signal.
- `package.json.bin` is no longer mutated by postinstall, eliminating a class of subtle bugs where a partially-completed install leaves `package.json` and the filesystem out of sync.
- Aligns with the established precedent set by `@apollo/rover` for shipping Rust binaries via the npm postinstall-download pattern, reducing the chance of encountering further unknown ecosystem footguns.
- The placeholder retains its role as a graceful failure surface if postinstall is interrupted between the binary download and the spawner write.

### Negative

- Reverses the "No Node.js wrapper" decision in [ADR-022](022-distribution-strategy.md). Every invocation of cursus through the npm channel now pays a Node.js startup cost (approximately 50ms) and runs through an extra process boundary.
- Introduces a small amount of JavaScript code (the spawner) that must be maintained, tested, and kept in sync with the native binary's signal-handling expectations. Bugs in the spawner manifest as runtime failures rather than install-time failures.
- The spawner adds a parent process to every cursus invocation, which is visible in process listings and can confuse users debugging cursus's behaviour from outside the process.
- Exit-code translation for signal-terminated children (`128 + signal`) is a convention rather than a guarantee; tools that distinguish between "exit code N" and "killed by signal N" through other channels (e.g., `WIFSIGNALED`) will see the spawner's exit-code translation rather than the underlying signal.

### Neutral

- The npm package's installed footprint on disk grows by the spawner script (a few hundred bytes) and now contains both `bin/cursus.js` (the spawner, copied from `bin/cursus.shim.js` at install time) and a sibling native binary file. Total size is dominated by the native binary in either model.
- Users who download the binary directly from GitHub Releases or install via [cargo-binstall](054-cargo-binstall-support.md) or `zantarix/actions/setup-cursus` are unaffected. The spawner only sits in front of invocations made through the npm package's bin entry.
- The `bin/cursus-bin` (Unix) and `bin/cursus.exe` (Windows) sibling paths are an internal implementation detail. They are not part of the npm package's public API and are not guaranteed to remain stable across cursus releases.

## Alternatives Considered

### Rename the bin target to `bin/cursus.exe` and ship a placeholder without a shebang

`cmd-shim` uses the absence of a shebang line as the trigger for direct-call shims, regardless of file extension. A placeholder file with no shebang and a `bin/cursus.exe` filename would cause `cmd-shim` to write direct-call shims at install time, and postinstall could overwrite the placeholder with the native binary in place — preserving the original [ADR-022](022-distribution-strategy.md) "no Node.js wrapper" intent.

This was rejected for several reasons. First, the placeholder loses its ability to print a friendly error message: a JavaScript file without a shebang cannot be executed directly, and a non-executable placeholder on Windows would produce a confusing "not a valid Win32 application" error if postinstall fails. Second, no large npm project uses this pattern; it relies on a `cmd-shim` implementation detail (shebang-based shim selection) that is undocumented. Third, naming a file `bin/cursus.exe` on Unix is unusual and may confuse tooling, audit scripts, or users browsing the package contents.

### Per-platform optional npm dependencies

Publish separate platform-specific packages (`@zantarix/cursus-win32-x64`, `@zantarix/cursus-linux-x64`, etc.) and have the umbrella `@zantarix/cursus` package declare them as optional dependencies with `os` and `cpu` fields. This is the approach used by esbuild, Biome, and SWC. It is the cleanest architectural answer because it avoids both postinstall scripts and runtime spawning entirely.

This was already explicitly rejected in [ADR-022](022-distribution-strategy.md) on the grounds that publishing seven-plus packages per release significantly increases publishing-pipeline complexity and the surface area for version drift. That trade-off has not changed in the intervening time. If cursus chooses to adopt this pattern in the future, it should be done as a full supersession of [ADR-022](022-distribution-strategy.md), not as a patch on top of the postinstall-download model.

### Windows-only Node.js spawner

Apply the spawner only on Windows; keep the Unix install path pointing the bin entry directly at the native binary as [ADR-022](022-distribution-strategy.md) originally specified. This minimises deviation from the existing decision and preserves the no-overhead Unix invocation path.

This was rejected on consistency and maintenance grounds. Two code paths in `download-binary.ts`, two shapes of `bin/cursus`, and two different signal-handling stories per platform increase the cognitive load on anyone maintaining the npm distribution code. The ~50ms Node.js startup overhead is acceptable for the npm channel given cursus's primary use as a CI tool and the availability of lower-overhead alternative install paths. The marginal benefit of skipping the spawner on Unix does not justify the dual-implementation cost.

### Synchronous spawn (`child_process.spawnSync`)

Use the synchronous `spawnSync` API instead of `spawn`. This would simplify the spawner code by avoiding manual signal-forwarding and exit-code propagation; the parent process would block on the child and exit with the child's status automatically.

This was rejected because `spawnSync` does not propagate signals to the child process. A `SIGTERM` delivered to the spawner during a `spawnSync` call terminates the spawner without giving the child a chance to clean up, which is exactly the failure mode that container orchestrators trigger during graceful shutdown. The asynchronous `spawn` API with explicit signal forwarding gives the child the chance to handle termination signals correctly.
