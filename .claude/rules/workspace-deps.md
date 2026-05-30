---
paths:
  - "**/Cargo.toml"
  - "**/Cargo.lock"
---

# Workspace dependency separation

Only promote a dependency to `[workspace.dependencies]` if it appears in at least one crate's `[dependencies]` (production use). Dev-only crates (`httpmock`, `insta`, `proptest`, `tempfile`, etc.) must be pinned directly in each crate's `[dev-dependencies]`.

**Why:** Keeps the workspace manifest a clear inventory of production dependencies; test infrastructure stays scoped to crates that actually use it.

**How to apply:** When adding a dependency needed only in tests, add it to the individual crate's `[dev-dependencies]` — never to `[workspace.dependencies]`.
