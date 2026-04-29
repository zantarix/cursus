---
name: Prefer full supersession when a new ADR absorbs a predecessor's decisions
description: When writing a superseding ADR, if the new ADR can plausibly re-home all of the predecessor's still-active decisions, prefer marking the predecessor fully Superseded rather than leaving it Accepted with a partial-supersession erratum.
type: feedback
---

When a new ADR (B) supersedes part of an older ADR (A), I should actively consider whether B can absorb *all* of A's still-active decisions and mark A fully Superseded, rather than defaulting to a scoped/partial supersession that leaves A as Accepted.

**Why:** Once an ADR is Accepted and committed, its scope of supersession is fixed — you cannot later "upgrade" a partial supersession to a full one. The user noted in retrospect they'd have preferred one ADR be fully superseded, but because the replacement is now Accepted and immutable, that re-scoping is no longer available. Choosing partial supersession is a one-way door.

**How to apply:** When drafting a superseding ADR, before settling on the supersession scope:

1. List every active decision in the predecessor (not just the headline one).
2. For each, ask: could the new ADR cleanly re-state it (or absorb it by reference) such that the predecessor becomes purely historical?
3. If yes for all, prefer full supersession — restate the retained decisions in the new ADR's Decision section so nothing is orphaned.
4. Only fall back to partial supersession (predecessor stays Accepted, erratum scopes the change) when the predecessor genuinely contains decisions that don't belong in the new ADR's scope.
5. Surface this choice to the user explicitly during drafting, since it's irreversible after acceptance.
