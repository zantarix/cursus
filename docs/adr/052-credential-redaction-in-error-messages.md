# ADR-052: Redact Credentials from Subprocess and API Error Messages

## Status

Accepted (2026-05-01)

## Context

Cursus invokes external tools (`git`, `cargo`, `npm`, `pnpm`, `yarn`) and external HTTP APIs (GitHub via `octocrab`) on behalf of the user. When those invocations fail, cursus surfaces the failure to the user by capturing the subprocess stderr or the HTTP response body and embedding the captured text into an `anyhow::Error` via `bail!` or `context()`. The error then propagates up to the binary's top-level handler, where it is rendered to the terminal, written to log files, and -- crucially in CI -- printed into GitHub Actions workflow summaries that may be retained indefinitely and viewed by anyone with read access to the repository.

This forwarding pipeline is unsafe whenever the failing operation was configured with credentials embedded in a URL. There are three concrete leak vectors today:

1. **Git remotes with embedded tokens.** [ADR-050](050-verified-release-commits-via-git-data-api.md) routes the prepare-step commit through the GitHub Git Data API, but ordinary push, fetch, and reset operations still go through the `git` binary against an HTTPS remote. CI workflows that run `cursus prepare` or `cursus publish` typically configure the remote as `https://x-access-token:${{ secrets.GITHUB_TOKEN }}@github.com/owner/repo.git` (or equivalent for a fine-grained PAT), because the `git` binary has no other way to authenticate over HTTPS in a non-interactive context. When `git push` fails -- protected-branch rejection, non-fast-forward, server-side hook -- the `git` binary echoes the full remote URL, including the userinfo component, into its stderr. Cursus currently captures that stderr verbatim and embeds it in the resulting `anyhow` error, which means the token surfaces in the workflow log and the workflow summary.

2. **Package registry URLs with embedded credentials.** Both `cargo publish` and `npm publish` (as well as `pnpm install` and `yarn install`) accept registry URLs configured via `.cargo/config.toml`, `.npmrc`, `.yarnrc.yml`, or environment variables. Self-hosted registries (Cloudsmith, JFrog Artifactory, Verdaccio, GitHub Packages) commonly embed credentials directly in the registry URL, e.g. `https://user:token@registry.example.com/`. When publish or install fails and the package manager echoes the URL in its diagnostic output, that output is captured into cursus error messages on the same pipeline as the git case.

3. **GitHub API response bodies.** `SignedCommitGit` (introduced by [ADR-050](050-verified-release-commits-via-git-data-api.md)) issues HTTP requests through `octocrab` to the GitHub Git Data API. On failure, the decorator includes the response body in the surfaced error so the user can see what the API actually said. While the GitHub API itself does not normally echo authorisation headers, error responses can include `Location` headers, redirect URLs, or proxy-injected diagnostic text that contains credentials, particularly when the request is routed through a corporate proxy or a self-hosted GitHub Enterprise instance that is misconfigured.

The risk is asymmetric: a single token leaked into a public CI log is treated as fully compromised the moment it is observed, requires manual rotation, and -- for `GITHUB_TOKEN` specifically -- can have a lifetime measured in hours, but the workflow summary that exposed it persists for as long as the run is retained. The blast radius is wide enough that "the user must remember to scrub error messages by hand" is not a credible mitigation.

Cursus already has two precedents for cross-cutting safety conventions enforced at the boundary where untrusted or sensitive data crosses an abstraction:

- **`ref_format`** (`packages/cursus/src/git/ref_format.rs`) requires every `GitWorkdir` method that accepts a caller-supplied branch, tag, or revision string to validate the string before passing it to the `git` binary. This prevents a hostile changeset filename or branch name from being interpreted as a `git` flag.
- **`name_validation`** (`packages/cursus/src/package_manager/name_validation.rs`) requires every `PackageManagerAdapter::enumerate_projects` implementation to validate manifest-sourced package names before they enter `ProjectInfo`.

Both conventions are documented as a contract that any new method or adapter must honour. Without an equivalent recorded decision for credential redaction, a contributor adding the next git operation, package manager adapter, or API caller has no authoritative signal that they must apply the redaction step, and the safety property regresses silently the next time the surface area grows.

The redaction primitive itself already exists on the `security` branch as `pub fn redact_credentials(s: &str) -> Cow<'_, str>` in `packages/cursus/src/redact.rs`. It strips the userinfo component from URLs (`scheme://[REDACTED]@host`) using RFC 3986's last-`@` split rule for the authority, and stops authority scanning at any of `/`, `?`, `#`, or whitespace. Stopping at whitespace is load-bearing: subprocess stderr is frequently multi-line, and a URL on one line without a trailing slash must not "swallow" the next line's authority -- if it did, any second URL on a subsequent line would not be redacted. The function returns `Cow::Borrowed` when the input contains no credentials, so the common no-leak case allocates nothing.

What remains is to commit to applying that primitive at every site where untrusted external text reaches an `anyhow` error, and to make that obligation a recorded contract that future contributors are expected to honour.

## Decision

We will apply `redact_credentials` at every point in the cursus codebase where subprocess stderr text or external API response text is embedded into an `anyhow::Error` (whether via `bail!`, `context()`, `with_context()`, or direct `anyhow!` construction). This obligation extends to all current and future code in the following surfaces:

- `GitWorkdir` (and any future `Git` trait implementation that shells out to a subprocess) -- every site that captures stderr from a failed `git` invocation must pass the captured text through `redact_credentials` before embedding it in the error.
- `PackageManagerAdapter::publish`, `update_lock_file`, and any other adapter method that captures subprocess stderr -- every such site must redact before embedding. This applies to the existing Cargo and npm/yarn/pnpm adapters and to any adapter added in the future.
- `SignedCommitGit` and any other `CodeForgeClient` consumer that includes an HTTP response body in an error -- the body text must be passed through `redact_credentials` before embedding.

The redaction step is unconditional: it runs in production, in tests, in dry-run, and regardless of whether any credentials are believed to be present in the input. Because `redact_credentials` is `Cow`-returning and credential-free input is a borrowed pass-through, the no-leak case has no measurable cost.

The contract is recorded in this ADR and in the `redact` module-level documentation in `packages/cursus/src/redact.rs`, which states the redaction primitive's purpose and the userinfo-stripping behaviour that callers depend on. Future contributors implementing alternative `Git`, `PackageManagerAdapter`, or `CodeForgeClient` implementations are expected to honour the contract on the same footing as the `ref_format` and `name_validation` conventions, both of which are likewise enforced by convention rather than by the type system.

Redaction is applied at the call site that constructs the `anyhow` error -- not at a lower layer (e.g., not inside `CommandRunner` itself, not inside `octocrab`). The call site is the right boundary because:

- It is the boundary where free-form external text becomes a user-visible message. Below that boundary the text is internal data that may legitimately contain URLs (e.g., a `git remote -v` output captured for debugging within a function that does not bail).
- Lower-layer redaction would force every consumer of `CommandRunner` output to re-derive whether the text is internal or user-visible, and would require a parallel "raw" channel for the legitimate cases.
- The `ref_format` and `name_validation` conventions follow the same boundary rule: validation happens at the trait method that owns the contract, not inside `CommandRunner`.

We will not attempt to scrub credentials from process stdout/stderr that is streamed live to the user terminal via `run_streaming` ([ADR-046](046-streaming-command-execution.md)). Live streaming bypasses cursus entirely -- the child process inherits the parent's stdout and stderr file descriptors -- and there is no point at which cursus sees the bytes. The `run_streaming` call sites are user-configurable shell commands (`github.build_command`, `npm.lock_command`); the user has explicit control over what those commands print, and the existing CI guidance for masking secrets via `::add-mask::` continues to apply. This decision narrows the contract to captured-output paths, where cursus is the agent that decides what reaches the error message.

## Consequences

### Positive

- Failed git, cargo, npm/pnpm/yarn, and GitHub API operations no longer leak credentials embedded in remote or registry URLs into error messages, terminal output, log files, or CI workflow summaries. This closes a credential-exposure path that exists today on every cursus install in CI.
- The contract gives future contributors an authoritative answer to "do I need to redact this?". The answer is "yes, at every error site that includes external text", and the convention has the same standing as `ref_format` and `name_validation`.
- The redaction primitive is centralised in one module with a focused test suite. A bug in the redaction algorithm is fixed in one place, not in eighteen call sites.
- Because `redact_credentials` returns `Cow<'_, str>` and borrows when the input is credential-free, the redaction step has no measurable runtime cost on the no-leak path -- which is the overwhelming majority of error paths, since most failures (file not found, dependency cycle, version conflict) do not include URLs at all.
- Multi-URL stderr (e.g. a `git fetch` that lists multiple remote URLs in its diagnostic output) is fully redacted; the authority-termination rule prevents the first URL from swallowing the rest of the stderr stream.

### Negative

- Redaction is enforced by convention, not by the type system. A contributor adding a new git operation, package manager adapter, or GitHub API call who does not know about this ADR can silently regress the safety property. This is the same trade-off accepted by `ref_format` and `name_validation`. We mitigate it by documenting the obligation on the relevant trait surfaces and in `redact.rs`, but ultimately the convention requires reviewer attention.
- Redacted error messages lose information that could be useful for debugging (e.g., "is the username portion of the URL correct?"). The trade is intentional: a slightly less precise error message is acceptable; a leaked token is not. Operators who genuinely need to see the unredacted URL can reproduce the failure interactively against a remote configured without embedded credentials.
- The redaction algorithm is a hand-rolled URL-authority parser, not a full RFC 3986 implementation. It handles the cases that arise in practice (HTTPS URLs with `user:pass@host`, `token@host`, and authorities terminated by `/`, `?`, `#`, or whitespace) but is not guaranteed to redact every conceivable URL syntax. The algorithm is documented and unit-tested; future leak vectors discovered in practice should be added as test cases and addressed in `redact_credentials`.
- The streaming path (`run_streaming`) is explicitly out of scope. Operators who configure `github.build_command` or `npm.lock_command` to invoke a tool that echoes credentials to stderr will see those credentials on the terminal and in the CI log. This is consistent with the documented contract for those hooks but is a real residual risk that this ADR does not address.

### Neutral

- The contract applies symmetrically to all three error-surfacing categories (subprocess stderr, GitHub API bodies, future external text sources) and to all current and future implementations of `Git`, `PackageManagerAdapter`, and `CodeForgeClient`. There is no exemption for "internal" or "trusted" subprocess invocations -- the per-call-site cost of calling `redact_credentials` is low enough that a uniform rule is simpler and safer than a per-call-site judgement call.
- The decision does not change the existing late-guard dry-run pattern from [ADR-017](017-late-guard-dry-run-pattern.md). Dry-run paths still capture and surface stderr from the read-only subprocess invocations that run during dry-run; redaction applies to those error messages on the same terms as the production path.
- The decision does not introduce a new abstraction or trait. `redact_credentials` is a free function in `packages/cursus/src/redact.rs`, called explicitly by each site that needs it. This keeps the cost of the convention to a one-line wrapping of the captured text and avoids a decorator-like indirection that would obscure where redaction happens.
- The `redact` module is a new public surface in the `cursus` library crate. Consumers of the library (currently only `cursus-bin`) gain access to `redact_credentials` and may use it for their own error-message scrubbing if appropriate.
- Most CI systems (e.g. GitHub Actions) maintain their own secret-redaction layer that automatically masks any registered secrets from log output, and cursus's redaction is complementary to that layer rather than an alternative. CI-side redaction operates only on secrets explicitly registered with the CI system, relies on exact string matching, and does not cover all token forms -- tokens derived at runtime, tokens for registries whose credentials are not registered as workflow secrets, and tokens surfaced in contexts outside the CI log (developer terminals, local log files, error messages copied into bug reports) are not protected by it. Cursus's responsibility to redact at the source is therefore unchanged: the two layers compose, and a leak that bypasses one may be caught by the other.

## Alternatives Considered

### Redact inside `CommandRunner` and `octocrab` wrappers

Apply `redact_credentials` automatically to the stderr returned by every `CommandRunner` mutating method, and to every HTTP response body captured by the `octocrab` wrapper. This was rejected because `CommandRunner` and the GitHub HTTP client return text that is not always destined for a user-visible error message -- some callers parse the output, some log it at debug level, some discard it. Forcing redaction at the lower layer would either corrupt those internal uses (the `[REDACTED]@host` token is no longer a parseable URL) or require a parallel "raw" channel for the legitimate cases, doubling the trait surface. Redacting at the call site that constructs the `anyhow` error is simpler and aligns with the `ref_format` / `name_validation` precedent of validating at the trait boundary that owns the contract.

### Strip credentials from remote URLs at configuration time

Detect remote URLs that contain embedded credentials when cursus first loads them (from `git remote get-url`, from `.cargo/config.toml`, from `.npmrc`) and rewrite them to use a credential helper or environment-injected credentials before any subprocess is invoked. This was rejected for two reasons. First, it does not address the GitHub API response-body leak, which has nothing to do with locally-configured URLs. Second, cursus does not own the user's git or package-manager configuration; rewriting credential storage on the user's behalf is a significantly larger commitment than scrubbing error messages. The credential URL patterns used in CI (e.g., `x-access-token:${GITHUB_TOKEN}@github.com`) are also the canonical, documented way to authenticate `git` against GitHub in non-interactive contexts; cursus rewriting them would break workflows that work today.

### Use a third-party redaction library

Pull in a crate such as `secrecy` or a regex-based scrubber. This was rejected because the redaction need here is narrow (one URL-authority shape, applied in one direction, only at error-construction time) and the existing implementation is a few dozen lines with comprehensive unit tests. A third-party dependency adds a transitive supply-chain surface (relevant in light of [ADR-051](051-bundle-sigstore-deps-via-workspace-removal.md)'s recent attention to transitive-dep risk) for a problem that is already solved by ten-line stdlib code.

### Apply redaction only to git operations

Limit the contract to `GitWorkdir` stderr capture, on the grounds that the most exposed credential is the GitHub token in the git remote URL. This was rejected because the same threat model applies to `cargo publish` and `npm publish` against any registry that uses URL-embedded credentials (Cloudsmith, Artifactory, Verdaccio, GitHub Packages), and to GitHub API response bodies that may include redirect or proxy-injected URLs. The cost of broadening the contract is one function call per error site; the cost of narrowing it is a credential leak the day the first user configures Cursus against a private registry.

### Enforce redaction via a wrapper error type

Introduce a `RedactedError` newtype that wraps `anyhow::Error` and applies redaction in its `Display` impl, then ban direct `anyhow::Error` construction from external text. This was rejected because it spreads a typed obligation across the entire codebase to solve a problem that is local to a few dozen call sites, and because the redaction must happen at error-*construction* time (before the message becomes part of the error chain), not at *display* time -- by display time, the unredacted text has already been preserved in the chain via `context()` and may be re-emitted by any consumer that walks `error.chain()`. Display-time redaction would also need to be applied identically by every renderer (terminal, log file, JSON output), whereas construction-time redaction guarantees the unredacted bytes never enter the error chain in the first place.
