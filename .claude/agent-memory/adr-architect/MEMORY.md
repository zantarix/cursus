# ADR Architect Memory - Chronicle Project

## ADR Style Directive
New ADRs MUST follow the agent's own template/style, NOT the style of existing ADRs 001-006.
Key differences from old style:
- **Consequences**: Split into ### Positive / ### Negative / ### Neutral subsections
- **Alternatives Considered**: Always include this section with named alternatives and rejection rationale
- **Decision language**: Imperative ("We will...", "The system shall...")
- The old ADRs are historical records; do not retroactively change them

## ADR Inventory
- See `inventory.md` for a detailed list of all ADRs.

## Key Architectural Patterns
- See `patterns.md` for detailed patterns extracted from ADRs.

## Project ADR Rules (from CLAUDE.md)
- ADRs are immutable once accepted — do not edit accepted ADRs
- Use Errata sections when new requirements contradict an older ADR
- Update status to "Superceded by ADR-XXX" if fully replaced
- Stored in `docs/adr/`
