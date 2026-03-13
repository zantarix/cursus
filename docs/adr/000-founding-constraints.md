# ADR-000: Founding Constraints and Initial Choices

## Status

Accepted

## Context

Chronicle is a release management CLI tool that interacts with code repositories. This ADR is a retrospective capture of the founding constraints and initial choices that shaped the project before it became public. These decisions were not made at a single point in time but rather represent the ground-truth assumptions that every subsequent architectural decision builds upon.

The project needed to solve a specific problem: managing releases across monorepo and single-package repositories with support for multiple package managers. The solution had to be portable across Linux, and macOS, integrate naturally into developer workflows and CI pipelines, and impose minimal runtime dependencies on users.

## Decision

We will document the following founding constraints and choices as the baseline for all future architectural decisions.

### Claude Code for development

>![INFO]
> As with the most of the rest of this repo, the other sections of this document were AI written, with guidance. This section is 100% human. To anyone who interacts with this repo, I have one promise. If something says it's from me, then it is from me and hasn't gone through a bot. That is just common decency.

One of the leading reasons for me starting this project was to stress test AI coding agents and see how good, or bad, they really were. From day one, there has been an explicit goal to be hands off with the precise code that is written and only provide feedback in the form of reviews and design work.

This isn't a vibe coded project - at time of writing, I am five weeks deep on this project. I have put a lot of effort into guiding both the design of the system in general and in code review of the specifics. 5 weeks sounds like a lot of time, but in that time this project has grown into the 22,000 line piece of software that it is now, and it's still growing. There are also thousands of lines of text documentation in these ADR's.

I will document elsewhere, probably in a discussion thread on GitHub, some of the learnings and pitfalls I've run into, but also some of the successes I've had. It has definitely been a mixed bag, but a good experience overall. I never could have gotten as far as I have with this project in the time that I have alone. My first instructions to Claude were in a completely empty repository and it bootstrapped everything, including the development environment. 99% of the code in this repository, including all the system prompts and Claude configuration, is all AI generated with oversight. Commits in this repo attributed solely to me are my own work, so you can judge exactly how much is not AI generated if you really wish to.

### Static binary distribution as a first-class goal

The primary distribution goal is a single static binary that users can download and run without any runtime dependencies. This was a hard constraint from day one and the single most influential decision on the project's technical direction. Every tooling and language choice flows downstream from this requirement.

Beyond portability, a static binary is essential for ecosystem-neutrality. Chronicle is designed to support multiple package ecosystems impartially. If it were distributed as an npm package, a pip package, or a Cargo crate, it would create an implicit dependency on that ecosystem's toolchain being installed -- a JavaScript developer managing a pure JavaScript project should not need Rust installed, and a Rust developer should not need Node.js. A self-contained static binary avoids this problem entirely: it can manage any supported ecosystem without requiring any of them to be present on the machine beyond what the project itself needs. This neutrality is what makes it possible to credibly support any package ecosystem without favouring the one Chronicle happens to be distributed through.

### Rust as the implementation language

Rust was chosen because of the static binary goal. It is the language that best enabled fully static binaries (via musl on Linux, native static linking on macOS, and GNULLVM on Windows) and straightforward cross-compilation for distribution across all three major platforms. The language choice was a consequence of the distribution goal, not the other way around.

### CLI tool as the user-facing interface

A CLI tool is the required user-facing interface for two reasons. First, it is the lowest common denominator: it works on every platform, in every terminal, in CI pipelines, and in shell scripts with no additional runtime or integration layer. Second, the primary target audience is developers, most of whom are comfortable at the command line already. A GUI, web interface, or IDE plugin would not serve the primary use case and would add distribution and maintenance complexity.

### Dual artifact: CLI tool and Rust library

Chronicle is designed as both a standalone CLI binary and a reusable Rust library (crate). The library interface allows other tooling to integrate Chronicle's release management logic programmatically. There are future plans to offer prepackaged GitHub Actions or other CI workflow integrations that may leverage this library interface.

### Initial supported package managers: JavaScript and Cargo

The first two supported package managers are JavaScript (npm/yarn/pnpm workspaces) and Cargo (Rust). JavaScript was chosen first because it is the author's primary day-to-day language, making it a natural and well-understood starting point. Cargo was chosen because Chronicle is itself a Rust project, which means it can use Chronicle to manage its own releases -- dogfooding the tool before it is released publicly. This allows the team to validate the implementation against a real-world use case before anyone else depends on it.

These two are the founding supported package managers. The architecture (the `PackageManagerAdapter` trait) is designed to make adding further package managers straightforward. Additional package managers will be added over time as community interest and contributions emerge.

### Nix flakes for the development environment

Nix flakes were chosen as the development environment manager. The developer uses NixOS, but the choice was also motivated by practical cross-platform benefits:

- The project has been developed on both NixOS and macOS, and the flake worked out of the box on both.
- Nix can provide virtually any development tooling (Rust nightly, zig for cross-compilation, cargo plugins) in a reproducible, declarative configuration.
- It solved cross-compilation setup pain that had been experienced with other languages and build systems in prior projects.

## Consequences

### Positive

- Static binary distribution eliminates "works on my machine" problems for end users and simplifies CI integration.
- Ecosystem-neutral distribution means users never need to install an unrelated toolchain just to run Chronicle. A JavaScript team does not need Rust; a Rust team does not need Node.js. This is a prerequisite for impartially supporting multiple ecosystems.
- Rust provides memory safety, strong type system, and excellent performance without a garbage collector or runtime.
- The dual CLI/library design keeps the door open for higher-level integrations without duplicating logic.
- Nix flakes make onboarding deterministic: `nix develop` provides the complete toolchain regardless of host OS.
- Cross-compilation to seven targets (three Linux architectures, two macOS, two Windows) is handled uniformly through cargo-zigbuild within the Nix-provided environment.
- Dogfooding via Cargo support means Chronicle's own release process exercises the tool, catching bugs before they reach external users.
- Starting with JavaScript covers the largest package ecosystem by volume, maximising early utility.

### Negative

- Rust has a steeper learning curve than Go or TypeScript, which limits the potential contributor pool.
- Nix flakes add a learning barrier for contributors unfamiliar with Nix. Developers who do not use Nix must manually assemble the toolchain.
- The static binary constraint rules out plugin architectures or runtime extensibility via dynamic loading.
- Committing to Rust means the library crate is only directly consumable by other Rust projects; FFI or subprocess invocation is required for other languages.

### Neutral

- Starting with only two package managers means early adopters using other ecosystems (Python, Go, Java) cannot use Chronicle yet, but the `PackageManagerAdapter` trait provides a clear extension point.
- The CLI-first design means TUI features (interactive wizards) are additive, not foundational. The tool must always be fully operable in non-interactive mode for CI use.
- Claude Code integration is a development workflow choice, not an architectural one. It does not affect the shipped artifact or user experience.
- The Nix flake currently targets three host systems (x86_64-linux, aarch64-linux, aarch64-darwin) but cross-compiles to seven binary targets from any of them.

## Alternatives Considered

### Go as the implementation language

Go produces static binaries and cross-compiles easily. It was not chosen because Rust's musl-based static linking and cargo-zigbuild cross-compilation pipeline produced more reliably portable binaries, particularly for the Windows and RISC-V targets. Go's garbage collector and larger binary sizes were also minor factors.

### TypeScript (Node.js CLI) as the implementation language

A TypeScript CLI would have been natural for the npm ecosystem but requires a Node.js runtime, directly contradicting the static binary distribution constraint. Bundlers like pkg or bun compile exist but produce larger artifacts with embedded runtimes and limited cross-platform support.

### Docker-based development environment

A Docker-based dev environment was considered but rejected. It adds overhead for interactive development, complicates access to host tools (editors, git credentials), and does not natively support macOS cross-compilation. Nix flakes provide the same reproducibility with less friction.
