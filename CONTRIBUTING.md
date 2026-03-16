# Contributing to Cursus

## Development environment

This project uses [Nix flakes](https://nixos.wiki/wiki/Flakes) and
[direnv](https://direnv.net/) for a reproducible development environment. The
flake supports x86_64-linux, aarch64-linux, and aarch64-darwin.

### Option A: Dev container (no Nix required)

If you would prefer not to install Nix, a
[dev container](https://containers.dev) configuration is provided in
`.devcontainer/`. Any tool that supports the spec will work — including
[VS Code](https://code.visualstudio.com/docs/devcontainers/containers) with the
Dev Containers extension, the standalone
[devcontainer CLI](https://github.com/devcontainers/cli), and
[GitHub Codespaces](https://codespaces.new/zantarix/cursus) (zero local setup).

The container installs Nix and enters the flake's dev shell automatically, so
all the same build commands work inside it.

### Option B: Nix + direnv

If you are new to Nix, the [First Steps with Nix](https://nix.dev/tutorials/first-steps/)
tutorial is a good place to start if you want to know more, but simply having
the `nix` package manager and `direnv` installed should be enough to get
things to just work for you.

The dev shell provides: rustc (nightly), cargo, rustfmt, clippy, rust-analyzer,
cargo-make, cargo-llvm-cov, zig, and cargo-zigbuild for cross-compilation to
all targets (Linux, macOS, Windows).

## Build commands

```bash
cargo build                    # Build the project
cargo run                      # Run the application
cargo test                     # Run all tests
cargo test <test_name>         # Run a specific test
cargo clippy                   # Lint
cargo fmt                      # Format
cargo make coverage            # Check coverage thresholds

# Generate static release binaries (all via cargo-zigbuild)
cargo make release                 # Build all release targets
cargo make release-linux-x86_64    # x86_64 Linux (musl static)
cargo make release-linux-aarch64   # ARM64 Linux (musl static)
cargo make release-linux-riscv64   # RISC-V Linux (musl static)
cargo make release-macos-x86_64    # x86_64 macOS
cargo make release-macos-aarch64   # ARM64 macOS
cargo make release-windows-x86_64  # x86_64 Windows (GNULLVM)
cargo make release-windows-aarch64 # ARM64 Windows (GNULLVM)
```

## Code style

- Rust 2024 edition
- Format code before every commit (`cargo fmt`)
- Prefer functional style over imperative style
- Never write production code that panics — avoid `unwrap()`, `expect()`, `panic!()`, and `unreachable!()` outside of tests; use `anyhow::Result`, `context()`, or `bail!()` instead

## Testing

Integration tests live in `tests/` and must always use `--no-interactive` to
prevent the TUI from running. They call `cursus::run()` as the entry point
and set up a temporary git repository to give the test a playground to operate
in.

Coverage thresholds:

| Metric | Threshold |
|--------|-----------|
| Lines | 90% |
| Regions | 90% |
| Functions | 90% |
| Branches | 80% |

## Architecture decisions

Significant design choices are documented as Architecture Decision Records in
`docs/adr/`.
