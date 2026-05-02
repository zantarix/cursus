+++
cursus = "minor"
+++

Fixes a gap where re-running `cursus publish` after a partial failure would not create missing git tags or GitHub Releases for packages already published to a registry. All three publish stages (registry, git tag, GitHub Release) are now idempotent: re-running safely completes any stage that did not finish in a prior run. If a draft GitHub Release already exists for a tag, cursus reports a clear error and exits non-zero instead of silently failing.
