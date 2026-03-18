# ADR-033: Extend Command Execution to Support Windows

## Status

Accepted

## Context

Cursus cross-compiles and ships Windows binaries via `cargo make release-windows-x86_64` and `cargo make release-windows-aarch64`. However, [ADR-011](011-command-execution-strategy.md) established command execution conventions that are Unix-specific: `RealCommandRunner::run_shell` and `run_shell_interactive` were hardcoded to `/bin/sh -c`, `shell_quote` used POSIX single-quote escaping, and `find_default_editor` probed via `which` and defaulted to Unix editors (`nano`, `vim`, `vi`, `emacs`). All of these would fail or produce incorrect behavior on Windows.

[ADR-011](011-command-execution-strategy.md) explicitly scoped Cursus's supported platforms as Linux and macOS (per the Nix flake: `x86_64-linux`, `aarch64-linux`, `aarch64-darwin`). The addition of Windows cross-compilation targets means the command execution layer must handle a platform where `/bin/sh` does not exist, single-quote escaping is not the shell convention, and `which` is not available.

The Nix flake development environment remains Unix-only. Windows support is a cross-compilation target, not a development host. This means Windows-specific code paths cannot be exercised natively during development and must be testable on the build host.

## Decision

We will extend the command execution and shell-quoting infrastructure to support Windows alongside the existing Unix behavior. The changes span four areas:

### Shell program and flag helpers

Two `pub(crate)` helpers will be added to the command module:

- `shell_program()` returns `"cmd.exe"` on Windows and `"/bin/sh"` on Unix.
- `shell_flag()` returns `"/C"` on Windows and `"-c"` on Unix.

`RealCommandRunner::run_shell` and `run_shell_interactive` will use these helpers instead of hardcoded Unix values. Test support runners (`RecordingCommandRunner`, `DispatchingCommandRunner`) will also use these helpers so recorded program names match the real implementation on any platform.

### Shell quoting

Shell quoting is delegated to the `shell-escape` crate (v0.1.5). The public `shell_quote` function is a thin wrapper over `shell_escape::escape(Cow::Borrowed(s)).into_owned()`. The crate dispatches automatically: on Unix (and on MSYS2/Git Bash via the `MSYSTEM` env var), it uses POSIX single-quote wrapping, whitelisting safe characters and only quoting strings that contain unsafe ones. On Windows, it uses `cmd.exe`-compatible double-quote wrapping with proper backslash handling before quotes.

### Default editor discovery

`find_default_editor` will probe `where.exe` with candidate `["notepad"]` on Windows, and `which` with candidates `["nano", "vim", "vi", "emacs"]` on Unix.

### `cfg!(windows)` runtime checks over `#[cfg(windows)]` conditional compilation

All platform-specific branching will use `cfg!(windows)` runtime conditionals rather than `#[cfg(windows)]` conditional compilation attributes. With `cfg!(windows)`, both code paths compile on every platform. The compiler optimizes away the dead branch, so there is no runtime cost. This design choice has two concrete benefits:

1. **CI catches errors in both paths on any host.** Clippy, `cargo check`, and `cargo build` verify the Windows code paths even on the Linux CI runners. With `#[cfg(windows)]`, Windows-only code would be invisible to the compiler on Linux and could silently accumulate syntax errors, type mismatches, or API drift.

2. **Windows-specific functions are unit-testable on Linux.** Because the platform-specific helpers are regular (non-`cfg`-gated) functions, they can be called directly in tests that run on the Linux development host.

## Consequences

### Positive

- Windows binaries produced by cross-compilation will have functional shell execution, shell quoting, and editor discovery instead of silently invoking nonexistent Unix programs.
- Both platform code paths compile and are linted on every platform, catching regressions without requiring a Windows CI runner.
- Windows-specific quoting logic is unit-testable on Linux, maintaining the project's test-on-host-platform development model.
- The `cmd.exe /C` choice aligns with what Cargo, npm, and git use internally on Windows, minimizing surprises for users.

### Negative

- The `cmd.exe` shell is significantly less capable than POSIX sh. Features like pipes and redirects work, but there is no equivalent to compound commands with `&&` short-circuit semantics in all edge cases, no subshell expressions, and limited variable expansion. Users with complex command strings may need to use PowerShell explicitly or call a script file.
- `find_default_editor` defaults to `notepad` on Windows, which is a minimal editor. Users who want a better editing experience must set `$VISUAL` or `$EDITOR`.

### Neutral

- The Nix flake development environment remains Unix-only. Windows is a cross-compilation target, not a development host. No changes to the flake are needed.
- `cmd.exe /C` is the execution model for user-configurable commands only. Hardcoded internal commands (cargo, npm, git) continue to use `std::process::Command` with explicit argument lists, unaffected by this change (consistent with [ADR-011](011-command-execution-strategy.md)).
- This decision does not add Windows to the Nix flake's supported systems list. The flake continues to support `x86_64-linux`, `aarch64-linux`, and `aarch64-darwin` for development.

## Alternatives Considered

### PowerShell instead of `cmd.exe`

PowerShell (`powershell.exe` or `pwsh.exe`) is more capable than `cmd.exe`, offering a richer scripting language with proper string handling, pipelines, and structured output. However, `cmd.exe /C` is the universal Windows baseline. It is what Cargo, npm, and git use internally for shell execution. It is always present on every Windows installation, including Server Core and minimal container images. PowerShell has a different command syntax for some operations and its presence is not guaranteed on all Windows variants. `cmd.exe` was chosen for the same reason POSIX sh was chosen over bash in [ADR-011](011-command-execution-strategy.md): reliability over richness.

### `#[cfg(windows)]` conditional compilation instead of `cfg!(windows)` runtime checks

Conditional compilation via `#[cfg(windows)]` attributes would exclude Windows code paths from compilation on non-Windows hosts. This is the idiomatic Rust approach for platform-specific code that uses platform-specific APIs (e.g., Windows-only system calls). However, the Windows-specific code in this change consists entirely of string manipulation and standard library calls that are available on all platforms. Using `#[cfg(windows)]` would make these code paths invisible to clippy and `cargo check` on Linux, allowing errors to accumulate undetected. Since all the code is portable Rust, `cfg!(windows)` preserves compile-time verification on all platforms at zero runtime cost.

### Hand-rolled shell quoting instead of a dedicated crate

The project's general dependency philosophy favours minimal inline implementations where the logic is small and well-understood. A hand-rolled approach was prototyped with a private `shell_quote_posix` function (POSIX single-quote wrapping with `'\''` escape sequences) and a private `shell_quote_windows` function (`""` double-quote escaping and `%%` percent escaping for `cmd.exe`). However, `shell-escape` was adopted instead for several reasons: the crate is mature and well-used across the Rust ecosystem, it handles the MSYS2/Git Bash edge case (detecting the `MSYSTEM` environment variable to select POSIX quoting on Windows when running under Git Bash), and it provides correctness guarantees for backslash-before-quote sequences on Windows that are difficult to replicate correctly by hand. Unlike the `fern` replacement documented in [ADR-018](018-replace-fern-with-cli-logger.md), where the hand-rolled alternative was straightforward and the crate was unmaintained, shell quoting has subtle platform-specific corner cases where a well-tested library offers meaningful correctness value over an inline implementation.
