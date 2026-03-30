---
files:
- "**/*.rs"
---

Further to the baseline Zantarix clippy exceptions, the following is allowed in this project:

* `clippy::excessive_nesting` and `clippy::too_many_lines` in `dependency_graph::strongconnect()` as this algorithm is indivisible.
