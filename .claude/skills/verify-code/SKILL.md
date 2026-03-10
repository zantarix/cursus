---
name: verify-code
description: Use to fully verify changes made to code.
---

Verify the current changes pass all quality checks. Run these steps in order, attempting to fix any failures or stop if
you are unable to fix the failures:

1. **Lint** the code: `cargo clippy -- -D warnings`
2. **Test and check Coverage** check: `cargo make coverage` (must meet 90% threshold, including 80% branch coverage)
3. **Format** the code: `cargo fmt`

Report a summary of results for each step. If any step fails, diagnose the issue and suggest a fix.
