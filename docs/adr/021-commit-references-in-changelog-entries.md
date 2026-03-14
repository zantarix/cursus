# ADR-021: Add Commit References to Changelog Entries

## Status

Accepted

## Context

Cursus's `prepare` command reads changeset files from `.cursus/`, aggregates them per-package, bumps versions, generates changelog entries, and deletes the consumed changeset files. The generated changelog entries currently contain only the change type category and the user-authored description message, for example:

```markdown
### Features

- Added widget
- Improved performance of batch processing
```

This provides no traceability back to the code change that introduced each entry. Users reviewing a changelog -- whether in a CHANGELOG.md file, a GitHub Release, or a release PR body -- cannot easily navigate from a changelog bullet to the commit or pull request that produced it. This is a common need during incident investigation, release auditing, and code archaeology.

Changeset files arrive in `.cursus/` via normal git workflow: a contributor creates a changeset on a branch, and it lands on the trunk branch through one of three merge strategies: rebase merge, squash merge, or regular merge commit. The commit on the trunk branch that *introduced* a changeset file is the natural anchor point for traceability, since it represents the moment the change was accepted into the mainline. Depending on the merge strategy, this may be a rebased commit, a squash commit, or a merge commit -- but in all cases it is the first-parent commit on the trunk that introduced the file.

GitHub and other forges automatically link short commit hashes (e.g., `abc1234`) and PR references (e.g., `#123`) when rendering Markdown. This means changelog entries can include lightweight plain-text references that become clickable links with zero URL construction, removing any dependency on knowing the forge URL or having `[github].enabled` configured.

The challenge is identifying the right commit for each changeset file across all merge strategies, extracting PR numbers from commit messages when available, handling cases where git history is unavailable, and integrating references into the existing changelog formatting pipeline without disrupting the current data flow through `Changeset`, `Changelog`, and the `prepare` orchestration.

## Decision

We will enrich changelog entries with commit and pull request references by resolving the git commit that introduced each changeset file at `prepare` time.

**Commit discovery.** For each changeset file path, we will run `git log --first-parent --diff-filter=A --format=%H -- <path>` to find the first-parent commit that added the file. The combination of these flags handles all three merge strategies correctly:

- **Rebase merge**: The rebased commit sits directly on the trunk. It is a first-parent commit and `--diff-filter=A` matches it.
- **Squash merge**: The squash commit sits directly on the trunk. Same behavior as rebase.
- **Regular merge commit**: Without `--first-parent`, git would walk into the feature branch and return the original commit that created the file there. With `--first-parent`, git only traverses the mainline, so the merge commit is the first point where the file appears as added. This correctly returns the merge commit SHA rather than the feature branch commit.

The `--diff-filter=A` restriction to addition-only commits also makes this robust against subsequent modifications from partial consumption ([ADR-010](010-scoped-release-changeset-consumption.md)), since changeset rewrites register as modifications, not additions.

**PR number extraction.** After resolving the introducing commit, we will retrieve its commit message via `git log -1 --format=%s <sha>` and attempt to extract a pull request number by matching against an ordered list of regex patterns:

1. `\(#(\d+)\)` -- squash-merge default format, e.g. `Add feature (#123)`
2. `Merge pull request #(\d+)` -- regular merge commits

The first matching pattern wins. The pattern list will be defined as a code-level constant, making it straightforward to extend for additional forge conventions without requiring user configuration. If no pattern matches -- as is typical for rebase merges, which preserve the original commit message without a PR reference -- the PR number is simply absent.

**Changelog rendering format.** Each changelog bullet that has a resolved commit reference will append a suffix after the message text:

- With PR number: `- Added widget [abc1234] via #123`
- Without PR number: `- Added widget [abc1234]`

The short hash will be the first 7 characters of the full SHA. Square brackets around the hash and the `#` prefix on the PR number are standard conventions that GitHub (and most forges) autolink into clickable references when rendering Markdown. No full URLs are constructed, and there is no dependency on `[github].enabled` or any forge-specific configuration.

**Data flow changes.** The `Changelog` struct's `changes` field currently holds `Vec<(ChangeType, Option<String>)>`. We will extend each entry to carry an optional commit reference alongside the change type and message. This reference will contain the short hash and optional PR number. The changelog formatting logic will append the reference suffix when present.

The commit resolution will occur during the aggregation phase in `prepare`, after `Changeset::read_all()` returns the list of changeset file paths. For each changeset path, the prepare command will query git for the introducing commit. The resolved references will flow through to `Changelog::new()` alongside the existing change type and message data.

**Graceful degradation.** Commit reference resolution is best-effort and never fatal. The log level when a reference cannot be resolved depends on whether the user has opted into git:

- **Git not enabled or not available** (`[git].enabled = false`, or no git repository): The commit reference is `None` and a `debug!`-level message notes the skip. This is silent by default, consistent with Cursus's opt-in git philosophy.
- **Git enabled but commit lookup fails** (`[git].enabled = true` and the `git log` command fails or returns no result): The commit reference is `None` and a `warn!`-level message alerts the user. Since the user has explicitly opted into git integration, a failure in an expected git operation warrants visible feedback.
- **PR number not found in commit message**: Always `debug!`-level regardless of git configuration. This is the normal case for rebase merges and is not worth warning about.

In all cases the changelog entry renders without a suffix -- identical to current behavior -- and `prepare` continues normally.

**Dry-run behavior.** Commit reference resolution is a read-only git operation (`git log`). Per [ADR-008](008-dry-run-local-only-guarantee.md) and [ADR-017](017-late-guard-dry-run-pattern.md), read-only local operations run unconditionally regardless of `--dry-run`. The resolved references will appear in dry-run changelog output, giving users an accurate preview of what the real run would produce.

## Consequences

### Positive

- Changelog entries gain traceability to the originating commit and pull request, making release auditing and incident investigation substantially easier.
- The forge-autolink approach (`[abc1234]`, `#123`) works without any configuration and produces clickable links on GitHub, GitLab, and other platforms that support these conventions.
- No new configuration fields or CLI flags are required. The feature activates automatically when git history is available.
- Dry-run output includes references, so users can verify traceability before committing to a release.
- All three GitHub merge strategies (rebase, squash, regular merge) are handled correctly by a single `git log` invocation with `--first-parent --diff-filter=A`.

### Negative

- Each changeset file requires one or two `git log` invocations during `prepare` (one for the commit SHA, one for the commit message if the SHA is found), adding subprocess overhead proportional to the number of pending changesets. For repositories with many pending changesets this may be noticeable, though `git log` on a single file path is fast even in large repositories.
- The PR number extraction is tied to GitHub's commit message conventions. Other forges (GitLab, Bitbucket) use different patterns. While the pattern list is extensible in code, it will silently produce no PR number for non-GitHub workflows until new patterns are added.
- The `changes` data structure in `Changelog` becomes more complex, carrying an optional reference alongside each entry. Existing tests that construct `Changelog` values will need updating to accommodate the new field.

### Neutral

- Changelog entries without resolvable references (no git, uncommitted files, non-GitHub forges for PR numbers) render identically to current output. This is a purely additive change with no regressions for existing users.
- The `--diff-filter=A` approach means that if a changeset file is deleted and re-created with the same path (an unlikely but possible scenario), the log would return the most recent addition commit, which is the correct behavior.
- The `format_sections()` method on `Changelog` gains awareness of references, but the overall formatting pipeline (category grouping, heading generation, continuation-line indentation) is unchanged.
- Rebase-merge workflows typically produce no PR number in the commit message, so those entries will show only the commit hash. This is expected and still valuable for traceability.

## Alternatives Considered

### Use HEAD at prepare time as the commit reference

Using the current HEAD commit when `prepare` runs would give every changelog entry in a release the same commit reference. This was rejected because it provides no per-entry granularity -- the whole point of traceability is linking each changelog bullet to the specific change that introduced it. HEAD at prepare time is the "release commit," not the "change commit."

### Use `git log` without `--first-parent` or `--diff-filter=A`

Running `git log -1 -- <path>` returns the most recent commit that touched the file, not necessarily the one that introduced it on the trunk. This has two problems. First, for partially consumed changesets ([ADR-010](010-scoped-release-changeset-consumption.md)), the rewrite commit becomes the most recent modification, attributing the remaining entries to the wrong commit. Second, without `--first-parent`, regular merge commits are traversed into the feature branch, returning the original feature branch commit rather than the merge commit on the trunk. The combination of `--first-parent` and `--diff-filter=A` avoids both issues.

### Construct full URLs to commits and PRs

Instead of relying on forge autolinking, we could construct full URLs like `[abc1234](https://github.com/owner/repo/commit/abc1234)`. This was rejected because it would require resolving the GitHub repository at prepare time (via `GitHubRepo::resolve`), creating a dependency on `[github].enabled` or a configured remote. The plain-text reference approach works universally, keeps the changelog portable across rendering contexts, and produces shorter, less noisy Markdown.

### Make PR extraction patterns user-configurable

Exposing the regex patterns in `.cursus/config.toml` would allow users to support arbitrary forge conventions. This was rejected as premature: the two GitHub patterns cover the dominant use case, the pattern list is trivially extensible in code, and adding configuration would increase the surface area of the config schema (which uses `deny_unknown_fields`) for a feature most users will never need to customize. A future ADR can add configurability if demand arises.
