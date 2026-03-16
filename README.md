# Cursus

A release management CLI for software. Cursus provides a structured workflow
for recording changes, bumping semantic versions, generating changelogs, and
publishing packages to registries.

Designed to truly run anywhere, and distributed as static binaries for most major
platforms. If rust can compile to it, and it's not already available then please
open a request if you need it to run a new platform.

**[Full documentation](https://zantarix.github.io/cursus/)**

## Quick start

```bash
# Initialise Cursus in your repository
cursus init

# Record a change (interactive TUI)
cursus

# Or non-interactively
cursus change -t minor -m "Add user authentication"

# When ready to release
cursus prepare

# Publish to registries
cursus publish
```

See the [getting started guide](https://zantarix.github.io/cursus/getting-started/installation/)
for installation options and a full walkthrough.

## Overview

Cursus breaks the release process into three distinct steps:

1. **Record changes** — developers describe what changed and how it affects the
   version (`major`, `minor`, or `patch`)
2. **Prepare** — Cursus aggregates pending changes, bumps versions, generates
   changelogs, and updates lock files
3. **Publish** — packages are published to registries in dependency order

Each step can be run interactively (TUI) or non-interactively for CI/CD
pipelines.

## Documentation

- [Installation](https://zantarix.github.io/cursus/getting-started/installation/)
- [CLI Reference](https://zantarix.github.io/cursus/reference/cli/)
- [Configuration](https://zantarix.github.io/cursus/reference/configuration/)
- [CI Integration](https://zantarix.github.io/cursus/guides/ci-integration/)
- [Contributing](https://zantarix.github.io/cursus/contributing/development-setup/)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines,
or the full [contributing guide](https://zantarix.github.io/cursus/contributing/development-setup/)
on the documentation site.

## License

[Mozilla Public License 2.0](LICENSE.md)
