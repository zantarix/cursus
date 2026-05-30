---
paths:
  - "**/*.rs"
---

# Self-describing token detection

When a token's value already encodes which variant it is, detect all variants unconditionally and let the matched variant drive rendering. Do not gate on a config flag.

**Example:** Commit references in changelogs — `#123` is inherently a GitHub PR; `!123` is a GitLab MR. Always detect both and render each in its matching style. Do not branch on `[gitlab].enabled`.

**Why:** Config-gating couples unrelated state and can mis-render a reference whose actual forge differs from the configured one. Self-describing detection is simpler and eliminates unnecessary plumbing.
