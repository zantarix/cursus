# ADR-020: Structure TUI Wizards as Submodule Directories with One File per Screen

## Status

Accepted

## Context

Chronicle's TUI wizards use ratatui and crossterm to present multi-screen interactive flows. Each wizard follows a pattern of a `Screen` enum for state, pure `handle_key()` functions for state transitions, and `render_*()` functions for drawing. Tests use `ratatui::backend::TestBackend` and assert on both state transitions and rendered output.

As the project matured, wizard modules grew substantially. The `tui::init` module reached approximately 2000 lines after [ADR-019](019-improve-init-workflow.md) expanded it to eight screens, and `tui::change` reached approximately 950 lines with two screens. Each screen carries its own handler logic, rendering logic, and a full test suite covering both handlers and rendering -- all colocated in a single file. Navigating, reviewing, and testing individual screens became increasingly difficult as unrelated screen code competed for attention in the same file.

The project needed a decomposition strategy that preserves the existing TUI architecture (dispatcher pattern, pure handler functions, `TestBackend`-based tests) while making each screen independently navigable and testable.

## Decision

We will structure each TUI wizard as a submodule directory with one file per screen. The directory layout follows this pattern:

```
src/tui/<wizard>/
  mod.rs              # Shared types, dispatchers, entry point, test helpers, workflow tests
  <screen_a>.rs       # Handler, renderer, and tests for screen A
  <screen_b>.rs       # Handler, renderer, and tests for screen B
```

The `mod.rs` file will contain:

- **Shared types**: The `Screen` enum, result types (e.g., `InitResult`, `ChangeResult`), accumulated state structs (e.g., `WizardState`), and type aliases (e.g., `HandleResult`).
- **Dispatcher functions**: The top-level `handle_key()` and `ui()` functions that match on the `Screen` enum and delegate to the appropriate submodule.
- **Entry point**: The `run()` function that owns the terminal event loop.
- **Shared test helpers**: A `pub(super) mod test_helpers` block providing factory functions and assertion helpers used by all screen test suites.
- **Cross-screen workflow tests**: Integration-style tests that drive multiple screens in sequence through the dispatcher, verifying end-to-end wizard flows.

Each screen submodule file will contain:

- A `handle_*()` function implementing state transitions for that screen.
- A `render_*()` function implementing the ratatui drawing logic for that screen.
- A `#[cfg(test)] mod tests` block with both handler tests and rendering tests.

Screen-specific functions will use `pub(super)` visibility rather than `pub` or `pub(crate)`. This is required because the handler and render function signatures reference types that are private to the wizard module (e.g., `Screen`, `WizardState`, `HandleResult`). Using `pub` or `pub(crate)` would trigger the `private_interfaces` lint, since those visibility levels would expose functions whose signatures mention module-private types.

Submodule tests access parent-level items (the `Screen` enum, the `handle_key()` dispatcher, test helpers) via `super::super::` paths. For example:

```rust
use super::super::test_helpers::*;
use super::super::{Screen, handle_key};
```

## Consequences

### Positive

- Each screen is independently navigable in file explorers and editors, reducing cognitive load when working on a single screen.
- Screen-specific tests are colocated with the code they test, making it obvious which tests cover which screen.
- Adding a new screen to a wizard requires creating one new file and adding a `mod` declaration plus dispatcher arms in `mod.rs`, without touching any other screen file.
- Code review is simplified: a PR that modifies one screen produces a diff scoped to that screen's file.
- The `mod.rs` file remains a readable overview of the wizard's structure, containing only shared types, dispatchers, and cross-cutting tests.

### Negative

- Submodule tests require `super::super::` paths to reach parent types and helpers, which is verbose and may be unfamiliar to contributors.
- The `pub(super)` visibility requirement is non-obvious -- contributors may instinctively reach for `pub` and encounter the `private_interfaces` lint. The pattern must be learned.
- Adding a new screen requires changes in two places (the new file and `mod.rs` dispatcher arms), whereas a monolithic file requires changes in only one place.

### Neutral

- The overall TUI architecture (Screen enum, pure handler functions, TestBackend rendering tests) is unchanged. This ADR addresses file organization, not the programming model.
- Cross-screen workflow tests remain in `mod.rs`, ensuring that screen-to-screen transitions are tested at the dispatcher level regardless of how files are organized.
- This pattern applies uniformly to all TUI wizards (`init`, `change`, and any future wizards).

## Alternatives Considered

### Single monolithic file per wizard

Keeping all screens, handlers, renderers, and tests in one file is the simplest structure and avoids visibility gymnastics. However, at 2000 lines the `init` wizard became difficult to navigate, and unrelated screen logic created merge conflicts when multiple screens were being modified concurrently. This approach does not scale with the number of screens.

### Organize by concern (handlers.rs, renderers.rs, tests.rs)

Splitting by function type rather than by screen would group all handlers together, all renderers together, and all tests together. This was rejected because it breaks the locality principle: understanding or modifying a single screen requires jumping across three files. It also makes it harder to add or remove a screen, since every concern-file must be updated. The screen-per-file approach keeps related code together.

### Use `pub(crate)` visibility and make shared types public

Making `Screen`, `WizardState`, and other internal types `pub(crate)` would allow screen functions to also be `pub(crate)`, avoiding the `pub(super)` pattern. This was rejected because it exposes wizard internals to the entire crate. These types are implementation details of the TUI layer and should not be accessible from CLI, model, or package manager code. `pub(super)` correctly constrains visibility to the wizard module boundary.
