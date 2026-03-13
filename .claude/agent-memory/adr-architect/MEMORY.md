# ADR Architect Memory - Chronicle Project

## ADR Style Directive

New ADRs MUST follow the agent's own template/style, NOT the style of existing ADRs 001-006.
Key differences from old style:

- **Consequences**: Split into ### Positive / ### Negative / ### Neutral subsections
- **Alternatives Considered**: Always include this section with named alternatives and rejection rationale
- **Decision language**: Imperative ("We will...", "The system shall...")
- The old ADRs (001-006) are historical records; do not retroactively change them

## ADR Inventory and Index

- See `inventory.md` for a detailed list of all ADRs.
- `docs/adr/README.md` is the public-facing ADR index. Both must be updated whenever an ADR is created, updated, or has its status changed.

## Key Architectural Patterns

- See `patterns.md` for detailed patterns extracted from ADRs.

## ADR Process Notes

- Always ask clarifying questions before writing. Do not assume the scope or preferred approach.
- The user values precision: understand the exact use case before proposing a solution.
- ADRs in "Proposed" status may be amended directly. Only "Accepted" ADRs are immutable.
- Never add "Implementation" sections to ADRs. Design choices discovered during implementation should be folded into the Decision section. Test results, coverage metrics, and file lists are outcomes, not decisions -- they do not belong in an ADR.
- After editing an ADR, the resulting document must conform to the standard ADR template. No new sections should be added. This ensures consistency across the entire ADR corpus regardless of whether an ADR was just created or amended later.
- Keep ADRs at the right abstraction level. Discussing implementation approaches is fine, but do not reference specific lines of code. ADRs capture architectural and design decisions conceptually, not as code documentation. For example: "Use `.get()` chain to avoid panics when accessing TOML fields" is appropriate, but "In line 173, use `doc.get("package").and_then(|p| p.get("publish"))`" is too specific. Similarly, "Separate publishability checks from publish operations via trait method" is good, but spelling out exact function signatures is too granular.

## ADR Cross-References

- All ADR cross-references MUST use markdown links: `[ADR-013](013-logging-infrastructure.md)` not plain `ADR-013`
- Links use relative paths (just the filename, no directory prefix) since all ADRs live in the same directory
- Title lines (`# ADR-NNN: ...`) are self-references and should NOT be linkified
- This applies to all sections: Context, Decision, Consequences, Alternatives, Errata

## ADR Context Style

- Do NOT enumerate all alternatives in the Context section. Context should describe the problem and forces at play.
- The Decision section details the chosen approach. Other options go in Alternatives Considered.

## ADR Template Compliance

- Errata sections are allowed as an exception to "no new sections" rule -- they record dated corrections to accepted ADRs without modifying the originals.
- Errata belong on the **affected ADR** as a forward pointer to the ADR that supersedes or amends it. Do NOT put errata on the ADR that introduces the change.
- Errata placement: **bottom of the document** (after Alternatives Considered).
- Only add an Errata section when there is actual errata to record. No empty "None." placeholders.

## ADR Status Quick Reference

- ADR-000: Accepted (founding constraints — retrospective)
- ADR-001 to ADR-018: Accepted
- ADR-013: backend sub-decision superseded by ADR-018 (log facade decision still valid)
- ADR-019: Accepted (improved init workflow)
- ADR-020: Accepted (TUI screen submodule structure)
- ADR-021: Proposed (commit references in changelog entries)
- ADR-022: Proposed (distribution strategy)
- ADR-023: Proposed (dependency propagation bumps)
- ADR-024: Proposed (linked package versions)
- ADR-025: Accepted (auto changeset from conventional commit)
- Next ADR number: 026

## Project ADR Rules (from CLAUDE.md)

- ADRs are immutable once accepted -- do not edit accepted ADRs
- Use Errata sections when new requirements contradict an older ADR
- Update status to "Superceded by ADR-XXX" if fully replaced
- Stored in `docs/adr/`
