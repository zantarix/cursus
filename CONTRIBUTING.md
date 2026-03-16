# Contributing to Cursus

Full contributing documentation is available on the
[documentation site](https://zantarix.github.io/cursus/contributing/development-setup/).

## Quick setup

This project uses [Nix flakes](https://nixos.wiki/wiki/Flakes) and
[direnv](https://direnv.net/) for a reproducible development environment. The
flake supports x86_64-linux, aarch64-linux, and aarch64-darwin.

A [dev container](https://containers.dev) is also available in `.devcontainer/`
for VS Code, the devcontainer CLI, and
[GitHub Codespaces](https://codespaces.new/zantarix/cursus) (zero local setup).

## Build commands

```bash
cargo build                    # Build the project
cargo test                     # Run all tests
cargo clippy                   # Lint
cargo fmt                      # Format
cargo make coverage            # Check coverage thresholds
```

## Architecture decisions

Significant design choices are documented as Architecture Decision Records in
[`docs/adr/`](docs/adr/).
