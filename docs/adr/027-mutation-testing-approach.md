# ADR-027: Adopt Mutation Testing as a Test Quality Verification Strategy

## Status

Accepted

## Context

Cursus maintains strict code coverage thresholds -- 90% for lines, regions, and functions, and 80% for branches. While coverage measures which code paths are exercised, it does not verify that tests actually assert meaningful behaviour. A test suite can achieve 100% coverage by executing every line without asserting anything about the results. This gap means that passing tests and high coverage metrics can mask code that is insufficiently tested: a mutation (e.g., flipping a comparison operator, removing a guard clause, replacing a return value) could be applied and tests would still pass.

Mutation testing addresses this gap by systematically applying small semantic changes (mutants) to the source code and checking whether the existing test suite detects each change. A "missed" mutant -- one where all tests still pass after the mutation -- indicates either that the code path lacks a meaningful assertion or that the code itself contains redundancy that makes the mutation behaviourally equivalent to the original.

Cursus uses `cargo mutants` as its mutation testing tool. The tool generates mutants, runs the test suite against each one, and reports missed mutants in `mutants.out/missed.txt`. The project also has an `analyse-mutations` skill in the Claude Code development environment that assists developers in working through missed mutants systematically.

Mutation testing runs are computationally expensive -- each mutant requires a full or partial test suite execution. The project needs a clear policy on when mutation testing runs, how missed mutants are triaged, what strategies are valid for resolving them, and how to handle the small number of cases where mutation testing is genuinely inapplicable.

## Decision

We will use `cargo mutants` for mutation testing as a best-effort, manual developer activity, not as part of CI. Developers will run mutation tests periodically and address missed mutants when practical.

**Two strategies for addressing missed mutants.** When a missed mutant is addressed, it should be resolved through one of exactly two approaches:

1. **Add a test** that exercises the mutated code path and would fail if the mutation were applied. This is the appropriate response when the missed mutant reveals a genuine gap in test assertions -- the code path represents meaningful business logic that was executed but not verified. The new test should target the specific semantic distinction the mutant exploits, not merely increase coverage.

2. **Simplify the code** when the mutation is equivalent -- that is, the original and mutated code behave identically in all reachable cases. This indicates the code contains redundant logic. For example, a guard condition that can never be false, or an explicit `if x < y { v = x }` pattern where `v = v.min(x)` expresses the same intent without the conditional branch. Simplification is preferred over testing in these cases because the code genuinely has no meaningful distinction between the two forms, and adding a test would be testing an implementation detail rather than a behaviour.

**Restricted use of `#[mutants::skip]`.** The `#[mutants::skip]` attribute is reserved exclusively for entry points like `main()` that cannot meaningfully be tested through mutation -- functions that serve only as a thin shell to invoke the real logic. It must not be used to silence false positives or equivalent mutations. Equivalent mutations are resolved by simplifying the code (strategy 2 above), which eliminates both the mutant and the redundancy.

**Manual execution, not CI.** Mutation testing will be run manually by developers using `cargo mutants`. Results appear in `mutants.out/missed.txt` and are triaged iteratively. The computational cost of mutation testing (running the full test suite per mutant) makes it impractical for CI pipelines, especially as the codebase grows. There is no requirement to achieve zero missed mutants or to run mutation testing on every change -- it is a best-effort quality tool used at the developer's discretion.

**Iterative triage workflow.** Developers will use the `analyse-mutations` skill to work through missed mutants. For each missed mutant, the developer determines whether it reveals a test gap (strategy 1) or code redundancy (strategy 2), applies the appropriate fix, and re-runs to confirm the mutant is resolved.

## Consequences

### Positive

- Tests are verified to actually assert meaningful behaviour, not merely execute code paths. This catches "assertion-free" tests and weak assertions that would pass regardless of the code's behaviour.
- Code simplification driven by equivalent mutants reduces unnecessary complexity and removes dead or redundant logic, improving maintainability.
- The two-strategy approach prevents the anti-pattern of writing contorted tests to "kill" mutants that are genuinely equivalent, which would add maintenance burden without testing value.
- The strict `#[mutants::skip]` policy prevents the attribute from becoming a blanket escape hatch that undermines the practice.
- Manual, best-effort execution gives developers control over when the expensive analysis runs, avoiding CI slowdowns while still improving quality when applied.

### Negative

- Best-effort execution means mutation testing coverage depends on developer initiative. There is no automated gate preventing under-tested code from merging, and some missed mutants may persist indefinitely.
- `cargo mutants` run time scales with the number of mutants and the test suite duration. As the codebase grows, full mutation runs may become prohibitively slow, potentially requiring targeted runs on changed modules only.
- The two-strategy triage requires developer judgement to determine whether a mutant is equivalent or reveals a real gap. Incorrect classification could lead to either unnecessary tests or missed coverage.

### Neutral

- Mutation testing results (`mutants.out/`) are ephemeral local artifacts, not committed to the repository. There is no persistent record of mutation testing runs beyond the code changes they motivate.
- The `analyse-mutations` skill is a development environment convenience, not a project dependency. Developers without Claude Code can still run `cargo mutants` and triage results manually.
- This decision complements but does not replace the existing coverage thresholds. Coverage remains the CI-enforced minimum; mutation testing is the manual quality verification layer above it.

## Alternatives Considered

### Run mutation testing in CI

Running `cargo mutants` as a CI step would enforce mutation testing automatically on every pull request. This was rejected because mutation testing is computationally expensive -- each mutant requires running some or all of the test suite, and a moderate codebase can produce hundreds of mutants. This would make CI pipelines prohibitively slow and expensive, especially for minor changes. The manual approach provides the same quality signal without the cost, at the expense of relying on developer discipline.

### Allow `#[mutants::skip]` for equivalent mutations

Permitting `#[mutants::skip]` on functions with equivalent mutations would be faster than refactoring the code to eliminate the redundancy. This was rejected because it treats the symptom (the missed mutant) rather than the cause (redundant code). Simplifying the code eliminates both the mutant and the unnecessary complexity, producing a net improvement. Using `#[mutants::skip]` would also create a maintenance risk: future code changes might make a previously-equivalent mutation no longer equivalent, but the skip attribute would hide the now-real test gap.

### Use a different mutation testing tool

Other mutation testing approaches exist for Rust, including custom test harnesses or manual fault injection. `cargo mutants` was chosen because it integrates directly with the standard Cargo test workflow, requires no code instrumentation, and is actively maintained. It supports incremental runs on changed code and produces clear, actionable output. No alternative tool offered a meaningfully better trade-off for this project's needs.

### Require zero missed mutants as a merge gate

Some projects enforce a strict policy that all missed mutants must be resolved before code can be merged. This was rejected because the computational cost and developer time investment make it impractical as a hard requirement. The best-effort approach still provides value -- each mutation testing session that a developer chooses to run produces either better tests or simpler code -- without creating a bottleneck or discouraging developers from running mutation tests at all due to the burden of mandatory full resolution.
