# ADR-011: Command Execution Strategy

## Status

Accepted

## Context

Cursus has several points in its release and publish workflows where it executes external commands. Some are hardcoded (e.g., `cargo generate-lockfile`, `npm install --package-lock-only`, the git operations from [ADR-006](006-git-lifecycle-hooks.md)), while others are user-configurable (e.g., `[npm].lock_command` from [ADR-009](009-javascript-package-manager-strategy.md), `[github].build_command` from [ADR-005](005-github-releases.md)). More configurable commands are likely as the tool evolves.

The existing user-configurable command, `[npm].lock_command`, splits the string on whitespace and executes the fragments directly via `std::process::Command`. This means shell features are unavailable: pipes, redirects, environment variable expansion, glob patterns, subshell expressions, and quoting rules all do not work. [ADR-009](009-javascript-package-manager-strategy.md) explicitly calls this out as a limitation and notes that "users needing shell features must wrap their command in a script." [ADR-005](005-github-releases.md), by contrast, specifies that `[github].build_command` should be executed via the system shell (`sh -c` on Unix), but this has not yet been implemented.

The inconsistency creates a confusing user experience. Two string-valued command fields in the same configuration file would behave differently depending on which section they appear in. Users cannot predict whether a given command field supports shell features without consulting documentation for that specific field.

Beyond execution semantics, there is no consistent policy for how configurable commands interact with dry-run mode, how their errors are handled, or how their working directory is determined. Each command field risks making ad-hoc choices that diverge from the rest of the tool.

## Decision

We will standardize how all user-configurable command fields are executed throughout Cursus. This ADR establishes conventions that apply to every current and future command configuration option.

### Shell execution

All user-configurable command strings will be executed through the system shell. Concretely, Cursus will invoke `/bin/sh -c "<command>"` using `std::process::Command::new("/bin/sh").args(["-c", &command_string])`.

Cursus's supported platforms are Linux and macOS (per the Nix flake: `x86_64-linux`, `aarch64-linux`, `aarch64-darwin`). `/bin/sh` is guaranteed to exist on all of these. On Linux it is typically dash (Debian/Ubuntu) or bash (most other distributions); on macOS it is zsh (as of macOS Catalina). In all cases it provides at least POSIX shell semantics.

Shell execution enables the features that users expect from command-line tools:

- Pipes: `command1 | command2`
- Redirects: `command > output.log 2>&1`
- Environment variable expansion: `echo $HOME`
- Glob patterns: `rm dist/*.tar.gz`
- Compound commands: `cd subdir && make release`
- Quoting and escaping: `echo "hello world"`

Users who need features beyond POSIX sh (e.g., bash arrays, `[[ ]]` conditionals, process substitution) can invoke their preferred shell explicitly within the command string (e.g., `bash -c "..."`) or call a script file.

### Configuration format

Command fields will be a single TOML string value, interpreted by the shell. For the initial implementation, Cursus will not support an array-of-strings command format.

Example:

```toml
[npm]
lock_command = "bun install --frozen-lockfile"

[github]
build_command = "cargo make release && sha256sum dist/* > dist/SHA256SUMS.txt"
```

A dual-format approach (accepting either a string for shell interpretation or an array for direct exec) may be revisited in the future. Many tools in the ecosystem support this pattern and it has clear utility for commands that do not need shell features. For now, the single-string format covers all use cases and keeps the implementation surface small.

### Working directory

All user-configurable commands execute with their working directory set to the git repository root (the directory containing `.git/`). This is consistent across all command fields regardless of which configuration section they appear in.

Commands that need to operate in a subdirectory can use `cd` within the shell command:

```toml
[npm]
lock_command = "cd frontend && bun install --frozen-lockfile"
```

### Dry-run interaction

Per [ADR-008](008-dry-run-local-only-guarantee.md), `--dry-run` must be strictly local-only: no subprocess invocations that could have side effects. All user-configurable commands are skipped during dry-run. Cursus prints what command would have been executed, but does not invoke the shell.

For example, during `cursus release --dry-run` with a configured `lock_command`, output would include:

```text
Would run lock command: bun install --frozen-lockfile
```

This is not a generic hook system with a per-hook dry-run safety toggle. Each command field is a named, purpose-specific configuration option, and all of them respect the dry-run invariant unconditionally.

### Error handling

Each command field defines its own error handling policy, appropriate to its role in the workflow. However, the following conventions apply universally:

- A non-zero exit code from the shell constitutes a failure.
- Cursus captures stderr from the failed command and includes it in the error message.
- Cursus does not attempt to interpret or parse command output beyond the exit code.
- Cursus does not roll back prior filesystem changes on command failure, consistent with the existing error handling philosophy (see [ADR-003](003-release-command.md), [ADR-006](006-git-lifecycle-hooks.md)).

Most command fields will use fail-fast semantics: a failure aborts the current workflow step and Cursus exits with a non-zero status code. Individual ADRs may override this for specific fields where continuation is appropriate (e.g., [ADR-005](005-github-releases.md) specifies that a `build_command` failure skips GitHub Release creation but does not roll back the registry publish).

### Scope of affected command fields

This ADR applies to all current and future user-configurable command fields in Cursus's configuration. As of this writing, the affected fields are:

- **`[npm].lock_command`** ([ADR-009](009-javascript-package-manager-strategy.md), implemented): Custom lock file update command. Currently uses whitespace splitting; will be migrated to shell execution.
- **`[github].build_command`** ([ADR-005](005-github-releases.md), not yet implemented): Build artifacts before creating a GitHub Release. [ADR-005](005-github-releases.md) already specifies shell execution; this ADR formalizes that choice as part of a broader standard.

Future command fields (e.g., a potential `[npm].publish_command` noted in [ADR-009](009-javascript-package-manager-strategy.md), or any new lifecycle commands) will follow the same conventions established here.

### Fields not affected

Hardcoded commands that Cursus invokes directly are not affected by this ADR. These include:

- `cargo generate-lockfile`, `cargo publish`
- `npm install --package-lock-only`, `pnpm install --lockfile-only`, `yarn install --mode update-lockfile`, `npm publish`
- Git operations (`git add`, `git commit`, `git tag`, `git push`)

These are internal implementation details of Cursus's adapters, not user-configurable. They use `std::process::Command` directly with explicit argument lists, which is correct for commands where Cursus controls the exact invocation.

### Migration of `[npm].lock_command`

The existing `[npm].lock_command` field will be migrated from whitespace-splitting to shell execution. This is a minor breaking change: commands that contain arguments with spaces in their values (relying on the current naive splitting behavior) could behave differently under shell parsing. In practice, this is unlikely to affect real-world configurations because:

- The whitespace-splitting approach already cannot handle quoted arguments or spaces in paths.
- Any command that works with whitespace splitting will also work under shell execution, since the shell splits unquoted words on whitespace identically.
- Users who need spaces in arguments are currently unable to express this; shell execution fixes rather than breaks their use case.

## Consequences

### Positive

- Consistent behavior across all command fields. Users learn the execution model once and can predict how any command field works.
- Shell features are available everywhere. Users no longer need to wrap commands in external scripts to use pipes, redirects, or compound expressions.
- Forward-compatible: new command fields added in future ADRs automatically inherit these conventions, reducing design decisions and documentation burden per field.
- The migration of `lock_command` to shell execution is practically non-breaking for existing configurations while enabling new use cases.

### Negative

- Shell execution introduces a dependency on `/bin/sh`. On minimal Docker images, `/bin/sh` may not be present. This is a theoretical concern; in practice, any environment capable of running Cursus's target package managers (Cargo, npm, git) will have a POSIX shell.
- Shell execution has security implications: command strings from the configuration file are passed to the shell, which interprets them with full shell semantics including command substitution, variable expansion, and arbitrary code execution. However, the configuration file (`.cursus/config.toml`) is checked into version control and subject to code review, making this equivalent in risk to any other checked-in script. Cursus does not interpolate user input or runtime values into command strings.
- Using `/bin/sh` limits commands to POSIX shell features by default. Bash-specific syntax (arrays, `[[ ]]`, process substitution) will fail on systems where `/bin/sh` is not bash. Users must explicitly invoke `bash -c "..."` or use a script file for non-POSIX features.
- The single-string format means complex multi-step operations may benefit from being extracted into a script file rather than inlined in TOML. This is standard practice for shell commands and not unique to Cursus.

### Neutral

- Environment variable injection (e.g., `CURSUS_VERSION`, `CURSUS_PACKAGE`) is not included in this ADR. If needed in the future, it can be added as a backward-compatible enhancement to the execution model defined here.
- Each command field's specific error handling behavior (fail-fast vs. continue) is defined by the ADR that introduces that field, not by this ADR. This ADR provides the defaults and conventions.

## Alternatives Considered

### Hardcoded `/bin/bash` instead of `/bin/sh`

Cursus could invoke `/bin/bash -c "<command>"` instead of `/bin/sh -c "<command>"`. Bash is more feature-rich than POSIX sh: it supports arrays, extended globbing, `[[ ]]` conditionals, process substitution (`<(cmd)`), and other conveniences that users of modern shells may expect.

This was rejected because bash is not universally present. Minimal container images (e.g., Alpine-based Docker images, which are common in CI) do not include bash by default. NixOS systems may also lack bash unless it is explicitly included in the environment. `/bin/sh` is guaranteed on all POSIX systems and provides a reliable baseline. Users who need bash features can invoke `bash -c "..."` within the command string or point the command at a bash script, preserving the option without imposing the dependency on all users.

### `$SHELL` (user's login shell)

Cursus could invoke the user's configured login shell via the `$SHELL` environment variable. This would respect the user's preferred shell (bash, zsh, fish, nushell, etc.).

This was rejected because `$SHELL` is unpredictable. Fish and nushell have incompatible syntax with POSIX sh. A command string written for one user's shell may not work for another contributor to the same repository. Since the configuration file is checked into version control and shared across the team, the shell used to interpret it must be deterministic and consistent across environments.

### Whitespace splitting without shell (status quo for `lock_command`)

Keep the current approach: split the command string on whitespace and pass the fragments to `std::process::Command`. This avoids shell dependencies and security concerns.

This was rejected because it produces surprising behavior for users. A string that looks like a shell command does not behave like one: quoting is ignored, pipes do not work, and environment variables are not expanded. [ADR-009](009-javascript-package-manager-strategy.md) already documents this as a known limitation. Maintaining two different execution models (whitespace splitting for `lock_command`, shell for `build_command`) would be worse than either approach applied consistently.

### Dual format: string or array

Command fields could accept either a TOML string (shell-interpreted) or a TOML array of strings (exec'd directly via `std::process::Command`), choosing the execution strategy based on the TOML value type. This provides maximum flexibility: shell features when needed, direct exec when simplicity and predictability are preferred.

This was deferred rather than rejected. Many tools in the ecosystem support this pattern (e.g., Docker's `CMD`, GitHub Actions' `run` vs. step arguments) and it has clear utility. However, it doubles the implementation and testing surface for every command field and adds a format choice that must be documented and explained. For the current scope, the single-string shell format covers all use cases. The dual format can be introduced as a backward-compatible enhancement if demand arises.

## Errata

- **2026-03-18**: [ADR-033](033-windows-shell-execution.md) extends this decision to support Windows. The `/bin/sh -c` convention described above applies to Unix only; on Windows, `cmd.exe /C` is used instead. The supported platform list (originally Linux and macOS) now includes Windows as a cross-compilation target. See [ADR-033](033-windows-shell-execution.md) for the full decision and rationale.
- **2026-04-19**: [ADR-046](046-streaming-command-execution.md) changed how user-configurable shell commands execute. `run_shell_mut` (which captured stdout/stderr into `Output`) was removed from `CommandRunner` and replaced by `run_streaming`. `run_streaming` inherits the parent's stdout and stderr so output appears live on the terminal; stdin is set to null. As a result, the two user-configurable command fields (`github.build_command` and `npm.lock_command`) no longer capture stderr on failure -- the stderr has already been streamed to the terminal. Error messages on failure include the exit status only. The dry-run log format for these commands is `[dry-run] would run (streaming): "<command>" (cwd: <path>)` rather than a generic "would run" line.
