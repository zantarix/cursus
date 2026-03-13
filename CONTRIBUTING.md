# Contributing to Cursus

## Development environment

This project uses [Nix flakes](https://nixos.wiki/wiki/Flakes) and
[direnv](https://direnv.net/) for a reproducible development environment. The
flake supports x86_64-linux, aarch64-linux, and aarch64-darwin.

If you are new to Nix, the [First Steps with Nix](https://nix.dev/tutorials/first-steps/)
tutorial is a good place to start if you want to know more, but simply having
the `nix` package manager and `direnv` installed should be enough to get
things to just work for you.

The dev shell provides: rustc (nightly), cargo, rustfmt, clippy, rust-analyzer,
cargo-make, cargo-llvm-cov, and a musl cross-compilation toolchain for static
binaries (Linux only).

## Build commands

```bash
cargo build                    # Build the project
cargo run                      # Run the application
cargo test                     # Run all tests
cargo test <test_name>         # Run a specific test
cargo clippy                   # Lint
cargo fmt                      # Format
cargo make coverage            # Check coverage thresholds
```

## Code style

- Rust 2024 edition
- Format code before every commit (`cargo fmt`)
- Prefer functional style over imperative style

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
