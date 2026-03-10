# ADR Architect Memory - Chronicle Project

## ADR Style Directive

New ADRs MUST follow the agent's own template/style, NOT the style of existing ADRs 001-006.
Key differences from old style:

- **Consequences**: Split into ### Positive / ### Negative / ### Neutral subsections
- **Alternatives Considered**: Always include this section with named alternatives and rejection rationale
- **Decision language**: Imperative ("We will...", "The system shall...")
- The old ADRs (001-006) are historical records; do not retroactively change them

## ADR Inventory

- See `inventory.md` for a detailed list of all ADRs.

## Key Architectural Patterns

- See `patterns.md` for detailed patterns extracted from ADRs.

## ADR Process Notes

- Always ask clarifying questions before writing. Do not assume the scope or preferred approach.
- The user values precision: understand the exact use case before proposing a solution.
- ADRs in "Proposed" status may be amended directly. Only "Accepted" ADRs are immutable.
- Never add "Implementation" sections to ADRs. Design choices discovered during implementation should be folded into the Decision section. Test results, coverage metrics, and file lists are outcomes, not decisions -- they do not belong in an ADR.
- After editing an ADR, the resulting document must conform to the standard ADR template. No new sections should be added. This ensures consistency across the entire ADR corpus regardless of whether an ADR was just created or amended later.
- Keep ADRs at the right abstraction level. Discussing implementation approaches is fine, but do not reference specific lines of code. ADRs capture architectural and design decisions conceptually, not as code documentation. For example: "Use `.get()` chain to avoid panics when accessing TOML fields" is appropriate, but "In line 173, use `doc.get("package").and_then(|p| p.get("publish"))`" is too specific. Similarly, "Separate publishability checks from publish operations via trait method" is good, but spelling out exact function signatures is too granular.

## ADR Context Style

- Do NOT enumerate all alternatives in the Context section. Context should describe the problem and forces at play.
- The Decision section details the chosen approach. Other options go in Alternatives Considered.

## ADR Template Compliance

- Errata sections are allowed as an exception to "no new sections" rule -- they record dated corrections to accepted ADRs without modifying the originals.
- Errata belong on the **affected ADR** as a forward pointer to the ADR that supersedes or amends it. Do NOT put errata on the ADR that introduces the change.

## ADR Status Quick Reference

- ADR-001 to ADR-016: Accepted
- Next ADR number: 017

## Project ADR Rules (from CLAUDE.md)

- ADRs are immutable once accepted -- do not edit accepted ADRs
- Use Errata sections when new requirements contradict an older ADR
- Update status to "Superceded by ADR-XXX" if fully replaced
- Stored in `docs/adr/`
