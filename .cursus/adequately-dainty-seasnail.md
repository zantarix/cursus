+++
cursus = "minor"
cursus-bin = "minor"
+++

Adds GitLab verified release commits. When running on GitLab CI ≥18.10 with a token, the prepare commit is routed through the GitLab commits API and appears as **Verified** in the GitLab UI — no signing key custody required. Reuses the existing `[git].signed_commits` config knob (`"auto"`, `"force"`, `"off"`); `"auto"` engages whenever `GITLAB_CI=true` and a token is present. See the [GitLab CI guide](https://zantarix.github.io/cursus/guides/ci-integration/gitlab/#verified-commits) for setup.
