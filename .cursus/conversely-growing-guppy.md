+++
"@zantarix/cursus" = "patch"
cursus = "patch"
cursus-bin = "patch"
+++

Fixes Windows release binaries, which were failing to build due to a linker incompatibility in the cross-compilation toolchain. Windows binaries are now built natively using the MSVC toolchain with a statically linked CRT, producing self-contained executables with no runtime DLL dependencies.
