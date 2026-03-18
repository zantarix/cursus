# ADR-034: Use fluent-templates for Compile-Time Embedded Localisation

## Status

Accepted

## Context

Cursus needs internationalisation (i18n) support across three surface areas: TUI wizard text, CLI output messages, and comments generated in init templates. The project distributes as a single static binary for seven cross-compilation targets ([ADR-000](000-founding-constraints.md), [ADR-022](022-distribution-strategy.md)), which rules out any approach that requires runtime file I/O to load translation resources. Translation strings must be embedded in the binary at compile time and retrievable at runtime through a straightforward API.

The library crate must remain environment-unaware per [ADR-030](030-bin-lib-crate-separation.md) -- it must not read `LANG`, `LC_ALL`, or any other environment variable directly. Locale resolution belongs in the binary entrypoint, with the resolved locale passed into the library via the `Env` struct.

The TUI layer uses `const` values for screen labels and button text in several traits and structs. Introducing translated strings -- which are runtime-computed `String` values -- requires changes to these interfaces.

## Decision

We will use `fluent-templates` (specifically its `static_loader!` macro) with `unic-langid` for locale identification and `sys-locale` for cross-platform system locale detection. Dependencies will be pinned with exact versions: `fluent-templates = "=0.13.3"`, `unic-langid = "=0.9.6"`, `sys-locale = "=0.3.2"`.

### Static embedding

The `static_loader!` macro will embed all `.ftl` files from the `locales/` directory tree at compile time as `&'static str` data. No runtime file I/O occurs, preserving the single-binary property.

### Process-global locale

A single `LazyLock<RwLock<LanguageIdentifier>>` will hold the active locale, set once at startup via a `set_locale()` function. As a CLI tool with no concurrent locale requirements, per-thread locale support is unnecessary.

### Library remains locale-unaware of environment variables

The `Env` struct will carry a `locale: String` field. The binary entrypoint (`main.rs`) will resolve the locale from the environment and system, then pass it into the library. The library will never read `LANG`, `LC_*`, or any locale-related environment variable directly, consistent with [ADR-030](030-bin-lib-crate-separation.md).

### Locale detection priority

Locale resolution will follow this precedence: `CURSUS_LOCALE` environment variable, then `sys-locale` crate detection (which uses the Win32 API on Windows, avoiding reliance on `LANG`/`LC_ALL` which do not exist there), then `"en"` as the final fallback.

### DEFAULT_LOCALE constant

A `pub const DEFAULT_LOCALE: &str = "en"` in `locale.rs` will be shared between `Env::new()` defaults and the `main.rs` fallback logic, preventing drift between the two.

### t! macro

A `#[macro_export]` macro named `t!` will wrap the `Loader::lookup` and `Loader::lookup_with_args` trait methods. Each macro arm will include `use ::fluent_templates::Loader as _` internally for trait method resolution.

### TUI trait changes

The `ButtonScreen` trait's `const QUESTION: &'static str` associated constant will become a `fn question(&self) -> String` method, since translation lookup produces a runtime `String`. Similarly, `ButtonDef`'s `label` field will change from `&'a str` to `String` because translated labels are dynamically generated.

### English as source of truth

Files in `locales/en/*.ftl` will be the canonical translation source. Other locales will fall back to English for any missing keys via Fluent's built-in locale negotiation.

### Test isolation for locale

Because the process-global locale (`LazyLock<RwLock<LanguageIdentifier>>`) is mutated by `set_locale()` and Cargo runs unit tests in parallel within the same process, unit tests must pin the locale to `"en"` via `set_locale("en")` before any `t!()` call and must not assert on locale-varying output. Tests that specifically verify locale-switching behaviour must be written as dedicated integration tests running in a subprocess, so each gets its own process-global state.

## Consequences

### Positive

- All translation strings ship inside the binary with zero runtime file dependencies, fully preserving the static single-binary distribution model
- Adding a new locale requires only creating a `locales/<lang>/` directory with `.ftl` files and recompiling -- no configuration changes needed
- The `t!()` macro provides a concise, grep-friendly API for all translated strings
- `sys-locale` handles cross-platform locale detection including Windows (Win32 API), avoiding the Unix-only `LANG`/`LC_ALL` assumption
- `CURSUS_LOCALE` gives users an explicit override independent of system locale settings
- The `DEFAULT_LOCALE` constant eliminates the risk of the library and binary disagreeing on the fallback locale

### Negative

- Three new dependencies are introduced (`fluent-templates`, `unic-langid`, `sys-locale`), increasing the dependency tree and requiring ongoing maintenance of pinned versions
- All TUI strings that were `const &'static str` must now go through `t!()` and return `String`, adding allocation overhead for every label lookup
- The `ButtonScreen` trait change (`const QUESTION` to `fn question(&self) -> String`) is a breaking change for any trait implementors, though none exist outside the crate today
- `ButtonDef.label` moving from borrowed `&'a str` to owned `String` increases allocation frequency in TUI rendering paths

### Neutral

- The process-global locale model is sufficient for a CLI tool but would need rethinking if Cursus ever became a library consumed by concurrent callers
- Fluent's message syntax (`.ftl` files) is more expressive than simple key-value formats, which is useful for pluralisation and variable substitution but adds a learning curve for contributors adding translations
- The `RwLock` on the global locale identifier adds negligible overhead since it is written once at startup and read infrequently thereafter

## Alternatives Considered

### rust-i18n

`rust-i18n` generates translation code via proc-macro and supports YAML/JSON/TOML source formats. It was rejected because its code-generation approach is less flexible for Fluent's variable substitution and pluralisation syntax, which Cursus benefits from for TUI messages that include counts and package names.

### Runtime file loading (fluent-rs directly)

Using the `fluent` crate directly with runtime `.ftl` file loading would provide maximum flexibility for adding locales without recompilation. This was rejected because it fundamentally conflicts with the single-static-binary requirement ([ADR-000](000-founding-constraints.md)). Users would need to install translation files alongside the binary, breaking the zero-dependency distribution model.

### gettext / .po files

The traditional `gettext` approach with `.po`/`.mo` files is well-established in the wider ecosystem but was rejected for three reasons: it requires a separate toolchain (`msgfmt`, `xgettext`) outside the Rust build system, `.mo` files are typically loaded at runtime from the filesystem, and the Rust `gettext` bindings are less actively maintained than the Fluent ecosystem.
