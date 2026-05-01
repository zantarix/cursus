# Changelog

## 0.5.1 - 2026-05-01

### Bug Fixes

- Bundle sigstore transitive dependency tree into the published tarball via bundleDependencies (ADR-051) [131dc96] via #112
- Fixes token leakage where GitHub access tokens, registry credentials, and other URL-embedded secrets could appear in error messages produced by failed git operations or package publishes. [131dc96] via #112
- Rejects changeset files larger than 64 KiB and config.toml larger than 256 KiB to prevent out-of-memory conditions when processing maliciously oversized inputs. [131dc96] via #112
- Fixes security vulnerabilities in the npm postinstall download script: redirect targets are now validated against an allowlist of known GitHub domains, response sizes are bounded to prevent memory exhaustion, and GitHub API rate-limit errors include actionable retry guidance. [131dc96] via #112

## 0.5.0 - 2026-04-29

### Features

- version sync to 0.5.0 (linked versions)

## 0.4.0 - 2026-04-27

### Features

- version sync to 0.4.0 (linked versions)

## 0.3.2 - 2026-04-27

### Bug Fixes

- Adds `Project::is_releasable_under(&Config)`, `Project::is_prepared_for_release(&dyn Filesystem)`, and `package_manager::matching::match_files_to_projects` as public API. [3082f8a]

## 0.3.1 - 2026-04-27

### Bug Fixes

- Fixes attestation verification failing on Linux during `npm install @zantarix/cursus`. [698d77a]

## 0.3.0 - 2026-04-27

### Features

- Verifies the Sigstore build-provenance attestation of the downloaded binary during `npm install @zantarix/cursus`, confirming it was produced by the official release workflow before writing it to disk. [c453746]

## 0.2.3 - 2026-04-27

### Bug Fixes

- Fixes Windows release binaries, which were failing to build due to a linker incompatibility in the cross-compilation toolchain. Windows binaries are now built natively using the MSVC toolchain with a statically linked CRT, producing self-contained executables with no runtime DLL dependencies. [6de8fe6]

## 0.2.2 - 2026-04-25

### Bug Fixes

- version sync to 0.2.2 (linked versions)

## 0.2.1 - 2026-04-19

### Bug Fixes

- version sync to 0.2.1 (linked versions)

## 0.2.0 - 2026-04-19

### Features

- Output from the configured build command and npm lock command is now streamed live to the terminal as the command runs, rather than buffered until completion. Long-running builds no longer appear to hang with no output. [ac67ec6]

## 0.1.1 - 2026-04-19

### Bug Fixes

- Logs the filename of the created changeset after running `cursus change`. [9ce35b8]
- Fixes npm package binary download failing due to incorrect release tag format. [ad7ef84]
- Expose crates.io trusted publishing as a viable option for publishing crates [9a13b99] via #87

## 0.1.0 - 2026-04-19

### Features

- version sync to 0.1.0 (linked versions)

