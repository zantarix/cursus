---
name: debug-errors
description: Use to systematically debug compilation errors, test failures, or runtime issues in the codebase.
user-invocable: false
---

Systematically debug the reported error(s). Follow these steps in order:

1. **Reproduce** the error: Run the failing command (e.g. `cargo build`, `cargo test`, or the specific command the user provided) to capture the exact error output.
2. **Test** the error: If applicable, create tests that more easily reproduce the error.
3. **Analyze** the error: Read the relevant source files indicated by the error messages. Identify the root cause — don't just treat symptoms.
4. **Check context**: Use `git diff` to see recent changes that may have introduced the issue. Search for related code with Grep/Glob to understand how the affected code is used elsewhere.
5. **Fix** the issue: Apply the minimal, targeted fix. Avoid unrelated refactors or cleanups.
6. **Verify** the fix: Re-run the originally failing command to confirm the error is resolved. Then run the verification skill to fully verify there are no regressions.

If multiple errors exist, fix them one at a time starting with the most fundamental (e.g. compilation errors before test failures). Report what the root cause was and what was changed to fix it.
