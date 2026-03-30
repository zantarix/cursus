# ADR-040: Strip Git Trailers from Conventional Commit Body During Parsing

## Status

Proposed

## Context

[ADR-025](025-auto-changeset-from-conventional-commit.md) introduced `cursus change --auto`, which derives changeset files from the single Conventional Commit on a branch. The changeset message is assembled from the commit's description and body: when a body is present, the result is `"{description}\n\n{body}"`.

Git commits frequently carry RFC 822-style trailers (also called "git interpret-trailers" format) in a contiguous block at the tail of the commit message body. These trailers take the form `Key: Value` or `Key #Value` and serve a variety of machine-readable purposes:

- **Authorship and sign-off**: `Signed-off-by:`, `Co-authored-by:`, `Reviewed-by:`, `Acked-by:`
- **Issue linking**: `Fixes #123`, `Closes #456`, `Refs #789`
- **CI metadata**: `Change-Id:`, `Ticket:`, `Cherry-picked-from:`
- **Conventional Commits semantics**: `BREAKING CHANGE:`, `BREAKING-CHANGE:`

Today, the `conventional_commit::parse()` function returns the entire text after the first blank line as the `body` field. This means all trailers are included verbatim in the changeset message and ultimately appear in the generated CHANGELOG. A Renovate-generated commit like:

```
fix(deps): update dependency foo to v2.1.0

Updated foo from v2.0.0 to v2.1.0 for security patch.

Signed-off-by: renovate[bot] <renovate[bot]@users.noreply.github.com>
Co-authored-by: renovate[bot] <29139614+renovate[bot]@users.noreply.github.com>
```

produces a changeset message containing the `Signed-off-by` and `Co-authored-by` lines, which then leak into the CHANGELOG. These trailers are metadata about the commit, not user-facing release information.

The Conventional Commits specification explicitly defines footers as a distinct section from the body. The git trailer convention (as implemented by `git interpret-trailers`) defines a trailer block as the contiguous block of `Key: Value` or `Key #Value` lines at the very end of the message, optionally preceded by a blank line separator. Stripping this block aligns the parser with the spec's intent that footers are structured metadata, not prose.

## Decision

We will strip RFC 822-style git trailers from the commit body in `conventional_commit::parse()`, so that all downstream consumers -- including changeset derivation and changelog generation -- receive clean prose text without metadata trailers.

### Trailer detection

A trailer block is the maximal contiguous run of non-empty lines at the tail of the rest-of-message (everything after the header's `\n\n` split) where every line in that run matches the git trailer format:

- `Token: Value` -- a word token (letters, digits, hyphens) followed by `:` and arbitrary text
- `Token #Value` -- same token followed by `#` and arbitrary text (used by `Fixes #123`, `Closes #456`)
- `BREAKING CHANGE: Value` and `BREAKING-CHANGE: Value` -- the two multi-word tokens explicitly allowed by the Conventional Commits specification

If the entire rest-of-message consists of trailers (no prose body at all), the body is `None`. If prose precedes the trailer block, only the trailer block is removed. Blank lines between the prose body and the trailer block are trimmed.

### Parse-time stripping

Stripping happens inside `conventional_commit::parse()` before constructing the `ConventionalCommit` struct. This is the single point where commit messages enter the domain model, so stripping here guarantees every consumer sees clean text without needing per-site filtering.

### BREAKING CHANGE trailers

The `BREAKING CHANGE:` and `BREAKING-CHANGE:` trailers are already scanned for their semantic meaning (setting the `breaking` flag) before the body is produced. After this change, these trailers will continue to be detected for their semantic purpose and will additionally be stripped from the body text. The two concerns -- semantic extraction and text cleaning -- are handled in sequence within the same function.

### GitHub keyword trailers

Trailers like `Fixes #123`, `Closes #456`, and `Refs #789` are issue-linking directives consumed by the forge. They have no place in a changelog entry. Since they conform to the `Token #Value` trailer format, they will be stripped along with all other trailers. Issue references that appear inline within prose body text (e.g., "This resolves the crash reported in #123") are not trailers and will be preserved.

### No allowlist or blocklist

The stripping logic does not maintain a hardcoded list of known trailer keys. Any line matching the structural trailer format within the contiguous tail block is stripped. This is forward-compatible with custom trailers (e.g., `Change-Id:`, `Ticket:`, organisation-specific keys) without requiring parser updates.

## Consequences

### Positive

- Changelog entries derived from `--auto` no longer contain `Signed-off-by`, `Co-authored-by`, or other commit metadata that is meaningless to end users
- GitHub keyword trailers (`Fixes`, `Closes`) are removed from changelogs while still being processed by the forge on the commit itself
- The parser aligns with the Conventional Commits specification's distinction between body and footer sections
- Forward-compatible with any trailer key, including custom organisation-specific trailers, without parser changes
- Single-point enforcement: every consumer of `ConventionalCommit` benefits without per-site awareness of trailers

### Negative

- If a user intentionally writes prose that happens to look like a trailer at the tail of their commit body (e.g., a line reading `Example: some value` as the last line), it will be stripped. This is an unlikely edge case and matches `git interpret-trailers`' own behaviour
- The trailer-format regex adds a small amount of complexity to the parser, though it remains straightforward pattern matching

### Neutral

- The `ConventionalCommit` struct's `body` field type (`Option<String>`) does not change; only its contents are affected
- Commits without trailers are completely unaffected -- the parser produces identical results
- The `breaking` flag detection continues to work because it runs before trailer stripping, scanning the raw rest-of-message text

## Alternatives Considered

### Strip only a hardcoded list of known trailers

Maintain an explicit list of trailer keys to strip (e.g., `Signed-off-by`, `Co-authored-by`, `Fixes`, `Closes`). This would give precise control over which trailers are removed.

Rejected because it requires ongoing maintenance as new trailer conventions emerge, misses organisation-specific trailers entirely, and the structural approach already matches git's own `interpret-trailers` behaviour. A hardcoded list provides a false sense of precision -- any well-formed trailer in the tail block is metadata, not prose.

### Strip trailers at changeset derivation time

Instead of modifying the parser, strip trailers in `derive_changeset()` within `model/changeset/mod.rs` where the body is assembled into the changeset message.

Rejected because it leaves the `ConventionalCommit` body field polluted for any other consumer, violating the principle that the domain model should represent clean, parsed data. If a future feature (e.g., PR body generation, release notes) reads `commit.body`, it would need its own trailer-stripping logic, leading to duplication.

### Strip trailers only when writing the changeset file

Defer stripping to the file-writing layer in changeset I/O, cleaning the message just before it hits disk.

Rejected for the same reason as the previous alternative: it pushes a parsing concern into an output layer, and any new output path would need to duplicate the logic. Parse-time stripping follows the established pattern where `parse()` is responsible for producing a fully normalized domain object.
