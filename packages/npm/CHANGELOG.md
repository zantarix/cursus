# Changelog

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

